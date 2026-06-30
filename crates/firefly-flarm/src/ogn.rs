//! Parser for OGN-flavoured APRS position reports (ADR 0026 §3).
//!
//! The Open Glider Network re-broadcasts FLARM/ADS-B/OGN-tracker beacons as
//! APRS position packets over APRS-IS. A typical aircraft line looks like:
//!
//! ```text
//! FLRDDE626>APRS,qAS,EGHL:/074548h5111.32N/00102.04W'086/007/A=000607 id0ADDE626 -019fpm +0.0rot 5.5dB
//! ```
//!
//! - `FLRDDE626` — source call: a 3-letter prefix (`FLR`/`ICA`/`OGN`) + 6-hex device address.
//! - `/074548h` — APRS data-type id (`/` = position **with** timestamp) + `HHMMSS` + `h` (HMS zulu).
//! - `5111.32N` — latitude `DDMM.mm` + hemisphere.
//! - `/` — symbol table id.
//! - `00102.04W` — longitude `DDDMM.mm` + hemisphere.
//! - `'` — symbol code.
//! - `086/007` — course/speed (degrees / knots), ignored here.
//! - `/A=000607` — altitude in feet.
//! - `id0ADDE626` — OGN id: byte `STttttaa` (stealth, no-track, aircraft type, address type) + address.
//! - optional `!W65!` — DAO extension adding a third decimal of minutes to lat/lon.
//!
//! # Robustness (security, charter §7 equivalent / ADR 0004)
//!
//! This is an **untrusted network input path**. Every step is bounds-checked and
//! returns [`None`] on anything malformed — the parser must never panic on input.
//! Lines that are not aircraft position beacons (server comments, receiver
//! beacons without an identifiable aircraft address) yield [`None`] and are
//! skipped by the caller.

/// How an aircraft's address was assigned — the low two bits of the OGN id byte.
///
/// Only [`AddressType::Icao`] is a real ICAO 24-bit address; FLARM and OGN-tracker
/// ids live in private ranges and are **not** ICAO addresses (ADR 0026 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType {
    /// Unknown / randomly assigned (id byte `aa = 0`).
    Unknown,
    /// Official ICAO 24-bit aircraft address (`aa = 1`).
    Icao,
    /// FLARM hardware id (`aa = 2`).
    Flarm,
    /// OGN tracker id (`aa = 3`).
    OgnTracker,
}

impl AddressType {
    /// Map the low two bits of the OGN id byte to an address type.
    fn from_bits(aa: u8) -> Self {
        match aa & 0b11 {
            1 => AddressType::Icao,
            2 => AddressType::Flarm,
            3 => AddressType::OgnTracker,
            _ => AddressType::Unknown,
        }
    }

    /// Derive the address type from the 3-letter source-call prefix, used when a
    /// line carries no explicit `id` field.
    fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "ICA" => Some(AddressType::Icao),
            "FLR" => Some(AddressType::Flarm),
            "OGN" => Some(AddressType::OgnTracker),
            _ => None,
        }
    }
}

/// A decoded OGN position report — the parser's output, before mapping to a
/// [`Plot`](firefly_core::Plot).
#[derive(Debug, Clone, PartialEq)]
pub struct OgnPosition {
    /// Full source call, e.g. `FLRDDE626`.
    pub source_call: String,
    /// Time of day in seconds since UTC midnight (from the `HHMMSS` timestamp),
    /// if the packet carried an HMS timestamp.
    pub time_of_day_s: Option<f64>,
    /// WGS84 latitude, degrees (DAO-refined when present).
    pub latitude_deg: f64,
    /// WGS84 longitude, degrees (DAO-refined when present).
    pub longitude_deg: f64,
    /// Altitude in feet (`/A=`), if present.
    pub altitude_ft: Option<f64>,
    /// 24-bit device address, if identifiable.
    pub address: Option<u32>,
    /// How that address was assigned (decides ICAO use downstream).
    pub address_type: AddressType,
    /// OGN aircraft-type nibble (`tttt`), if an `id` field was present.
    pub aircraft_type: Option<u8>,
}

