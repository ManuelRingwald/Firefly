# Firefly

A web-based **radar tracker** — the computational heart of an air-situation
picture (*Luftlagedarstellung*). Firefly turns the noisy, intermittent plot
streams of primary (PSR) and secondary (SSR) surveillance radars into clean,
continuous **tracks**: estimated position, velocity and identity for every
aircraft in coverage.

> **Status: early development.** This is an open, educational demonstrator of
> the real algorithms used in air surveillance — not a certified operational
> system.

## What it does (and will do)

The processing chain from detections to tracks:

1. **Sensor / measurement model** — PSR gives range + azimuth (no identity, no
   barometric height); SSR adds Mode 3/A (squawk), Mode C (flight level) and
   Mode S (ICAO address). Measurements are polar and sensor-referenced.
2. **Track initiation** — forming tentative tracks from unassociated plots.
3. **Prediction** — motion models: constant velocity / acceleration /
   coordinated turn, ultimately IMM for manoeuvring targets.
4. **Gating & data association** — assigning plots to tracks (NN → GNN →
   JPDA / MHT) via a validation gate.
5. **Filtering** — Kalman / Extended Kalman state estimation.
6. **Track maintenance** — confirmation, coasting on misses, deletion.
7. **Multi-radar fusion** — time/bias registration and combining PSR + SSR +
   ADS-B into one air picture.
8. **Web display** — live 2-D map of tracks over WebSocket.

## Roadmap

| Milestone | Scope | Status |
|-----------|-------|--------|
| **M1** | Scenario + radar-plot simulator (data source) | ✅ in progress |
| **M2** | Single-radar tracker: gating + GNN + Kalman, track lifecycle | ⏳ next |
| **M3** | Web frontend with live 2-D map over WebSocket | ⏳ |
| **M4** | SSR/ADS-B identity correlation + multi-radar fusion | ⏳ |
| **M5** | IMM / JPDA for manoeuvres and dense traffic | ⏳ |

## Architecture

A Rust workspace (engine) plus a JavaScript map frontend (added in M3).
Input/interchange targets the **ASTERIX** format used by real radar systems
(CAT048 monoradar target reports, CAT021 ADS-B, CAT062 system tracks).

```
firefly-geo      Geodesy: WGS84 ↔ ECEF ↔ local ENU ↔ polar
firefly-core     Shared domain types: plots, sensors, time, identities
firefly-sim      Scenario + radar-plot simulator (M1)
firefly-asterix  ASTERIX CAT048/021/062 encode/decode (M1.5)
firefly-track    Gating + association + Kalman filter + track lifecycle (M2)
firefly-server   axum WebSocket server (M3)
web/             MapLibre 2-D air-picture frontend (M3)
```

## Building

Requires a recent Rust toolchain.

```bash
cargo test --workspace          # run all tests
cargo run --example demo -p firefly-sim   # see the M1 simulator in action
```

The `demo` example builds a two-aircraft scenario around a single radar and
prints the resulting plot stream.

## Design notes

- **Reproducibility:** the simulator uses a seeded PCG32 PRNG, so a given seed
  yields exactly the same plot stream on any machine — essential for
  regression-testing tracker behaviour.
- **Polar noise:** measurement noise is applied in the radar's native polar
  frame (range vs. angle), where it physically lives, not in Cartesian x/y.
- **Frames:** target motion is scripted in one shared local ENU frame; each
  radar re-projects ground truth into its own polar frame, keeping per-sensor
  geometry geodetically correct.

## License

MIT
