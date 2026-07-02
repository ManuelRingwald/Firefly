//! The [`OpenSkyPoller`]: polls the OpenSky Network REST API on a fixed
//! interval and converts each response into a batch of
//! [`Plot`](firefly_core::Plot)s for the tracker.

use std::time::Duration;

use firefly_core::Plot;
use tracing::{debug, warn};

use crate::{
    api::{parse_state, StatesResponse},
    auth::{self, AuthError, TokenCache},
    config::OpenSkyConfig,
};

/// Base URL of the OpenSky REST API.
const API_BASE: &str = "https://opensky-network.org/api/states/all";

/// Upper cap (seconds) for the exponential poll backoff. Growth from the base
/// poll interval is clamped here so a persistently rate-limited (429) or down feed
/// retries at most once every few minutes — but a single success drops it straight
/// back to the base cadence. The effective cap is never below the base interval.
const MAX_BACKOFF_SECS: u64 = 300;

/// Exponential backoff for the poll loop. A successful poll runs at the base
/// interval; each consecutive failure **doubles** the sleep (2·base, 4·base, 8·base
/// …) up to [`MAX_BACKOFF_SECS`], so a 429/rate-limited or down OpenSky feed is not
/// polled at the base cadence. A single success resets the sleep to the base.
///
/// Kept as a small, pure state machine so the growth/cap/reset logic is unit-
/// testable without running the (infinite) poll loop or any real HTTP.
#[derive(Debug)]
struct Backoff {
    base: Duration,
    cap: Duration,
    failures: u32,
}

impl Backoff {
    fn new(base: Duration) -> Self {
        let base = base.max(Duration::from_secs(1));
        // The cap must never sit below the base interval (an operator may configure
        // a base slower than MAX_BACKOFF_SECS), else backoff could shorten the poll.
        let cap = Duration::from_secs(MAX_BACKOFF_SECS.max(base.as_secs()));
        Self {
            base,
            cap,
            failures: 0,
        }
    }

    /// Reset after a successful poll; the next sleep is the base interval.
    fn reset(&mut self) -> Duration {
        self.failures = 0;
        self.base
    }

    /// Grow after a failed poll and return the (capped) next sleep. The first
    /// failure already backs off to 2·base so a rate limit is respected at once.
    fn fail(&mut self) -> Duration {
        self.failures = self.failures.saturating_add(1);
        // Clamp the shift so `base << shift` cannot overflow u64 on a long outage.
        let shift = self.failures.min(20);
        let secs = self.base.as_secs().saturating_mul(1u64 << shift);
        Duration::from_secs(secs).min(self.cap)
    }
}

/// Error returned by a single poll attempt.
#[derive(Debug)]
pub enum PollError {
    /// HTTP-layer failure (network unreachable, timeout, non-2xx response other
    /// than 429, …).
    Http(reqwest::Error),
    /// Failure obtaining an OAuth2 access token (ADR 0024).
    Auth(AuthError),
    /// OpenSky returned HTTP 429 (rate limit). Split out from [`Http`](Self::Http)
    /// so the poll loop can log it distinctly, count it, and back off — instead of
    /// hammering the API at the base cadence. Detected explicitly in `fetch_states`
    /// before `error_for_status`, which makes it trivially unit-testable.
    RateLimited,
}

impl PollError {
    /// True when this poll failed because OpenSky rate-limited the request
    /// (HTTP 429). Drives a distinct warning log and the
    /// `firefly_opensky_rate_limited_total` metric.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, PollError::RateLimited)
    }
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::Http(e) => write!(f, "HTTP error: {e}"),
            PollError::Auth(e) => write!(f, "{e}"),
            PollError::RateLimited => write!(f, "OpenSky rate limit (HTTP 429)"),
        }
    }
}

impl From<reqwest::Error> for PollError {
    fn from(e: reqwest::Error) -> Self {
        PollError::Http(e)
    }
}

impl From<AuthError> for PollError {
    fn from(e: AuthError) -> Self {
        PollError::Auth(e)
    }
}

