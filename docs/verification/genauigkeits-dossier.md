# Genauigkeits- und Verifikations-Dossier — Firefly SDPS

> **Anforderung:** NFR-ASSUR-001 (ASSUR.2) · **Stand:** 2026-07-16 ·
> **Zweck:** Jede Genauigkeits-, Robustheits- und Kapazitäts-**Behauptung**
> des Projekts an **einem Ort**, jeweils mit Beleg (Test/Bench/Referenz/
> Verfahren) und Reproduktions-Befehl — die Verifikations-Evidenz im Sinne
> der ED-153/ED-109A-Orientierung (ADR 0004), auf der ein unabhängiger
> Prüfer aufsetzen würde.
>
> **Pflege:** Bei jeder Änderung, die eine Zeile dieses Dossiers berührt
> (neue Messung, verschobene Grenze, neuer Nachweis), wird die Zeile
> aktualisiert — analog zur FHA-Pflege-Regel (CLAUDE.md §5).

## 0. Ehrliche Grenzen — zuerst

- **Simulator-Wahrheit:** Alle absoluten Genauigkeitszahlen (§2) messen
  gegen die exakte Wahrheit des eigenen Simulators (HA.4). Es gibt noch
  **keine** Messung gegen unabhängige Live-Referenzdaten; der
  COMPASS-Gegen-Check (NFR-SAFE-003) prüft Format/Konsistenz, nicht
  wahrheitsbasierte Genauigkeit. **Betreiber-Lauf ausstehend.**
- **Host-Abhängigkeit:** Alle Laufzeit-/Durchsatz-Zahlen (§4) stammen vom
  Sandbox-Entwicklungshost — Verhältnisse übertragbar, Absolutwerte auf
  Zielhardware wiederholen.
- **Coverage misst Ausführung, nicht Korrektheit:** 88 % Zeilen heißt
  „88 % wurden von mindestens einem Test durchlaufen", nicht „88 % sind
  richtig". Die Aussagekraft entsteht erst zusammen mit den inhaltlichen
  Prüfungen (§2/§3) — und den Instrument-Tests, die belegen, dass die
  Prüfungen **beißen**.
