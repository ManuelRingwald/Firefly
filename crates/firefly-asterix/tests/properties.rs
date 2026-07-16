//! Property tests (ASSUR.2): the CAT062 wire contract holds for THOUSANDS
//! of randomly generated tracks — sharper than the crash-only fuzzing
//! (NFR-SAFE-002): here the decoded VALUES must come back LSB-exact, not
//! merely without a panic.
//!
//! Two invariant families:
//!
//! 1. **Round trip**: `decode(encode(track))` reproduces every encoded
//!    field within its documented LSB (ICD §4) — for arbitrary positions,
//!    velocities, identities and status flags.
//! 2. **Total decoders**: all three block decoders (CAT062/063/065) are
//!    total functions over arbitrary bytes — they answer `Ok`/`Err`,
//!    never panic and never hang (the robust-decoder rule, charter §7,
//!    proptest edition of the fuzz targets).
//!
//! REQ: NFR-ASSUR-001

use firefly_asterix::{
    decode_data_block, decode_sensor_block, decode_status_block, Cat062Encoder, DataSourceId,
};
use firefly_core::{Callsign, SourceAges, SystemTrack, Timestamp, TrackId};
use firefly_geo::Wgs84;
use proptest::prelude::*;

/// LSB of I062/105 (WGS84 lat/lon): 180/2²⁵ degrees.
const LSB_POSITION_DEG: f64 = 180.0 / (1u64 << 25) as f64;
/// LSB of I062/185 (velocity): 0.25 m/s.
const LSB_VELOCITY: f64 = 0.25;
/// LSB of I062/136 (flight level): 1/4 FL = 25 ft.
const LSB_FLIGHT_LEVEL_FT: f64 = 25.0;
/// LSB of I062/290 (ages): 0.25 s.
const LSB_AGE_S: f64 = 0.25;
/// LSB of I062/500 APC: 0.5 m.
const LSB_APC_M: f64 = 0.5;

/// A minimal, valid track with every optional item absent; the properties
/// then switch individual fields on.
fn base_track(track_number: u16) -> SystemTrack {
    SystemTrack {
        id: TrackId(track_number as u32),
        track_number,
        time: Timestamp(0.0),
        position: Wgs84::from_degrees(0.0, 0.0, 0.0),
        v_east: 0.0,
        v_north: 0.0,
        confirmed: true,
        coasting: false,
        monosensor: false,
        spi: false,
        daps: firefly_core::Daps::default(),
        ended: false,
        update_age: 0.0,
        position_uncertainty: 0.0,
        mode_3a: None,
        icao_address: None,
        flight_level_ft: None,
        callsign: None,
        contributing_sensors: Vec::new(),
        adsb_age_s: None,
        source_ages: SourceAges::default(),
        barometric_altitude_ft: None,
        barometric_qnh_corrected: false,
        geometric_altitude_ft: None,
        rocd_ft_min: None,
        acceleration_mps2: None,
        mode_of_movement: None,
        identity_conflict: false,
        flight_plan: None,
    }
}

fn encoder() -> Cat062Encoder {
    Cat062Encoder::new(
        DataSourceId::new(0x19, 0x01),
        Wgs84::from_degrees(45.0, 11.25, 0.0),
        0.0,
    )
}

