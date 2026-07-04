//! 12-factor server configuration, read from the environment.
//!
//! Everything the server needs to start comes from environment variables, so
//! the binary runs with no flags and no config file (ADR 0003, 12-factor).
//! Parsing is split from the environment lookup so it can be tested without
//! touching the real process environment.
//!
//! The earlier `FIREFLY_MODE`/`FIREFLY_SCENE`/`FIREFLY_SPEED` replay knobs were
//! removed with the scene demo mode (ADR 0030): the server always runs the
//! sources-driven live tracker; its inputs come from `FIREFLY_SOURCES`
//! (ADR 0023) or the standalone per-adapter env config.

/// Startup configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerConfig {
    /// TCP port to listen on. `FIREFLY_PORT`, default `8080`.
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { port: 8080 }
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
    /// variable should not stop the server from starting.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let default = Self::default();
        let port = get("FIREFLY_PORT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.port);
        Self { port }
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

    /// A valid port is parsed.
    #[test]
    fn valid_port_is_parsed() {
        let config =
            ServerConfig::from_lookup(|key| (key == "FIREFLY_PORT").then(|| "9000".to_string()));
        assert_eq!(config.port, 9000);
    }

    /// Garbage falls back to the default rather than crashing the server.
    #[test]
    fn invalid_port_falls_back_to_default() {
        let config = ServerConfig::from_lookup(|key| {
            (key == "FIREFLY_PORT").then(|| "not-a-port".to_string())
        });
        assert_eq!(config.port, ServerConfig::default().port);
    }

    /// The removed replay knobs have no effect on parsing (ADR 0030): the
    /// config is port-only and simply ignores them.
    #[test]
    fn legacy_replay_knobs_are_ignored() {
        let config = ServerConfig::from_lookup(|key| match key {
            "FIREFLY_MODE" => Some("live".to_string()),
            "FIREFLY_SCENE" => Some("frankfurt".to_string()),
            "FIREFLY_SPEED" => Some("4".to_string()),
            _ => None,
        });
        assert_eq!(config, ServerConfig::default());
    }
}
