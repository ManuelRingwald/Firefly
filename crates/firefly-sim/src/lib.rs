//! Scenario and radar-plot simulator for the Firefly radar tracker.
//!
//! The simulator is the data source for milestone M1: it scripts ground-truth
//! targets, flies them through a shared local frame, and lets one or more
//! simulated radars observe them — producing the noisy, intermittent plot
//! stream that the tracker will later have to turn back into clean tracks.
//!
//! ```
//! use firefly_geo::Wgs84;
//! use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
//! use firefly_geo::Enu;
//! use firefly_core::{Sensor, SensorId, TargetId};
//!
//! let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
//! let radar = Radar::new(Sensor::new(SensorId(1), origin), RadarParams::default());
//! let target = Target {
//!     id: TargetId(1),
//!     initial: State { position: Enu::new(0.0, 0.0, 3000.0), speed: 200.0, heading: 0.0, climb_rate: 0.0 },
//!     legs: vec![Leg::cruise(60.0)],
//!     mode_3a: Some(0o7000),
//!     icao_address: None,
//!     callsign: None,
//! };
//! let scenario = Scenario::new(origin).add_radar(radar).add_target(target);
//! let plots = firefly_sim::run(&scenario);
//! assert!(!plots.is_empty());
//! ```

mod radar;
mod rng;
mod run;
mod scenario;
mod target;

pub use radar::{Radar, RadarParams};
pub use rng::Pcg32;
pub use run::run;
pub use scenario::Scenario;
pub use target::{Leg, State, Target};
