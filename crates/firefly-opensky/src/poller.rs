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

/// Error returned by a single poll attempt.
#[derive(Debug)]
pub enum PollError {
    /// HTTP-layer failure (network unreachable, timeout, non-2xx response, …).
    Http(reqwest::Error),
    /// Failure obtaining an OAuth2 access token (ADR 0024).
    Auth(AuthError),
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::Http(e) => write!(f, "HTTP error: {e}"),
            PollError::Auth(e) => write!(f, "{e}"),
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
    /// Transient HTTP errors (network hiccup, rate-limit 429, OpenSky downtime)
    /// are logged as warnings and skipped; the loop continues at the configured
    /// interval.  This method never returns.
    pub async fn run<F, E>(&self, mut on_plots: F, mut on_error: E)
    where
        F: FnMut(Vec<Plot>),
        E: FnMut(&PollError),
    {
        let interval = Duration::from_secs(self.config.poll_interval_secs);
        loop {
            match self.poll().await {
                Ok(plots) if !plots.is_empty() => on_plots(plots),
                Ok(_) => debug!("OpenSky poll returned no airborne aircraft"),
                Err(e) => {
                    warn!("OpenSky poll failed: {e}");
                    on_error(&e);
                }
            }
            tokio::time::sleep(interval).await;
        }
    }
}