/// Polls the OpenSky Network REST API periodically and converts each batch of
/// state vectors into [`Plot`]s that the tracker can consume directly.
///
/// # Usage
///
/// ```no_run
/// # use firefly_opensky::{OpenSkyConfig, OpenSkyPoller};
/// # #[tokio::main]
/// # async fn main() {
/// let config = OpenSkyConfig::from_env();
/// let poller = OpenSkyPoller::new(config);
/// poller.run(
///     |plots| {
///         for plot in plots {
///             // hand to tracker
///             drop(plot);
///         }
///     },
///     |e| eprintln!("poll error: {e}"),
/// ).await;
/// # }
/// ```
pub struct OpenSkyPoller {
    config: OpenSkyConfig,
    client: reqwest::Client,
    /// Cached OAuth2 bearer token (ADR 0024). Unused on the anonymous path.
    token_cache: TokenCache,
}

impl OpenSkyPoller {
    /// Create a new poller.  The `reqwest` HTTP client is built once and reused
    /// across all polls (connection pooling).
    pub fn new(config: OpenSkyConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("TLS backend should initialise successfully");
        Self {
            config,
            client,
            token_cache: TokenCache::new(),
        }
    }

    /// The OAuth2 credentials, when both are configured. Both must be present to
    /// authenticate; either alone (or neither) means anonymous polling.
    fn credentials(&self) -> Option<(&str, &str)> {
        match (
            self.config.client_id.as_deref(),
            self.config.client_secret.as_deref(),
        ) {
            (Some(id), Some(secret)) => Some((id, secret)),
            _ => None,
        }
    }

    /// Send one `states/all` request, attaching a bearer token when authenticated.
    async fn send_states(&self) -> Result<reqwest::Response, PollError> {
        let mut req = self.client.get(API_BASE).query(&[
            ("lamin", self.config.lat_min.to_string()),
            ("lomin", self.config.lon_min.to_string()),
            ("lamax", self.config.lat_max.to_string()),
            ("lomax", self.config.lon_max.to_string()),
        ]);
        if let Some((id, secret)) = self.credentials() {
            let token = self
                .token_cache
                .token(|| auth::fetch_token_http(&self.client, &self.config.token_url, id, secret))
                .await?;
            req = req.bearer_auth(token);
        }
        Ok(req.send().await?)
    }

