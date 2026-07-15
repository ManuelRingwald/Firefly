//! The evaluation CLI (HA.4): run the built-in benchmark scenarios through
//! the tracker and print the quality report — human-readable by default,
//! `--json` for CI trend lines.
//!
//! Usage: `firefly-eval [--json] [scenario…]` (no names = all built-ins).
//!
//! REQ: FR-TRK-051

use firefly_eval::{evaluate, scenarios, EvalConfig};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let names: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(String::as_str)
        .collect();

    let builtin = scenarios::builtin();
    let selected: Vec<_> = if names.is_empty() {
        builtin
    } else {
        let unknown: Vec<&&str> = names
            .iter()
            .filter(|n| !builtin.iter().any(|(name, _)| name == *n))
            .collect();
        if !unknown.is_empty() {
            let known: Vec<&str> = builtin.iter().map(|(n, _)| *n).collect();
            eprintln!("unknown scenario(s) {unknown:?}; available: {known:?}");
            std::process::exit(2);
        }
        builtin
            .into_iter()
            .filter(|(name, _)| names.contains(name))
            .collect()
    };

    let mut reports = Vec::new();
    for (name, scenario) in selected {
        let cfg = EvalConfig {
            label: name.to_string(),
            ..EvalConfig::default()
        };
        let report = evaluate(&scenario, &cfg);
        if !json {
            println!("{report}");
        }
        reports.push(report);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&reports).expect("reports serialise")
        );
    }
}
