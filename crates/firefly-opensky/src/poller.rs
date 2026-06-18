//! The [`OpenSkyPoller`]: polls the OpenSky Network REST API on a fixed
//! interval and converts each response into a batch of
//! [`Plot`](firefly_core::Plot)s for the tracker.

use std::time::Duration;

use firefly_core::Plot;
use tracing::{debug, warn};

use crate::{
    api::{parse_state, StatesResponse},
    config::OpenSkyConfig,
};

/// Base URL of the OpenSky REST API.
const API_BASE: &str = "https://opensky-network.org/api/states/all";

/// Error returned by a single poll attempt.
#[derive(Debug)]
pub enum PollError {
    /// HTTP-layer failure (network unreachable, timeout, non-2xx response, …).
    Http(reqwest::Error),
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::Http(e) => write!(f, "HTTP error: {e}"),
        }
    }
}

impl From<reqwest::Error> for PollError {
    fn from(e: reqwest::Error) -> Self {
        PollError::Http(e)
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
}

impl OpenSkyPoller {
    /// Create a new poller.  The `reqwest` HTTP client is built once and reused
    /// across all polls (connection pooling).
    pub fn new(config: OpenSkyConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("TLS backend should initialise successfully");
        Self { config, client }
    }

    /// Perform a single poll: fetch the state vectors for the configured
    /// bounding box and return them as a [`Vec<Plot>`].
    ///
    /// Aircraft that are on the ground, or whose position is unknown, are
    /// silently dropped.
    pub async fn poll(&self) -> Result<Vec<Plot>, PollError> {
        let mut req = self.client.get(API_BASE).query(&[
            ("lamin", self.config.lat_min.to_string()),
            ("lomin", self.config.lon_min.to_string()),
            ("lamax", self.config.lat_max.to_string()),
            ("lomax", self.config.lon_max.to_string()),
        ]);

        if let (Some(user), Some(pass)) = (
            self.config.username.as_deref(),
            self.config.password.as_deref(),
        ) {
            req = req.basic_auth(user, Some(pass));
        }

        let resp: StatesResponse = req.send().await?.error_for_status()?.json().await?;
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