/// Parse one OGN-flavoured APRS line into an [`OgnPosition`].
///
/// Returns [`None`] for server comments, non-position packets, malformed lines,
/// or position beacons that carry **no identifiable aircraft address** (e.g.
/// ground receiver beacons) — we only track aircraft.
pub fn parse_position(line: &str) -> Option<OgnPosition> {
    // Header `SRC>DEST,PATH` : body.
    let (header, body) = line.split_once(':')?;
    let source_call = header.split('>').next()?;
    if source_call.is_empty() {
        return None;
    }

    // Body must start with a position data-type id.
    let first = *body.as_bytes().first()?;
    let (has_time, mut idx) = match first {
        b'/' | b'@' => (true, 1),  // position with timestamp
        b'!' | b'=' => (false, 1), // position without timestamp
        _ => return None,
    };

    // Optional `HHMMSSh` timestamp (7 chars). Only the HMS ('h') form is mapped;
    // a day/hour/minute form ('z' / local) leaves the time unknown.
    let mut time_of_day_s = None;
    if has_time {
        let ts = body.get(idx..idx + 7)?;
        idx += 7;
        if let Some(hms) = ts.strip_suffix('h') {
            time_of_day_s = parse_hms(hms);
        }
    }

    // Fixed-width position: lat `DDMM.mmH` (8), symbol table (1), lon `DDDMM.mmH` (9), symbol code (1).
    let lat_field = body.get(idx..idx + 8)?;
    idx += 8;
    let _sym_table = body.get(idx..idx + 1)?;
    idx += 1;
    let lon_field = body.get(idx..idx + 9)?;
    idx += 9;
    let _sym_code = body.get(idx..idx + 1)?;
    idx += 1;

    let (lat_deg, lat_min, lat_sign) = parse_lat(lat_field)?;
    let (lon_deg, lon_min, lon_sign) = parse_lon(lon_field)?;

    // Remainder carries course/speed, `/A=` altitude, the OGN comment, and DAO.
    let rest = body.get(idx..).unwrap_or("");

    // DAO `!Dxy!` (datum letter + two extra-precision chars). Apply only the
    // human-readable digit form; ignore base-91 / unknown forms (still produce a
    // position, just without the sub-minute refinement).
    let (dao_lat, dao_lon) = find_dao(rest);
    let latitude_deg = lat_sign * (lat_deg + (lat_min + dao_lat) / 60.0);
    let longitude_deg = lon_sign * (lon_deg + (lon_min + dao_lon) / 60.0);
    if !(-90.0..=90.0).contains(&latitude_deg) || !(-180.0..=180.0).contains(&longitude_deg) {
        return None;
    }

    let altitude_ft = find_altitude_ft(rest);

    // Identity: prefer the explicit `id` field; otherwise fall back to the
    // source-call prefix. No identifiable aircraft address → not an aircraft.
    let (address, address_type, aircraft_type) = match find_id(rest) {
        Some((addr, atype, actype)) => (Some(addr), atype, Some(actype)),
        None => {
            let prefix = source_call.get(0..3)?;
            let atype = AddressType::from_prefix(prefix)?;
            let addr = source_call
                .get(3..9)
                .and_then(|h| u32::from_str_radix(h, 16).ok());
            (addr, atype, None)
        }
    };

    Some(OgnPosition {
        source_call: source_call.to_string(),
        time_of_day_s,
        latitude_deg,
        longitude_deg,
        altitude_ft,
        address,
        address_type,
        aircraft_type,
    })
}

/// Parse `HHMMSS` into seconds since midnight, validating the field ranges.
fn parse_hms(hms: &str) -> Option<f64> {
    if hms.len() != 6 || !hms.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let h: u32 = hms.get(0..2)?.parse().ok()?;
    let m: u32 = hms.get(2..4)?.parse().ok()?;
    let s: u32 = hms.get(4..6)?.parse().ok()?;
    if h > 23 || m > 59 || s > 59 {
        return None;
    }
    Some((h * 3600 + m * 60 + s) as f64)
}

