//! 12-factor server configuration, read from the environment.
//!
//! Everything the server needs to start comes from environment variables, so
//! the binary runs with no flags and no config file (ADR 0003, 12-factor;
//! NFR-OPS-001 one-command demo). Parsing is split from the environment lookup
//! so it can be tested without touching the real process environment.

/// Which built-in scene the server replays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scene {
    /// The small two-aircraft, one-radar demo (see [`crate::scene::demo_frames`]).
    #[default]
    Demo,
    /// The Frankfurt showcase: three radars, eight aircraft (see
    /// [`crate::scene::frankfurt_frames`]).
    Frankfurt,
}

/// Whether the server runs a pre-computed replay or a live ADS-B tracker.
///
/// `FIREFLY_MODE=live` switches to the real-time path: the OpenSky Network ADS-B
/// adapter feeds plots into a long-lived [`Tracker`](firefly_track::Tracker) and
/// the resulting snapshots are distributed to WebSocket clients and the CAT062
/// multicast feed (ADR 0020, AP9.4c-3). In `live` mode `FIREFLY_SCENE` and
/// `FIREFLY_SPEED` are ignored; OpenSky bbox/credentials are read from
/// `FIREFLY_OPENSKY_*` (see [`firefly_opensky::OpenSkyConfig`]).
///
/// [`Tracker`]: firefly_track::Tracker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServerMode {
    /// Deterministic replay of a built-in scene, paced by `FIREFLY_SPEED`.
    #[default]
    Replay,
    /// Real-time ADS-B tracking via the OpenSky Network adapter.
    Live,
}

/// Startup configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerConfig {
    /// TCP port to listen on. `FIREFLY_PORT`, default `8080`.
    pub port: u16,
    /// Playback speed in **data-seconds per wall-second**. `FIREFLY_SPEED`,
    /// default `1.0`. `2.0` plays twice as fast. Clamped to be finite and
    /// strictly positive so the pacing maths can never divide by zero.
    /// Ignored in Live mode.
    pub speed: f64,
    /// Which scene to replay. `FIREFLY_SCENE`, `"demo"` (default) or
    /// `"frankfurt"`. An unrecognised value falls back to the default rather
    /// than stopping the demo from starting. Ignored in Live mode.
    pub scene: Scene,
    /// Replay or live ADS-B tracker. `FIREFLY_MODE`, `"replay"` (default) or
    /// `"live"`. Unrecognised values fall back to `Replay`.
    pub mode: ServerMode,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            speed: 1.0,
            scene: Scene::default(),
            mode: ServerMode::default(),
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
        let scene = match get("FIREFLY_SCENE").as_deref() {
            Some("frankfurt") => Scene::Frankfurt,
            _ => default.scene,
        };
        let mode = match get("FIREFLY_MODE").as_deref() {
            Some("live") => ServerMode::Live,
            _ => default.mode,
        };
        Self {
            port,
            speed,
            scene,
            mode,
        }
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

    /// `FIREFLY_MODE=live` selects the live tracker; anything else falls back to
    /// replay.
    #[test]
    fn mode_selects_live_or_falls_back_to_replay() {
        let live = ServerConfig::from_lookup(|key| (key == "FIREFLY_MODE").then(|| "live".into()));
        assert_eq!(live.mode, ServerMode::Live);

        let unset = ServerConfig::from_lookup(|_| None);
        assert_eq!(unset.mode, ServerMode::Replay);

        let garbage =
            ServerConfig::from_lookup(|key| (key == "FIREFLY_MODE").then(|| "unknown".into()));
        assert_eq!(garbage.mode, ServerMode::Replay);
    }

    /// `FIREFLY_SCENE=frankfurt` selects the Frankfurt showcase; anything else
    /// (including an unset or unrecognised value) falls back to the demo.
    #[test]
    fn scene_selects_frankfurt_or_falls_back_to_demo() {
        let frankfurt =
            ServerConfig::from_lookup(|key| (key == "FIREFLY_SCENE").then(|| "frankfurt".into()));
        assert_eq!(frankfurt.scene, Scene::Frankfurt);

        let unset = ServerConfig::from_lookup(|_| None);
        assert_eq!(unset.scene, Scene::Demo);

        let garbage =
            ServerConfig::from_lookup(|key| (key == "FIREFLY_SCENE").then(|| "atlantis".into()));
        assert_eq!(garbage.scene, Scene::Demo);
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