- Erstellt vom KI-Assistenten, geprüft vom Projektverantwortlichen —
  keine unabhängige V&V (ADR 0004, „ehrliche Grenze").

## 1. Test-Abdeckung (Coverage)

Gemessen mit `cargo llvm-cov --workspace --summary-only`
(cargo-llvm-cov 0.8.7, Release-Toolchain, 2026-07-16; inkl. der
Property-Tests aus §3):

| Bereich | Zeilen | Abdeckung |
|---------|-------:|----------:|
| **Workspace gesamt** | ~21 000 | **88 % Zeilen / 88 % Regionen** |
| `firefly-track` (Tracker-Kern: IMM/JPDA/Fusion) | 5 201 | **98,4 %** |
| `firefly-asterix` (Draht-Encoder/-Decoder) | 4 381 | 93,0 % |
| `firefly-geo` (Geodäsie) | 223 | 97,8 % |
| `firefly-sim` / `firefly-player` | 805 | 95,8 / 99,0 % |
| `firefly-multicast` | 698 | 94,6 % |
| `firefly-server` | 5 381 | 74,6 % |
| übrige Quell-Adapter (opensky/flarm/radar/adsb021/mlat/adsbagg) | ~2 800 | 76–93 % |
| `firefly-recorder` | 324 | 60,8 % |

**Ehrliche Analyse der Ausreißer** (statt sie wegzuerklären):

- `firefly-server` **74,6 %** liegt fast vollständig an `main.rs`
  (1 075 Zeilen, **0 %**): der Binary-Einstieg (Env-Verdrahtung,
  Task-Spawns) läuft in keinem Unit-Test. Die dort verdrahtete **Logik**
  ist in die getesteten Module extrahiert (`live.rs` 91,7 %,
  `standby.rs` 92,8 %, `sources.rs` 94,7 %, `metrics.rs` 100 %,
  `app.rs`-Handler über Router-Tests); das Restrisiko ist
  Verdrahtungs-Reihenfolge — durch Betrieb/Deployment-Smoke abgedeckt,
  nicht durch Unit-Tests. Gleiche Signatur bei `firefly-recorder`/
  `firefly-eval`: die **CLI-Einstiege** sind ungetestet, die Bibliothek
  dahinter ist es.
- `firefly-opensky` 76,7 %: der ungetestete Teil ist im Wesentlichen der
  echte HTTP-Pfad (OAuth-Refresh gegen den Live-Dienst); die
  Fehler-Klassifikation und das Parsing sind getestet.

**Reproduktion:** `cargo llvm-cov --workspace --summary-only`
(einmalig `rustup component add llvm-tools-preview &&
cargo install cargo-llvm-cov`). Kein CI-Schwellwert-Gate im Repo —
Coverage-Prozente als harte CI-Grenze erzeugen Schwellwert-Kosmetik;
die Zahl gehört ins Dossier, Trends in die CI-Historie.

## 2. Tracking-Genauigkeit (gemessen, HA.4 / FR-TRK-051)

Messstand `firefly-eval` (Lib + CLI): exakte Simulator-Wahrheit
(`TruthTrajectory`), bewertet wird das **projizierte Ausgabe-Bild**
(`snapshot_at` je 1-s-Tick — nicht der Last-Update-Zustand; der
Erst-Entwurf maß falsch und überschätzte den RMSE ×6, dokumentiert in
HA.4), Zuordnung greedy-exklusiv im 500-m-Gate, produktive
Tracker-Konfiguration.

| Kenngröße (Single-Target-Benchmark, 2026-07-15) | Messwert | CI-Gate |
|---|---|---|
| Track Probability of Detection (PD) | **0,967** | ≥ 0,95 |
| Positions-RMSE (projiziertes Bild) | **45,6 m** | < 60 m |
| Track-Identitäten je Ziel / ID-Switches | 1 / 0 | == 1 / == 0 |
| Bestätigungs-Latenz | **9 s** | ≤ 15 s |
| Falsch-Tracks | 0 | == 0 |

**Instrument-Tests** (die Messlatte beißt nachweislich): degradierte
Detektion (PD 0,5) senkt die gemessene PD messbar
(`pd_metric_drops_under_degraded_detection`); ein vorenthaltendes
Wahrheits-Set macht den korrekten Track zum gezählten Falsch-Track
(`false_track_metric_counts_unmatched_tracks`).

**Reproduktion:** `cargo run -p firefly-eval` (Text) bzw. `--json`;
Regression-Gates laufen in jedem `cargo test --workspace`.

## 3. Draht- und Parser-Korrektheit

| Behauptung | Beleg | Reproduktion |
|---|---|---|
| CAT062/063/065-Encoding ist **byte-genau** gegen die EUROCONTROL-Spezifikation | Referenz-Dump-Tests (u. a. `single_track_matches_reference_dump`, CAT063/065-Pendants, I062/390-Referenz-Bytes ICD §4.10) | `cargo test -p firefly-asterix` |
| `decode(encode(x)) = x` **LSB-genau für beliebige Tracks** — Position ≤ 180/2²⁵ °, Geschwindigkeit ≤ 0,25 m/s, FL ≤ 25 ft, Identität exakt | **Property-Tests** (proptest, 256 Zufallsfälle je Lauf, shrinkend): `cat062_kinematics_round_trip_within_lsb`, `cat062_identity_round_trip` | `cargo test -p firefly-asterix --test properties` |
| Alle drei Block-Decoder sind **total** über beliebige Bytes (kein Panic, kein Hänger) | Fuzzing (NFR-SAFE-002, echter Befund: FSPEC-u8-Überlauf → `MAX_FSPEC_OCTETS`) + Property `decoders_are_total_over_arbitrary_bytes` in jedem Testlauf | `cargo test -p firefly-asterix`; Fuzz-Targets s. NFR-SAFE-002 |
| Geodäsie-Roundtrip WGS84 ↔ ENU ist Identität **< 0,1 mm** für beliebige Ursprünge (±80°) und Punkte ±2°/0–20 km Höhe | Property `wgs84_enu_round_trip_is_identity` — mit zwei dokumentierten Funden **gegen die eigenen Test-Entwürfe**: (1) Toleranz gemessen kalibriert (reale f64-Restfehler ~1,4 µm @ 12,8 km, ~3,5 µm @ 20 km — die 1-µm-Wunsch-Schranke fiel durch); (2) **Antimeridian**: ±180°-überschreitende Längen kommen korrekt in den Hauptbereich normalisiert zurück (−180,58° ↔ +179,42° = derselbe Punkt) — Längen-Vergleich daher modulo 360°. Beide Funde als Kalibrier-Protokoll im Test-Kommentar | `cargo test -p firefly-geo --test properties` |
| Squawk wird **oktal wie geschrieben** gelesen; jede 8/9-Ziffer ist ein lauter Fehler — für **alle** 4 096 Codes und beide Schreibweisen | Property-Tests `every_squawk_round_trips_as_number_and_string`, `any_digit_eight_or_nine_is_rejected` | `cargo test -p firefly-fpl --test properties` |
| Unabhängiger Fremd-Decoder liest unseren Strom | COMPASS-Gegen-Check-**Verfahren** (NFR-SAFE-003, Checkliste C1–C6) | `docs/verification/compass-gegen-check.md` — **Betreiber-Lauf ausstehend** |

## 4. Kapazität & Auslegungsgrenzen (gemessen, CAP.1/CAP.2)

Details und Tabellen: `docs/TECHNICAL.md` §11. Kernzahlen
(Release-Build, Sandbox-Host, 2026-07-15/16):

| Behauptung | Messwert | Beleg |
|---|---|---|
| Durchsatz separationstreuer Verkehr | 114 k–221 k Plots/s (≥ 1500× Echtzeit) | Bench `tracker_load` (NFR-CAP-001) |
| JPDA-Worst-Case ist **begrenzt** | 10er-Kolonne: 27,8 s → **0,75 ms** durch Cluster-Kappe 8/10; teuerster exakter Fall ≈ 160 ms an der Kappe | Bench `dense_cluster`, Test `dense_column_is_bounded_by_the_cluster_cap` (FR-TRK-052) |
| Kein Datenverlust unter Normal-Last, Verlust unter Überlast **gezählt** | `firefly_live_plot_batches_dropped_total` | Back-Pressure-Design FR-OPS-007 |

**Reproduktion:** `cargo bench -p firefly-eval`.

## 5. Robustheits- und Sicherheits-Nachweise (Verweis)

- **FHA** mit 24 Gefährdungen, Barrieren-Trace und Lücken-Register:
  `docs/safety/FHA.md` (NFR-SAFE-004; L1 durch SAFE.4 geschlossen).
- **Determinismus/Reproduzierbarkeit:** gleicher Eingang ⇒ gleicher
  Ausgang (NFR-REPRO-001); jeder Betriebs-Lauf per `.ffplots`/`.ffrec`
  bit-genau nachstellbar (FR-OPS-005/006).
- **Kein `unsafe`** ohne dokumentierte Begründung (NFR-SAFE-001; aktuell:
  keines im Workspace).
- **Rückverfolgbarkeit:** Anforderungs-Register
  `docs/requirements/README.md` — jede Zeile mit Code-/Test-Trace.

## 6. Was dieses Dossier NICHT belegt

Damit niemand mehr hineinliest, als drinsteht: keine Live-Genauigkeit
gegen unabhängige Referenz (§0), keine quantitative Zuverlässigkeit
(Ausfallraten), keine Aussage über Wayfinder (eigenes Repo, eigene
Nachweise), keine formale Zertifizierung (organisatorisch-regulatorisch,
außerhalb dieses Code-Projekts — ADR 0004).
