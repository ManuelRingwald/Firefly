//! 12-factor server configuration, read from the environment.
//!
//! Everything the server needs to start comes from environment variables, so
//! the binary runs with no flags and no config file (ADR 0003, 12-factor;
//! NFR-OPS-001 one-command demo). Parsing is split from the environment lookup
//! so it can be tested without touching the real process environment.

/// Startup configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerConfig {
    /// TCP port to listen on. `FIREFLY_PORT`, default `8080`.
    pub port: u16,
    /// Playback speed in **data-seconds per wall-second**. `FIREFLY_SPEED`,
    /// default `1.0`. `2.0` plays twice as fast. Clamped to be finite and
    /// strictly positive so the pacing maths can never divide by zero.
    pub speed: f64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            speed: 1.0,
        }
    }
}

impl ServerConfig {
    /// Read configuration from the process environment.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Read configuration from an arbitrary key→value lookup.
    ///
    /// Unset or unparseable values fall back to the defaults — a misconfigured
    /// variable should not stop a demo from starting.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let default = Self::default();
        let port = get("FIREFLY_PORT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.port);
        let speed = get("FIREFLY_SPEED")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(default.speed);
        Self { port, speed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With nothing set, the defaults apply.
    #[test]
    fn empty_environment_yields_defaults() {
        let config = ServerConfig::from_lookup(|_| None);
        assert_eq!(config, ServerConfig::default());
    }

    /// Valid values are parsed.
    #[test]
    fn valid_values_are_parsed() {
        let config = ServerConfig::from_lookup(|key| match key {
            "FIREFLY_PORT" => Some("9000".to_string()),
            "FIREFLY_SPEED" => Some("4".to_string()),
            _ => None,
        });
        assert_eq!(config.port, 9000);
        assert!((config.speed - 4.0).abs() < 1e-12);
    }

    /// Garbage and non-positive speeds fall back to the defaults rather than
    /// crashing the server.
    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let config = ServerConfig::from_lookup(|key| match key {
            "FIREFLY_PORT" => Some("not-a-port".to_string()),
            "FIREFLY_SPEED" => Some("0".to_string()),
            _ => None,
        });
        assert_eq!(config.port, ServerConfig::default().port);
        assert_eq!(config.speed, ServerConfig::default().speed);

        let nan =
            ServerConfig::from_lookup(|key| (key == "FIREFLY_SPEED").then(|| "-2.5".to_string()));
        assert_eq!(nan.speed, ServerConfig::default().speed);
    }
}
