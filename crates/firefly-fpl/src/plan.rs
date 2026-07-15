//! The minimal flight-plan record (FPL.1).
//!
//! Deliberately small: the fields a correlation and a first label/strip
//! need. The set grows **additively** as the EFS requirements land
//! (Wayfinder #244) — deserialisation tolerates unknown fields, so a
//! richer orchestrator-side plan never breaks an older Firefly.
//!
//! REQ: FR-TRK-047

use serde::{Deserialize, Serialize};

/// One filed flight plan as handed to Firefly (see [`crate::FplConfig`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlan {
    /// The callsign (e.g. `DLH123`) — the primary correlation key
    /// (ADR 0038 rule 1). Compared case-insensitively and trimmed.
    pub callsign: String,
    /// The assigned Mode 3/A code (squawk), **octal as usually written** —
    /// `1234` and `"1234"` both mean octal 1234 (a digit 8 or 9 is a loud
    /// parse error, never a silent decimal reinterpretation). Stored as the
    /// binary code value. Fallback correlation key only (ADR 0038 rule 2).
    #[serde(default, with = "squawk_as_octal")]
    pub squawk: Option<u16>,
    /// Departure aerodrome (ICAO locator), for display/strips.
    #[serde(default)]
    pub departure: Option<String>,
    /// Destination aerodrome (ICAO locator), for display/strips.
    #[serde(default)]
    pub destination: Option<String>,
    /// Expected time in the area of interest (Unix epoch seconds): the
    /// centre of the plausibility window (ADR 0038 rule 3). Absent = no
    /// temporal claim, the plan is a candidate at any time.
    #[serde(default)]
    pub expected_time: Option<f64>,
}

impl FlightPlan {
    /// The normalised callsign key: trimmed, ASCII-uppercased.
    pub fn callsign_key(&self) -> String {
        self.callsign.trim().to_ascii_uppercase()
    }
}

/// Mode 3/A codes are written as up to four **octal** digits everywhere in
/// aviation ("squawk 7500"). The JSON field therefore reads the digits as
/// written — number `1234` and string `"1234"` both mean octal 1234
/// (decimal 668). A decimal digit that is not an octal digit (8/9) or a
/// fifth digit is a hard error: a mistyped or decimally-thought code must
/// fail the start, not correlate the wrong aircraft (ADR 0038).
mod squawk_as_octal {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u64),
        Text(String),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
        let Some(raw) = Option::<Raw>::deserialize(d)? else {
            return Ok(None);
        };
        let digits = match raw {
            Raw::Number(n) => n.to_string(),
            Raw::Text(s) => s.trim().to_string(),
        };
        if digits.is_empty() || digits.len() > 4 {
            return Err(D::Error::custom(format!(
                "squawk {digits:?}: expected 1-4 octal digits"
            )));
        }
        u16::from_str_radix(&digits, 8).map(Some).map_err(|_| {
            D::Error::custom(format!(
                "squawk {digits:?}: not an octal code (digits 0-7 only; \
                 codes are read as written, e.g. 1234 = octal 1234)"
            ))
        })
    }

    pub fn serialize<S: Serializer>(code: &Option<u16>, s: S) -> Result<S::Ok, S::Error> {
        match code {
            Some(c) => s.serialize_str(&format!("{c:04o}")),
            None => s.serialize_none(),
        }
    }
}