/// Parse a `DDMM.mmH` latitude field into (degrees, minutes, sign).
fn parse_lat(field: &str) -> Option<(f64, f64, f64)> {
    let deg: f64 = field.get(0..2)?.parse().ok()?;
    let min: f64 = field.get(2..7)?.parse().ok()?;
    let sign = match field.as_bytes().get(7)? {
        b'N' => 1.0,
        b'S' => -1.0,
        _ => return None,
    };
    if !(0.0..60.0).contains(&min) || deg > 90.0 {
        return None;
    }
    Some((deg, min, sign))
}

/// Parse a `DDDMM.mmH` longitude field into (degrees, minutes, sign).
fn parse_lon(field: &str) -> Option<(f64, f64, f64)> {
    let deg: f64 = field.get(0..3)?.parse().ok()?;
    let min: f64 = field.get(3..8)?.parse().ok()?;
    let sign = match field.as_bytes().get(8)? {
        b'E' => 1.0,
        b'W' => -1.0,
        _ => return None,
    };
    if !(0.0..60.0).contains(&min) || deg > 180.0 {
        return None;
    }
    Some((deg, min, sign))
}

/// Find the `/A=NNNNNN` altitude (feet) in the remainder.
fn find_altitude_ft(rest: &str) -> Option<f64> {
    let pos = rest.find("/A=")?;
    let digits: String = rest[pos + 3..]
        .chars()
        .take(6)
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.len() == 6 {
        digits.parse().ok()
    } else {
        None
    }
}

/// Find and decode the OGN `id` token (`id` + 2 hex flag byte + 6 hex address).
/// Returns (address, address_type, aircraft_type).
fn find_id(rest: &str) -> Option<(u32, AddressType, u8)> {
    let token = rest
        .split_whitespace()
        .find(|t| t.len() == 10 && t.starts_with("id"))?;
    let hex = token.get(2..10)?;
    let flag = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
    let address = u32::from_str_radix(hex.get(2..8)?, 16).ok()?;
    let address_type = AddressType::from_bits(flag);
    let aircraft_type = (flag >> 2) & 0x0F;
    Some((address, address_type, aircraft_type))
}

