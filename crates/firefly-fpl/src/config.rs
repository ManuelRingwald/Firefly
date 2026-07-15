//! Environment-driven flight-plan input (FPL.1) — the `firefly-meteo`
//! honesty pattern: a configured-but-broken source fails the start, it
//! never degrades silently; unset means "no flight plans" (allowed, INFO).
//!
//! REQ: FR-TRK-047

use crate::plan::FlightPlan;

/// Name of the environment variable carrying the JSON flight-plan list.
pub const FLIGHT_PLANS_ENV: &str = "FIREFLY_FLIGHT_PLANS";

/// The parsed and validated flight-plan configuration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FplConfig {
    pub plans: Vec<FlightPlan>,
}

impl FplConfig {
    /// Read and validate `FIREFLY_FLIGHT_PLANS`. Unset or empty ⇒ empty
    /// configuration (INFO); malformed JSON, an empty callsign, a squawk
    /// that is not an octal code as written (see [`FlightPlan::squawk`]),
    /// a non-finite time or a **duplicate callsign** (two plans could never
    /// be told apart by the primary key) ⇒ `Err` — the caller treats this
    /// as fatal.
    pub fn from_env() -> Result<Self, String> {
        match std::env::var(FLIGHT_PLANS_ENV) {
            Ok(raw) if !raw.trim().is_empty() => Self::parse(&raw),
            _ => {
                tracing::info!("no flight plans configured ({FLIGHT_PLANS_ENV} unset)");
                Ok(Self::default())
            }
        }
    }

    /// Parse and validate a JSON flight-plan list (separated from the env
    /// read so it is directly testable).
    pub fn parse(raw: &str) -> Result<Self, String> {
        let plans: Vec<FlightPlan> = serde_json::from_str(raw)
            .map_err(|e| format!("{FLIGHT_PLANS_ENV}: malformed JSON: {e}"))?;
        let mut seen = std::collections::BTreeSet::new();
        for plan in &plans {
            let key = plan.callsign_key();
            if key.is_empty() {
                return Err(format!("{FLIGHT_PLANS_ENV}: a plan has an empty callsign"));
            }
            if !seen.insert(key.clone()) {
                return Err(format!(
                    "{FLIGHT_PLANS_ENV}: duplicate callsign {key:?} — two plans \
                     could never be told apart by the primary correlation key"
                ));
            }
            if let Some(t) = plan.expected_time {
                if !t.is_finite() {
                    return Err(format!(
                        "{FLIGHT_PLANS_ENV}: non-finite expected_time for {key}"
                    ));
                }
            }
        }
        Ok(Self { plans })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid list parses; unset-equivalent (empty) is an empty config.
    /// REQ: FR-TRK-047
    #[test]
    fn valid_json_parses_and_empty_is_empty() {
        let cfg = FplConfig::parse(
            r#"[{"callsign":"DLH123","squawk":1234,"departure":"EDDF",
                 "destination":"EDDM","expected_time":1000.0},
                {"callsign":"BAW22","squawk":"7500"}]"#,
        )
        .expect("valid");
        assert_eq!(cfg.plans.len(), 2);
        assert_eq!(cfg.plans[0].callsign_key(), "DLH123");
        assert_eq!(
            cfg.plans[0].squawk,
            Some(0o1234),
            "squawk digits are read as written, i.e. octal"
        );
        assert_eq!(cfg.plans[1].squawk, Some(0o7500), "string form works too");
        assert!(FplConfig::parse("[]").expect("empty list").plans.is_empty());
    }

    /// Malformed or implausible values are startup errors — a configured
    /// flight-plan source never degrades silently. REQ: FR-TRK-047
    #[test]
    fn malformed_or_implausible_values_are_errors() {
        assert!(FplConfig::parse("{nope").is_err(), "malformed JSON");
        assert!(
            FplConfig::parse(r#"[{"callsign":"  "}]"#).is_err(),
            "empty callsign"
        );
        assert!(
            FplConfig::parse(r#"[{"callsign":"A","squawk":668}]"#).is_err(),
            "a non-octal digit (decimal thinking) is a loud error"
        );
        assert!(
            FplConfig::parse(r#"[{"callsign":"A","squawk":12345}]"#).is_err(),
            "more than four digits is a loud error"
        );
        assert!(
            FplConfig::parse(r#"[{"callsign":"A","expected_time":null}]"#).is_ok(),
            "explicit null time is absence, not an error"
        );
        assert!(
            FplConfig::parse(r#"[{"callsign":"dlh123"},{"callsign":"DLH123 "}]"#).is_err(),
            "duplicate callsign after normalisation"
        );
    }
}
