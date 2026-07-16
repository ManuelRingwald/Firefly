//! Property tests (ASSUR.2) for the squawk octal-as-written parser — the
//! Weeze-lesson field (FPL.1): `1234` and `"1234"` both mean OCTAL 1234,
//! and a digit 8/9 is a loud error, never a silent decimal reinterpretation.
//!
//! REQ: NFR-ASSUR-001

use firefly_fpl::FlightPlan;
use proptest::prelude::*;

fn parse_plan(squawk_json: &str) -> Result<FlightPlan, serde_json::Error> {
    serde_json::from_str(&format!(r#"{{"callsign":"TEST1","squawk":{squawk_json}}}"#))
}

proptest! {
    /// Every representable code (octal 0000–7777) round-trips through BOTH
    /// accepted spellings — bare number and string — to the same binary
    /// value.
    #[test]
    fn every_squawk_round_trips_as_number_and_string(code in 0u16..=0o7777) {
        let written = format!("{code:o}");
        let from_number = parse_plan(&written).expect("octal-digit number parses");
        prop_assert_eq!(from_number.squawk, Some(code));
        let from_string = parse_plan(&format!("\"{written}\"")).expect("octal-digit string parses");
        prop_assert_eq!(from_string.squawk, Some(code));
    }

    /// Any spelling containing a digit 8 or 9 is rejected loudly — for
    /// every position and every surrounding of otherwise-valid digits.
    #[test]
    fn any_digit_eight_or_nine_is_rejected(
        prefix in "[0-7]{0,2}",
        bad in 8u8..=9,
        suffix in "[0-7]{0,2}",
    ) {
        let written = format!("{prefix}{bad}{suffix}");
        prop_assert!(parse_plan(&written).is_err(), "number {written} must be rejected");
        prop_assert!(
            parse_plan(&format!("\"{written}\"")).is_err(),
            "string \"{}\" must be rejected", written
        );
    }
}
