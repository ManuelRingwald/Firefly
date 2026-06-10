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
//! ## Scope of this first piece (Häppchen 3.X.1)
//!
//! The framing ([`cat062`]) and the FSPEC machinery ([`fspec`]) are in place,
//! together with the three straightforward fixed-point items that need no
//! geometry: `I062/010` (data source), `I062/070` (time of track) and `I062/040`
//! (track number). Position, velocity and status items follow in 3.X.2 / 3.X.3.
//!
//! REQ: FR-IO-003

mod cat062;
mod fspec;

pub use cat062::{Cat062Encoder, DataSourceId};
