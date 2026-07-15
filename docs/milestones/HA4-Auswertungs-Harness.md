# HA.4 — Auswertungs-Harness (Tracker-Güte messen statt glauben)

> **Anforderung:** FR-TRK-051 · **ADR:** — (Verifikations-Baustein zu
> ADR 0004/0006) · **ICD:** unberührt · **Einstufung:** S4 · umgesetzt
> auf Fable 5

## Fachlich

Bisher war die Tracker-Güte durch verstreute Einzeltests belegt. Jetzt
gibt es den **Messstand** (das Pendant zu EUROCONTROLs SASS-C, das für
uns nicht verfügbar ist — siehe Abstimmung mit dem Betreiber
2026-07-15): Ein Szenario mit exakt bekannter Wahrheit läuft durch den
produktiv konfigurierten Tracker, heraus kommt ein **Güte-Bericht** mit
den Standard-Metriken. Jede künftige Änderung (Tuning, CAP-Optimierung,
Refactoring) wird damit messbar besser oder schlechter — mit Zahl,
nicht mit Gefühl. Die Aussagekraft kommt nicht vom Werkzeug, sondern von
den **öffentlichen Metrik-Definitionen** (ESASSP) plus offener,
deterministisch nachrechenbarer Berechnung.

## Metrik-Mapping (ESASSP-Orientierung)

| Bericht-Feld | ESASSP-Intention | Berechnung im Harness |
|--------------|------------------|----------------------|
| `track_pd` | Track Probability of Detection: Anteil der Existenzzeit, den ein System-Track abdeckt | abgedeckte Ticks / Wahrheits-Ticks (Tick 1 s) |
| `position_rmse_m` | Horizontale Positionsgenauigkeit (RMS-Fehler) | RMS der Distanz Wahrheit ↔ zugeordneter Track über abgedeckte Ticks |
| `track_ids`, `id_switches` | Track-Kontinuität: ein Flugzeug = **ein** Track | distinkte IDs je Flugzeug; Identitätswechsel via `TrackContinuity` |
| `false_tracks` | Falsch-Track-Anteil | bestätigte Tracks, die **nie** ein Wahrheits-Flugzeug repräsentierten |
| `confirmation_latency_s` | Initiierungs-Verzug | erste Abdeckung nach Wahrheits-Beginn |

Hinweis: ESASSP definiert die Metriken über realen Verkehr mit
rekonstruierter Referenz; wir übernehmen **Namen und Intention** und
messen gegen **exakte** Simulator-Wahrheit — methodisch sauberer, aber
nur so breit wie das Simulationsmodell (s. Grenzen).

## Technik

- **Neue Crate `firefly-eval`** (Bibliothek + CLI
  `firefly-eval [--json] [szenario…]`): `tracker_for` baut den Tracker
  **wie die Live-Verdrahtung** (jedes Radar mit eigenem Site-Frame und
  echtem Fehlermodell); `evaluate` speist die Simulator-Plots im
  asynchronen Pfad (`process_plots`) ein und bewertet je 1-s-Tick.
- **Bewertet wird das projizierte Ausgabe-Bild** (`snapshot_at`), nicht
  der Last-Update-Filterzustand. Die wichtigste Korrektur unterwegs: der
  Erst-Entwurf maß den Last-Update-Zustand und lastete dem Tracker bis
  zu einem ganzen Scan Eigenbewegung als Positionsfehler an (RMSE ~288 m
  statt ~46 m) — ESASSP misst den *Output*, also tun wir das auch.
- **Wahrheits-Zuordnung:** greedy nächster-Nachbar im Gate (500 m
  Default), je Tick beidseitig exklusiv (ein Flugzeug ↔ höchstens ein
  Track), deterministisch sortiert. `firefly-sim::TruthTrajectory` ist
  dafür öffentlich geworden — Messung und Messdaten teilen per
  Konstruktion ein Trajektorien-Modell.
- **Bericht:** Text (Mensch) + stabiles JSON (CI-Trend); byte-identisch
  reproduzierbar (NFR-CLOUD-001).
- **Instrument-Tests** (SPEC.1-Lektion „ein Test muss beißen", auf das
  Messgerät angewandt): degradierte Detektion (PD 0,5) senkt den Score
  messbar; ein der Wahrheit vorenthaltenes Flugzeug erscheint als
  Falsch-Track. **Regression-Gates** am ehrlichen Ist-Stand kalibriert
  (Single-Benchmark 2026-07-15: PD 0,967 · RMSE 45,6 m · Latenz 9 s ⇒
  Gates PD ≥ 0,95 / RMSE < 60 m / Latenz ≤ 15 s / 1 ID / 0 Geister).

## Ehrliche Grenzen

- **Misst nur, was der Simulator modelliert:** kein Clutter-Modell ⇒ das
  Falsch-Track-Gate ist konstruktiv 0; reale Sensor-Pathologien
  (Mehrwege, Splits, Duplikate) deckt erst der Betrieb bzw. der
  unabhängige Gegen-Check auf.
- **Selbst gebaut = selbst benotet?** Entkräftung dreifach: öffentliche
  Metrik-Definitionen, offener deterministischer Code, und der geplante
  **unabhängige Gegen-Check mit OpenATS COMPASS** über den echten
  CAT062-Mitschnitt (**HA.5**, eigene Roadmap-Zeile). Das formale
  Qualitätssiegel bleibt, wie in ADR 0004 abgegrenzt, ein
  regulatorischer Akt außerhalb dieses Projekts.
- Live-Mitschnitte ohne Wahrheit können nur wahrheitsfrei bewertet
  werden (Track-Stabilität, Coasting-Anteile) — bewusst noch nicht
  implementiert, um keine Schein-Genauigkeit anzubieten.
- Die Frankfurt-Mehrradar-Szene bleibt vorerst beim bestehenden
  Regressions-Fixture (`firefly-player`); ihre Aufnahme in die
  Benchmark-Suite ist ein natürliches Folge-Häppchen.