proptest! {
    /// Kinematics + status round trip for arbitrary values in the wire
    /// domain: position (±85°), velocity (±2000 m/s), ages, uncertainty and
    /// all four status flags come back within their documented LSB.
    #[test]
    fn cat062_kinematics_round_trip_within_lsb(
        lat in -85.0f64..85.0,
        lon in -179.9f64..179.9,
        v_east in -2000.0f64..2000.0,
        v_north in -2000.0f64..2000.0,
        track_number in 0u16..=u16::MAX,
        update_age in 0.0f64..63.0,
        uncertainty in 0.0f64..500.0,
        time in 0.0f64..86_399.0,
        confirmed in any::<bool>(),
        coasting in any::<bool>(),
        ended in any::<bool>(),
        spi in any::<bool>(),
    ) {
        let mut t = base_track(track_number);
        t.position = Wgs84::from_degrees(lat, lon, 5_000.0);
        t.v_east = v_east;
        t.v_north = v_north;
        t.update_age = update_age;
        t.position_uncertainty = uncertainty;
        t.confirmed = confirmed;
        t.coasting = coasting;
        t.ended = ended;
        t.spi = spi;

        let block = encoder().encode(Timestamp(time), &[t]);
        let decoded = decode_data_block(&block).expect("own encoding decodes");
        prop_assert_eq!(decoded.len(), 1);
        let d = &decoded[0];

        prop_assert!((d.position.lat_deg() - lat).abs() <= LSB_POSITION_DEG);
        prop_assert!((d.position.lon_deg() - lon).abs() <= LSB_POSITION_DEG);
        prop_assert!((d.v_east - v_east).abs() <= LSB_VELOCITY);
        prop_assert!((d.v_north - v_north).abs() <= LSB_VELOCITY);
        prop_assert_eq!(d.track_number, track_number);
        prop_assert!((d.update_age - update_age).abs() <= LSB_AGE_S);
        prop_assert!((d.position_uncertainty - uncertainty).abs() <= LSB_APC_M);
        prop_assert!((d.time.0 - time).abs() <= 1.0 / 128.0);
        prop_assert_eq!(d.confirmed, confirmed);
        prop_assert_eq!(d.coasting, coasting);
        prop_assert_eq!(d.ended, ended);
        prop_assert_eq!(d.spi, spi);
    }

    /// Identity round trip: Mode 3/A (any octal code), ICAO 24-bit address,
    /// flight level and an arbitrary IA-5 callsign come back exactly (the
    /// callsign space-trimmed, the flight level within its 25-ft LSB).
    #[test]
    fn cat062_identity_round_trip(
        mode_3a in 0u16..=0o7777,
        icao in 0u32..0x0100_0000,
        fl_ft in -1_000.0f64..60_000.0,
        callsign in "[A-Z0-9]{1,7}",
        track_number in 0u16..=u16::MAX,
    ) {
        let mut t = base_track(track_number);
        t.mode_3a = Some(mode_3a);
        t.icao_address = Some(icao);
        t.flight_level_ft = Some(fl_ft);
        t.callsign = Some(Callsign::new(&callsign));

        let block = encoder().encode(Timestamp(1.0), &[t]);
        let decoded = decode_data_block(&block).expect("own encoding decodes");
        let d = &decoded[0];

        prop_assert_eq!(d.mode_3a, Some(mode_3a));
        prop_assert_eq!(d.icao_address, Some(icao));
        let got_fl = d.flight_level_ft.expect("flight level present");
        prop_assert!((got_fl - fl_ft).abs() <= LSB_FLIGHT_LEVEL_FT);
        let got_cs = d.callsign.expect("callsign present");
        prop_assert_eq!(got_cs.as_str(), callsign.as_str());
    }

    /// All three block decoders are total over arbitrary input: any byte
    /// soup — including inputs that start with the right category octet —
    /// yields `Ok` or `Err`, never a panic (proptest edition of the
    /// NFR-SAFE-002 fuzz targets, runs in every `cargo test`).
    #[test]
    fn decoders_are_total_over_arbitrary_bytes(
        mut bytes in proptest::collection::vec(any::<u8>(), 0..256),
        force_category in any::<bool>(),
        category in prop_oneof![Just(0x3Eu8), Just(0x3F), Just(0x41)],
    ) {
        if force_category && !bytes.is_empty() {
            bytes[0] = category;
        }
        let _ = decode_data_block(&bytes);
        let _ = decode_sensor_block(&bytes);
        let _ = decode_status_block(&bytes);
    }
}
