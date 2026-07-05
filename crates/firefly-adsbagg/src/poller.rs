//! The [`AdsbAggPoller`]: polls an ADSBExchange-v2-compatible community
//! aggregator on a fixed interval and converts each response into a batch of
//! [`Plot`](firefly_core::Plot)s for the tracker (ADR 0031).
//!
//! The coverage bbox is queried as its circumscribed circle (the APIs are
//! point+radius) and the response trimmed back to the box — see
//! [`crate::geometry`]. No authentication: these are open community services;
//! politeness is enforced by the poll interval and the 429 backoff.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use firefly_core::Plot;
use tracing::{debug, warn};

use crate::{
    api::{parse_response, AircraftResponse},
    config::AdsbAggConfig,
    geometry::{circle_for_bbox, BBoxDeg, QueryCircle},
};

/// Upper cap (seconds) for the exponential poll backoff — same tuning as the
/// OpenSky poller (#49): a persistently rate-limited or down provider retries
/// at most every few minutes, one success drops straight back to base cadence.
const MAX_BACKOFF_SECS: u64 = 300;

/// Exponential backoff for the poll loop: a success runs at the base interval,
/// each consecutive failure doubles the sleep up to [`MAX_BACKOFF_SECS`].
///
/// Same small, pure state machine as the OpenSky poller's — deliberately
/// duplicated (≈30 lines) rather than shared: the adapter crates stay
/// dependency-free of each other (Ports & Adapters, ADR 0023).
#[derive(Debug)]
struct Backoff {
    base: Duration,
    cap: Duration,
    failures: u32,
}

impl Backoff {
    fn new(base: Duration) -> Self {
        let base = base.max(Duration::from_secs(1));
        // Never let backoff shorten a slower-than-cap configured base.
        let cap = Duration::from_secs(MAX_BACKOFF_SECS.max(base.as_secs()));
        Self {
            base,
            cap,
            failures: 0,
        }
    }

    fn reset(&mut self) -> Duration {
        self.failures = 0;
        self.base
    }

    fn fail(&mut self) -> Duration {
        self.failures = self.failures.saturating_add(1);
        let shift = self.failures.min(20);
        let secs = self.base.as_secs().saturating_mul(1u64 << shift);
        Duration::from_secs(secs).min(self.cap)
    }
}

/// Error returned by a single poll attempt.
#[derive(Debug)]
pub enum PollError {
    /// HTTP-layer failure (network unreachable, timeout, non-2xx other than 429).
    Http(reqwest::Error),
    /// The provider returned HTTP 429 — logged distinctly, counted separately
    /// and answered with backoff, exactly like the OpenSky poller (#49).
    RateLimited,
}

impl PollError {
    /// True when this poll failed because the provider rate-limited the request.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, PollError::RateLimited)
    }
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::Http(e) => write!(f, "HTTP error: {e}"),
            PollError::RateLimited => write!(f, "aggregator rate limit (HTTP 429)"),
        }
    }
}

impl From<reqwest::Error> for PollError {
    fn from(e: reqwest::Error) -> Self {
        PollError::Http(e)
    }
}

/// Polls a community aggregator's point-query endpoint periodically and
/// converts each batch of aircraft into [`Plot`]s.
pub struct AdsbAggPoller {
    config: AdsbAggConfig,
    client: reqwest::Client,
    bbox: BBoxDeg,
    circle: QueryCircle,
    url: String,
}

impl AdsbAggPoller {
    /// Create a new poller. The HTTP client is built once and reused across
    /// all polls (connection pooling); the query circle and URL are derived
    /// once from the configured bbox. A bbox too large for the provider's
    /// 250 NM cap is clamped **with a prominent warning** — silent partial
    /// coverage would be an invisible surveillance gap.
    pub fn new(config: AdsbAggConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("TLS backend should initialise successfully");
        let bbox = BBoxDeg {
            lat_min: config.lat_min,
            lat_max: config.lat_max,
            lon_min: config.lon_min,
            lon_max: config.lon_max,
        };
        let circle = circle_for_bbox(&bbox);
        if circle.clamped {
            warn!(
                radius_nm = circle.radius_nm,
                "configured bbox exceeds the provider's 250 NM query radius; \
                 coverage is CLAMPED to the circle around the bbox centre"
            );
        }
        let url = format!(
            "{}{}",
            config.effective_base_url(),
            config
                .provider
                .point_path(circle.lat, circle.lon, circle.radius_nm)
        );
        Self {
            config,
            client,
            bbox,
            circle,
            url,
        }
    }

