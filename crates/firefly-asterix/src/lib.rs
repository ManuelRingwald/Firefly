//! ASTERIX CAT062 output adapter — the tracker's *operational* wire format.
//!
//! Where [`firefly-io`](../firefly_io/index.html) serialises the neutral
//! [`SystemTrack`](firefly_core::SystemTrack) to JSON for the M3 web map (easy to
//! read, easy to debug), this crate encodes the **same** track into **ASTERIX
//! CAT062**, the binary System-Track format the ASD actually expects (ADR 0006).
//! It is a second adapter sitting beside JSON on the same neutral output; the
//! tracker core never learns a wire format (Ports & Adapters, NFR-INT-001).
//!
//! ## What ASTERIX looks like
//!
//! ASTERIX is a **bit-exact binary** format. One CAT062 *data block* is:
//!
//! ```text
//! [CAT = 62] [LEN : 2 bytes] [record] [record] …
//! ```
//!
//! `LEN` is the total block length (including the three header bytes), big-endian.
//! Each **record** begins with an **FSPEC** — a bitmask saying *which* data items
//! follow — after which the present items appear in a fixed order, the **UAP**
//! (User Application Profile). Each data item (e.g. `I062/070`, time of track) has
//! a defined byte width and, for numbers, a fixed **LSB** (the value of its least
//! significant bit), so a float like seconds becomes an integer count of LSBs.
//!
//! ## Scope so far (Häppchen 3.X.1–3.X.3)
//!
//! The framing ([`cat062`]) and the FSPEC machinery ([`fspec`]) are in place.
//! A record carries the data source (`I062/010`), time of track (`I062/070`),
//! WGS-84 position (`I062/105`), Cartesian velocity (`I062/185`), track number
//! (`I062/040`) and the safety-relevant status — confirmation/coasting
//! (`I062/080`), update age (`I062/290`) and position accuracy (`I062/500`).
//!
//! REQ: FR-IO-003, FR-TRK-008

mod cat034;
mod cat048;
mod cat062;
mod cat063;
mod cat065;
mod fspec;

pub use cat034::{
    decode_service_messages, Cat034DecodeError, DecodedServiceMessage, ServiceMessageType,
};
pub use cat048::{decode_target_reports, Cat048DecodeError, DecodedTargetReport, Detection};
pub use cat062::{
    decode_data_block, unproject_cartesian_position, Cat062Encoder, DataSourceId, DecodeError,
    DecodedRecord,
};
pub use cat063::{
    decode_sensor_block, Cat063DecodeError, Cat063Encoder, DecodedSensorStatus, SensorReason,
    SensorReport, SsrBias,
};
pub use cat065::{
    decode_status_block, Cat065DecodeError, Cat065Encoder, DecodedStatus, MESSAGE_TYPE_SDPS_STATUS,
};