    /// Fetch the state vectors, transparently recovering from a `401`: a cached
    /// token the server rejected (revoked / server-side expired) is invalidated
    /// and the request retried once with a fresh token. Anonymous requests do not
    /// retry (a `401` then is a genuine failure).
    async fn fetch_states(&self) -> Result<StatesResponse, PollError> {
        let resp = self.send_states().await?;
        let resp =
            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.credentials().is_some() {
                warn!("OpenSky returned 401 — refreshing OAuth2 token and retrying once");
                self.token_cache.invalidate().await;
                self.send_states().await?
            } else {
                resp
            };
        // Detect a rate limit explicitly (before error_for_status collapses every
        // non-2xx into an opaque Http error) so the poll loop can back off and the
        // 429 is countable/testable as its own error kind.
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(PollError::RateLimited);
        }
        Ok(resp.error_for_status()?.json().await?)
    }

    /// Perform a single poll: fetch the state vectors for the configured
    /// bounding box and return them as a [`Vec<Plot>`].
    ///
    /// Aircraft that are on the ground, or whose position is unknown, are
    /// silently dropped.
    pub async fn poll(&self) -> Result<Vec<Plot>, PollError> {
        let resp = self.fetch_states().await?;
        let timestamp = resp.time as f64;
        let sensor = self.config.sensor_id;

        let plots: Vec<Plot> = resp
            .states
            .unwrap_or_default()
            .iter()
            .filter_map(|s| parse_state(s, sensor, timestamp))
            .collect();

        debug!(
            count = plots.len(),
            lat_min = self.config.lat_min,
            lat_max = self.config.lat_max,
            lon_min = self.config.lon_min,
            lon_max = self.config.lon_max,
            "OpenSky poll succeeded"
        );

        Ok(plots)
    }

    /// Run the polling loop indefinitely, calling `on_plots` after each
    /// successful poll that returns at least one airborne aircraft, and
    /// `on_error` after each failed poll.
    ///
    /// Transient failures (network hiccup, rate-limit 429, OpenSky downtime) are
    /// logged and skipped rather than propagated; the loop **backs off
    /// exponentially** on consecutive failures (see [`Backoff`]) so a rate-limited
    /// or down feed is not hammered, and returns to the base interval on the first
    /// success. A 429 is logged distinctly. This method never returns.
    pub async fn run<F, E>(&self, mut on_plots: F, mut on_error: E)
    where
        F: FnMut(Vec<Plot>),
        E: FnMut(&PollError),
    {
        let mut backoff = Backoff::new(Duration::from_secs(self.config.poll_interval_secs));
        loop {
            let sleep = match self.poll().await {
                Ok(plots) if !plots.is_empty() => {
                    on_plots(plots);
                    backoff.reset()
                }
                Ok(_) => {
                    debug!("OpenSky poll returned no airborne aircraft");
                    backoff.reset()
                }
                Err(e) => {
                    if e.is_rate_limited() {
                        warn!("OpenSky rate-limited (HTTP 429) — backing off");
                    } else {
                        warn!("OpenSky poll failed: {e}");
                    }
                    on_error(&e);
                    backoff.fail()
                }
            };
            tokio::time::sleep(sleep).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(d: Duration) -> u64 {
        d.as_secs()
    }

    #[test]
    fn backoff_first_failure_doubles_the_base() {
        let mut b = Backoff::new(Duration::from_secs(10));
        assert_eq!(
            secs(b.fail()),
            20,
            "first failure backs off to 2·base at once"
        );
    }

    #[test]
    fn backoff_doubles_on_each_consecutive_failure() {
        let mut b = Backoff::new(Duration::from_secs(10));
        assert_eq!(secs(b.fail()), 20);
        assert_eq!(secs(b.fail()), 40);
        assert_eq!(secs(b.fail()), 80);
        assert_eq!(secs(b.fail()), 160);
    }

    #[test]
    fn backoff_is_capped_at_the_maximum() {
        let mut b = Backoff::new(Duration::from_secs(10));
        // Drive many failures; the delay must never exceed the cap.
        let mut last = 0;
        for _ in 0..20 {
            last = secs(b.fail());
            assert!(last <= MAX_BACKOFF_SECS, "delay {last} exceeded cap");
        }
        assert_eq!(
            last, MAX_BACKOFF_SECS,
            "settles at the cap under a long outage"
        );
    }

    #[test]
    fn backoff_resets_to_base_on_success() {
        let mut b = Backoff::new(Duration::from_secs(10));
        b.fail();
        b.fail();
        assert_eq!(secs(b.reset()), 10, "a success drops straight back to base");
        // …and growth starts over from 2·base.
        assert_eq!(secs(b.fail()), 20);
    }

    #[test]
    fn backoff_cap_is_never_below_the_base_interval() {
        // An operator may configure a base slower than MAX_BACKOFF_SECS (up to the
        // Wayfinder ceiling of 3600 s). Backoff must never shorten the poll below it.
        let mut b = Backoff::new(Duration::from_secs(3600));
        for _ in 0..10 {
            assert!(
                secs(b.fail()) >= 3600,
                "backoff must not poll faster than base"
            );
        }
    }

    #[test]
    fn backoff_floors_a_zero_base_at_one_second() {
        // A 0 base (should not happen — config rejects it) must not divide-by-zero
        // or hot-spin; it is floored to 1 s.
        let mut b = Backoff::new(Duration::from_secs(0));
        assert_eq!(secs(b.reset()), 1);
        assert_eq!(secs(b.fail()), 2);
    }

    #[test]
    fn rate_limited_error_is_flagged() {
        assert!(PollError::RateLimited.is_rate_limited());
        assert_eq!(
            PollError::RateLimited.to_string(),
            "OpenSky rate limit (HTTP 429)"
        );
    }
}
