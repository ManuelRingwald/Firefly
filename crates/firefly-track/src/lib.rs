//! The Firefly tracker: turning radar plots into clean, continuous tracks.
//!
//! This crate is the heart of milestone M2. It grows step by step:
//!
//! - **2.1:** converting a polar plot into a Cartesian measurement with a
//!   proper covariance ([`measurement`]).
//! - **2.2:** a Kalman filter on a constant-velocity model ([`kalman`]).
//! - **2.3:** gating — a Mahalanobis validation region ([`gating`]).
//! - **2.4:** data association — global nearest neighbour ([`association`]).
//! - **2.5 (here):** the track lifecycle and the per-scan loop ([`track`],
//!   [`tracker`]).
//!
//! Design intent (see ADR 0003): the tracking step is kept a pure,
//! deterministic function of its inputs — no wall clock, no I/O — so it is
//! replayable, testable and cloud-recoverable.

mod acceleration;
mod association;
mod clutter_map;
mod gating;
mod imm;
mod jpda;
mod kalman;
mod kalman6;
mod measurement;
mod metrics;
mod motion;
mod pda;
mod registration;
mod track;
mod track_number;
mod tracker;
mod vertical;

pub use acceleration::AccelerationEstimator;
pub use association::{associate, Association};
pub use clutter_map::ClutterMap;
pub use gating::Gate;
pub use imm::{Imm, ImmConfig};
pub use jpda::{joint_association_probabilities, joint_association_probabilities_local};
pub use kalman::{LinearKalman, ProcessNoise};
pub use kalman6::{
    constant_acceleration_transition, constant_velocity_transition6, coordinated_turn_transition6,
    JerkNoise, LinearKalman6,
};
pub use measurement::{convert_plot, tracking_measurement, CartesianMeasurement, SensorErrorModel};
pub use metrics::{Rmse, TrackContinuity};
pub use motion::MotionModel;
pub use pda::{association_probabilities, ClutterModel};
pub use registration::{
    correspondences_by_identity, estimate_biases, ApplyPolicy, Correspondence, RegistrationApplier,
    RegistrationConfig, RegistrationMonitor, RegistrationSolution, SensorBias, Sighting,
};
pub use track::{Track, TrackStatus};
pub use tracker::{SensorModel, Tracker, TrackerConfig};
pub use vertical::VerticalFilter;