/// Find a human-readable DAO token `!Dxy!` and return the extra latitude/longitude
/// minute refinement (thousandths of a minute). Non-digit (base-91 / unknown)
/// forms yield `(0, 0)` — the position stays at base precision.
fn find_dao(rest: &str) -> (f64, f64) {
    for token in rest.split_whitespace() {
        let bytes = token.as_bytes();
        if bytes.len() == 5
            && bytes[0] == b'!'
            && bytes[4] == b'!'
            && bytes[1].is_ascii_alphabetic()
        {
            if let (Some(x), Some(y)) = (
                (bytes[2] as char).to_digit(10),
                (bytes[3] as char).to_digit(10),
            ) {
                return (x as f64 * 0.001, y as f64 * 0.001);
            }
        }
    }
    (0.0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLR_LINE: &str =
        "FLRDDE626>APRS,qAS,EGHL:/074548h5111.32N/00102.04W'086/007/A=000607 id0ADDE626 -019fpm +0.0rot 5.5dB 3e -4.3kHz";
    const ICA_LINE: &str =
        "ICA4B4E68>APRS,qAS,Letzi:/152339h4726.50N/00814.20E'260/059/A=002253 !W65!";

    #[test]
    fn parses_a_flarm_beacon() {
        let p = parse_position(FLR_LINE).expect("valid FLARM line");
        assert_eq!(p.source_call, "FLRDDE626");
        // 51°11.32' N = 51.18867 ; 1°02.04' W = -1.034
        assert!((p.latitude_deg - (51.0 + 11.32 / 60.0)).abs() < 1e-9);
        assert!((p.longitude_deg - -(1.0 + 2.04 / 60.0)).abs() < 1e-9);
        assert_eq!(p.altitude_ft, Some(607.0));
        // id0ADDE626: 0x0A = 0b00001010 → aa=2 (FLARM), tttt=2
        assert_eq!(p.address, Some(0xDDE626));
        assert_eq!(p.address_type, AddressType::Flarm);
        assert_eq!(p.aircraft_type, Some(2));
        // 07:45:48 → 27948 s
        assert_eq!(p.time_of_day_s, Some(27948.0));
    }

    #[test]
    fn parses_an_icao_beacon_without_id_field_via_prefix() {
        let p = parse_position(ICA_LINE).expect("valid ICAO line");
        assert_eq!(p.address, Some(0x4B4E68));
        assert_eq!(p.address_type, AddressType::Icao);
        assert!(p.aircraft_type.is_none(), "no id field → no aircraft type");
        assert_eq!(p.altitude_ft, Some(2253.0));
    }

    #[test]
    fn dao_refines_the_coordinates() {
        // ICA_LINE carries !W65! → lat +6/1000', lon +5/1000'.
        let p = parse_position(ICA_LINE).expect("valid");
        let expect_lat = 47.0 + (26.50 + 0.006) / 60.0;
        let expect_lon = 8.0 + (14.20 + 0.005) / 60.0;
        assert!((p.latitude_deg - expect_lat).abs() < 1e-9, "DAO lat refine");
        assert!(
            (p.longitude_deg - expect_lon).abs() < 1e-9,
            "DAO lon refine"
        );
    }

    #[test]
    fn southern_and_western_hemispheres_negate() {
        let line = "FLRAABBCC>APRS,qAS,X:/000000h3344.00S/07012.00W^000/000/A=000100 id06AABBCC";
        let p = parse_position(line).expect("valid");
        assert!(p.latitude_deg < 0.0, "S → negative latitude");
        assert!(p.longitude_deg < 0.0, "W → negative longitude");
        assert!((p.latitude_deg - -(33.0 + 44.0 / 60.0)).abs() < 1e-9);
    }

    #[test]
    fn id_byte_bits_decode_address_and_aircraft_type() {
        // 0x4D = 0b0100_1101 → aa=01 (ICAO), tttt=0011 (3)
        let line = "OGN112233>APRS,qAS,X:/000000h0000.00N/00000.00E'000/000/A=000000 id4D112233";
        let p = parse_position(line).expect("valid");
        assert_eq!(p.address_type, AddressType::Icao);
        assert_eq!(p.aircraft_type, Some(3));
        assert_eq!(p.address, Some(0x112233));
    }

    #[test]
    fn receiver_beacon_without_aircraft_address_is_skipped() {
        // A ground receiver beacon: prefix is not FLR/ICA/OGN and there is no id field.
        let line = "EGHL>APRS,TCPIP*,qAC,GLIDERN1:/074555h5126.00NI00113.00W&/A=000077 v0.2.7";
        assert!(
            parse_position(line).is_none(),
            "receiver beacon must not become an aircraft plot"
        );
    }

    #[test]
    fn server_comment_and_garbage_yield_none() {
        assert!(parse_position("# aprsc 2.1.10 ...").is_none());
        assert!(parse_position("not a packet").is_none());
        assert!(parse_position("").is_none());
        assert!(parse_position("SRC>APRS:>status text, not a position").is_none());
    }

    #[test]
    fn truncations_never_panic() {
        // Robustness: every byte-prefix of a real line must parse without panicking.
        for line in [FLR_LINE, ICA_LINE] {
            for end in 0..line.len() {
                if line.is_char_boundary(end) {
                    let _ = parse_position(&line[..end]);
                }
            }
        }
    }

    #[test]
    fn mutated_bytes_never_panic() {
        // Robustness: flipping each byte to a few values must not panic.
        let base = FLR_LINE.as_bytes();
        for i in 0..base.len() {
            for repl in [b'0', b'/', b'!', b' ', 0xFF] {
                let mut bytes = base.to_vec();
                bytes[i] = repl;
                if let Ok(s) = std::str::from_utf8(&bytes) {
                    let _ = parse_position(s);
                }
            }
        }
    }
}