    /// The derived point query (centre + radius NM) — exposed for logging.
    pub fn query_summary(&self) -> (f64, f64, f64) {
        (self.circle.lat, self.circle.lon, self.circle.radius_nm)
    }

    /// Perform a single poll: fetch the aircraft in the query circle, trim to
    /// the configured bbox, and return the usable ones as [`Plot`]s.
    pub async fn poll(&self) -> Result<Vec<Plot>, PollError> {
        let resp = self.client.get(&self.url).send().await?;
        // Detect a rate limit explicitly (before error_for_status collapses
        // every non-2xx into an opaque Http error), like the OpenSky poller.
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(PollError::RateLimited);
        }
        let body: AircraftResponse = resp.error_for_status()?.json().await?;

        // Fallback clock for a response without a usable `now`: the receive
        // wall clock, consistent with the FLARM adapter's epoch anchoring.
        let fallback_now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let plots = parse_response(&body, self.config.sensor_id, fallback_now, &self.bbox);
        debug!(
            provider = %self.config.provider,
            received = body.ac.len(),
            usable = plots.len(),
            "aggregator poll succeeded"
        );
        Ok(plots)
    }

    /// Run the polling loop indefinitely, calling `on_plots` after each
    /// successful poll that returns at least one usable aircraft, and
    /// `on_error` after each failed poll. Consecutive failures back off
    /// exponentially (see [`Backoff`]); a 429 is logged distinctly. This
    /// method never returns.
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
                    debug!("aggregator poll returned no usable aircraft");
                    backoff.reset()
                }
                Err(e) => {
                    if e.is_rate_limited() {
                        warn!(provider = %self.config.provider, "aggregator rate-limited (HTTP 429) — backing off");
                    } else {
                        warn!(provider = %self.config.provider, %e, "aggregator poll failed");
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
    use crate::config::Provider;

    fn secs(d: Duration) -> u64 {
        d.as_secs()
    }

    #[test]
    fn backoff_doubles_and_caps_and_resets() {
        let mut b = Backoff::new(Duration::from_secs(10));
        assert_eq!(secs(b.fail()), 20, "first failure backs off at once");
        assert_eq!(secs(b.fail()), 40);
        for _ in 0..20 {
            assert!(secs(b.fail()) <= MAX_BACKOFF_SECS);
        }
        assert_eq!(secs(b.reset()), 10, "success drops back to base");
    }

    #[test]
    fn backoff_never_polls_faster_than_a_slow_base() {
        let mut b = Backoff::new(Duration::from_secs(3600));
        for _ in 0..5 {
            assert!(secs(b.fail()) >= 3600);
        }
    }

    /// The poller derives one URL from bbox + provider at construction: the
    /// circumscribed circle on the provider's documented path.
    #[test]
    fn poller_builds_the_point_query_url_from_the_bbox() {
        let config = AdsbAggConfig {
            enabled: true,
            provider: Provider::AdsbLol,
            lat_min: 49.5,
            lat_max: 50.5,
            lon_min: 7.8,
            lon_max: 9.3,
            ..AdsbAggConfig::default()
        };
        let poller = AdsbAggPoller::new(config);
        let (lat, lon, radius) = poller.query_summary();
        assert!((lat - 50.0).abs() < 1e-12);
        assert!((lon - 8.55).abs() < 1e-12);
        assert!(radius > 40.0 && radius < 80.0);
        assert!(poller
            .url
            .starts_with("https://api.adsb.lol/v2/lat/50/lon/8.55/dist/"));
    }

    /// A base-URL override redirects the whole query (self-hosted aggregator,
    /// tests) while keeping the provider's path shape.
    #[test]
    fn base_url_override_redirects_the_query() {
        let config = AdsbAggConfig {
            enabled: true,
            provider: Provider::AdsbFi,
            base_url: Some("http://localhost:8080/".into()),
            ..AdsbAggConfig::default()
        };
        let poller = AdsbAggPoller::new(config);
        assert!(poller.url.starts_with("http://localhost:8080/v3/lat/"));
    }

    #[test]
    fn rate_limited_error_is_flagged() {
        assert!(PollError::RateLimited.is_rate_limited());
        assert_eq!(
            PollError::RateLimited.to_string(),
            "aggregator rate limit (HTTP 429)"
        );
    }
}
