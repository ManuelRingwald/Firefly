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

mod association;
mod gating;
mod imm;
mod kalman;
mod measurement;
mod metrics;
mod motion;
mod track;
mod tracker;

pub use association::{associate, Association};
pub use gating::Gate;
pub use imm::Imm;
pub use kalman::{LinearKalman, ProcessNoise};
pub use measurement::{convert_plot, CartesianMeasurement, SensorErrorModel};
pub use metrics::{Rmse, TrackContinuity};
pub use motion::MotionModel;
pub use track::{Track, TrackStatus};
pub use tracker::{SensorModel, Tracker, TrackerConfig};
