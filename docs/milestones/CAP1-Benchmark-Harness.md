# CAP.1 — Benchmark-Harness + synthetische Lastszenarien

> **Anforderung:** NFR-CAP-001 · **ADR:** — · **ICD:** unberührt ·
> **Einstufung:** S3 · umgesetzt auf Fable 5 (Roadmap-Empfehlung: Sonnet)

## Fachlich

Bevor wir Auslegungsgrenzen dokumentieren können („Firefly schafft X
Sensoren mit Y Zielen"), müssen wir den Durchsatz **messen** können —
reproduzierbar, auf dem echten Hot-Path, mit realistisch geformter Last.
CAP.1 liefert den Messstand dafür; die dokumentierten Grenzen und die
gezielte Optimierung (JPDA-Cluster-Grenzen) sind CAP.2.

## Technik

- **criterion-Benchmark** `firefly-eval/benches/load.rs`
  (`cargo bench -p firefly-eval`): misst `Tracker::process_plots` über
  den kompletten Plot-Strom eines Szenarios; der Tracker wird **wie in
  der Live-Verdrahtung** gebaut (`tracker_for` — jedes Radar mit eigenem
  Site-Frame und echtem Fehlermodell). Szenario-Erzeugung liegt außerhalb
  der Messung; criterion rechnet den Durchsatz in **Plots/s** um.
- **Lastszenarien** `scenarios::load_grid(N, M, dauer)`: M Ziele auf
  einem Raster mit 5-km-Separation (alternierende Kurse), N Radare
  30 km auseinander mit je eigenem Site-Frame — bewusst
  **separationstreu**: Die Stressgröße ist *Volumen*, nicht
  pathologische Überlappung (dichte Konflikt-Cluster sind der
  JPDA-Worst-Case und gehören zur CAP.2-Analyse).
- **Der Generator ist selbst abgesichert** (HA.4-Harness): Auf dem
  kleinen Raster wird jedes Flugzeug als genau ein Track bestätigt
  (PD ≥ 0,9, 0 Geister) — ein Benchmark über Input, dem der Tracker
  nicht folgen kann, misst Müll.

## Messwerte (2026-07-15, Release-Build, Sandbox-Host)

| Konstellation | Plots (60 s) | Zeit/Szenario | Durchsatz | Echtzeit-Last¹ | Reserve |
|---------------|--------------|---------------|-----------|-----------------|---------|
| 1 Radar × 10 Ziele | ~150 | 0,68 ms | ~221 k Plots/s | 2,5 Plots/s | ~88 000× |
| 1 Radar × 50 Ziele | ~750 | 4,7 ms | ~160 k Plots/s | 12,5 Plots/s | ~12 800× |
| 2 Radare × 50 Ziele | ~1 500 | 9,9 ms | ~151 k Plots/s | 25 Plots/s | ~6 000× |
| 3 Radare × 100 Ziele | ~4 500 | 39 ms | ~114 k Plots/s | 75 Plots/s | ~1 500× |

¹ dieselbe Konstellation live: Ziele × Radare / 4-s-Scan-Periode.

Lesart: In separationstreuem Verkehr ist der Tracker um Größenordnungen
schneller als Echtzeit; der Durchsatz sinkt moderat mit der Zieldichte
(mehr Gating-Kandidaten je Plot). Das ist die **Baseline** für CAP.2 —
dort kommen die Worst-Case-Formen (dichte Cluster) und daraus die
ehrlichen Auslegungsgrenzen.

## Ehrliche Grenzen

- **Host-abhängig:** Sandbox-Werte ≠ Zielhardware — vor einer
  Auslegungs-Aussage auf dem Zielsystem wiederholen
  (`cargo bench -p firefly-eval`).
- **Separationstreu:** dichte Konflikt-Cluster (JPDA-Kombinatorik) sind
  bewusst nicht Teil dieser Baseline (CAP.2).
- **Kein CI-Zeit-Gate:** Laufzeit-Schwellen in geteilten CI-Umgebungen
  sind konstruktionsbedingt flaky; Trends laufen über die
  criterion-Historie (`target/criterion/`), nicht über harte Asserts.
- Gemessen wird der Tracker-Kern (Fusion) — Encoder/Netz-Overhead ist
  vernachlässigbar klein dagegen, aber nicht Teil dieser Zahl.
