# Arbeitsstand (Handover-Notiz)

> **Zweck:** Diese Datei ist der schnelle Wiedereinstieg — egal ob am PC oder
> Handy. Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

- **Zuletzt aktualisiert:** 2026-06-12 (Branch `claude/branch-merge-main-cqzdwk` stabilisiert + nach `main` gemergt; ADR 0013 angenommen, Umsetzung ausstehend)
- **Branch:** `main` — grün und stabil (M1–M6, Stand M6.5). Branch
  `claude/branch-merge-main-cqzdwk` wurde aufgeräumt und gemergt.

> 🔭 **NÄCHSTE WOCHE — ADR 0013 (asynchrone Pro-Plot-Verarbeitung) umsetzen.**
> Die Architektur-Entscheidung ist **angenommen** (`docs/decisions/0013-…md`), die
> Umsetzung steht noch aus. Ein erster Foundation-Schritt (Simulator: azimut-abhängige
> Pro-Plot-Zeitstempel, `scan_offset` entfernt) wurde begonnen — Commit **`6a58a03`** —
> und bewusst wieder **zurückgenommen** (Revert **`0959059`**), weil ohne die Tracker-
> und Server-Teile der Frankfurt-Test rot wird (155 statt 8 Track-IDs). `main` ist
> deshalb wieder grün. **Wiedereinstieg:** der Abschnitt *„Umsetzungsstand /
> Wiedereinstieg"* in ADR 0013 enthält den vollständigen Häppchen-Plan
> (**13.1 – 13.7**, beginnend mit `Tracker::process_plot`). Vorgehen wie immer:
> *erst erklären, dann bauen* (CLAUDE.md §2).

- **Diese Sitzung (Aufräumen + Merge):**
  - Offener Branch endete mit unfertigem ADR-0013-WIP, der ein Qualitäts-Gate verletzte
    (`cargo test` rot). WIP per `git revert` zurückgenommen → **alle 32 Suites grün**,
    Clippy sauber, `cargo fmt` ok.
  - ADR 0013 von „in Entscheidung" auf **„akzeptiert — Umsetzung ausstehend"** gesetzt
    und um einen ausführlichen **Wiedereinstiegs-Abschnitt** (Foundation, Grund der
    Rücknahme, Häppchen-Plan 13.1–13.7, Anker-Commits) ergänzt.
  - Branch nach `main` gemergt und gepusht.
- **Abgeschlossen — M6.5 ✅ (Nachträge, gemergt zu `main`):**
  - **Server-seitige Roh-Plot-Geolokation:** `Player::frames()` rechnet jeden Plot über
    `Polar::to_enu()` + `LocalFrame::enu_to_geodetic()` (sensorbezogen, `TrackerConfig.sensors`)
    nach WGS84 um; neue `Tracker::config()`-Zugriffsmethode, `FramePlot` jetzt aus `firefly-io`
    re-exportiert. Das „raw plots"-Overlay zeigt jetzt echte Positionen statt einer leeren Liste.
  - **Coasting-Anzeige entflackert:** Bei mehreren Radaren mit `scan_offset` (ADR 0012) wurde der
    rohe Pro-Scan-`coasting`-Status sichtbar geflackert (Blau↔Orange), obwohl der Track insgesamt
    aktuell war. Frontend zeigt „coasting" jetzt erst, wenn `update_age_s` über
    `COAST_DISPLAY_THRESHOLD_S = 5.0` (eine Scan-Periode) liegt.
  - Doku: `docs/milestones/M6-showcase.md` (M6.3-Abschnitt ergänzt, „Ausblick" bereinigt).
  - **Verzug-Demo holt auf (Dead-Reckoning + Snap):** Server taktet jetzt **absolut**
    (`pacing::due_at`), sodass nach dem 5-s-Verzug die aufgelaufenen Frames durchlaufen und
    das Bild zur Gegenwart aufholt. Das Frontend extrapoliert die Tracks während der Lücke aus
    ihrer Geschwindigkeit und schnappt beim nächsten echten Frame ein. Die `delay_triggered`-
    Nachricht trägt jetzt `speed`. Track-Strom unangetastet (NFR-CLOUD-004).
  - **History-Trail (Kometenschweif):** Roh-Plots und vergangene Track-Positionen bleiben als
    verblassende Spur (Frontend), behebt auch das kurze Aufblitzen der Plots.
  - **Realistische, gemischte Scan-Perioden:** Frankfurt-Radare 4 s (Anflug) / 10 s / 12 s
    (En-Route) statt einheitlich 4 s; 8 stabile IDs bleiben (ADR 0012).
  - Glossar: Dead-Reckoning, History-Trail/Kometenschweif ergänzt.
  - **Alle Tests grün (32 Suites), Clippy sauber, `cargo fmt` ok, JS-Syntax geprüft.**
- **Live-Debugging (diese Sitzung, direkt auf `main`):** Beim ersten Start über Docker blieb die
  Karte leer — behoben: ungültige Font-Awesome-Glyphs-URL entfernt, Track-Label-Layer komplett auf
  HTML-Marker (`maplibregl.Marker`) umgestellt (kein externer Glyphs-Server mehr nötig).
- **PR:** Entwicklung abgeschlossen — PR erstellt und zu `main` gemergt (55 Commits,
  Meilensteine M1–M6 vollständig).

---

## 1. Wo wir gerade stehen

- **M1 (Simulator) ist fertig** und gepusht: Workspace + drei Crates
  (`firefly-geo`, `firefly-core`, `firefly-sim`).
- **M2 läuft:** Häppchen **2.1–2.8 erledigt (M2 abgeschlossen)** — Crate `firefly-track` mit
  Converted-Measurement, Kalman-Filter (CV, Joseph-Form), Gating (Mahalanobis/χ²),
  Datenassoziation (GNN/Ungarische Methode) und **Track-Lebenszyklus** (`Tracker`,
  Pro-Scan-Orchestrierung: Geburt/Bestätigung/Coasting/Löschung). Der
  Single-Radar-Tracker steht — inkl. End-to-End-Test mit zwei kreuzenden Zielen.
  **2.6**: serialisierbarer Zustand mit Snapshot/Replay (serde, ADR 0007).
  **2.7**: neutraler `SystemTrack`-Output in WGS84 (`firefly-core`) + Projektion
  `Tracker::system_tracks(&LocalFrame)` — der ASD-Port Richtung CAT062.
  **2.8**: Güte-Metriken (`Rmse`, `TrackContinuity`) gegen Ground Truth; E2E-Test
  mit Positions-RMSE < 40 m, 0 ID-Wechsel, Coverage > 90 %.
  **Nachtrag (ADR 0008, FR-TRK-008):** der `SystemTrack` trägt jetzt den
  safety-relevanten Status — `coasting`, `update_age`, `position_uncertainty`
  (1σ-Halbachse aus `P`) → bereitet CAT062 I062/080, /290, /500 vor.
  **Härtung (NFR-CLOUD-004):** `tests/timing.rs` beweist — lange Scan-Lücke mit
  Daten erhält Identität; Löschung nach Fehltreffer-*Anzahl*, nicht nach Zeit.
  Externe Abhängigkeiten `nalgebra` (ADR 0005), `serde` (ADR 0007).
- Die **Arbeitsregeln** stehen (`CLAUDE.md`): *erst erklären, dann bauen*;
  keine unerklärten Begriffe; Doku ist Teil der Leistung.
- **M3 läuft:** Häppchen **3.0–3.4 erledigt.**
  **3.0**: Architektur-Entscheidung (ADR 0009): Async-Server **Tokio + axum**,
  Transport **WebSocket**, erster Ausgabe-Adapter **JSON**, Karten-Frontend
  **MapLibre GL** (GPU-Vektorkarte, anbieter-neutral, mit Blick auf M4).
  **3.1**: neue Crate **`firefly-io`** mit dem neutralen `Frame` (Datenzeit +
  Sensor + `SystemTrack[]`) und `FrameTrack`; web-freundliche Drahtform (Position
  in **Grad**, abgeleitete Geschwindigkeit/Kurs), verlustfreier JSON-Roundtrip
  (FR-IO-001).
  **3.2**: neue Crate **`firefly-player`** — `Player::new(&scenario, config)` +
  `.frames()` führt das Szenario (M1) durch den Tracker (M2) und erzeugt den
  **Frame-Strom** (ein `Frame` je Scan-Zeit) über `firefly-io` (FR-IO-002). Rein
  und deterministisch.
  **3.3**: neue Crate **`firefly-server`** (axum/Tokio) streamt den Frame-Strom
  über **WebSocket** (`/ws`) an den Browser, getaktet nach Datenzeit am
  Ausgabe-Rand (Tempo-Faktor `FIREFLY_SPEED`); dazu Health-/Readiness-Probes,
  12-Factor-Config (`FIREFLY_PORT`), geordnetes Shutdown (SIGTERM) und
  strukturierte `tracing`-Logs (FR-NET-001, NFR-OBS-001). Start mit **einem
  Befehl**: `cargo run -p firefly-server` → `http://localhost:8080`.
  **3.4**: **MapLibre-Frontend** (`crates/firefly-server/static/index.html`, ins
  Binary eingebettet) — 2D-Karte (Stil `demotiles.maplibre.org`), die den
  `/ws`-Strom konsumiert und je Frame die Tracks zeichnet. Safety-Status sichtbar
  (ADR 0008): Farbe nach confirmed/tentativ/coasting, Unsicherheits-Ring
  (gestrichelt beim Coasting), Geschwindigkeitsvektor (FR-UI-001).
  **Bugfix (nach Sichtprüfung im Browser):** Die Track-IDs zählten hoch (statt
  zwei stabile Tracks gab es bis zu #21). Zwei Ursachen — beide behoben:
  *(1)* Die umgerechnete Mess-Kovarianz ignorierte das **Höhenwinkel-Rauschen**;
  bei 10–11 km Höhe streut das die Bodenentfernung um ~175 m, das Gate war viel
  zu eng → Plot fiel raus → Dublette. Jetzt fließt der Term
  `σ_ρ² = (cos φ·σ_r)² + (r·sin φ·σ_φ)²` ein (FR-TRK-002, `from_polar_deg`).
  *(2)* Das **Prozessrauschen** des CV-Filters war auf Geradeausflug getunt und
  zu klein für die 1°/s-Kurve → das kurvende Ziel zerbrach; Demo nutzt jetzt
  `accel_psd ≈ 60` (passt zum Manöver). Ergebnis: je Flugzeug **eine** stabile
  ID (Regressionstests `identity::*`, `scene::demo_scene_keeps_one_identity_*`).
  **3.5**: „Verzug"-Auslöser (NFR-OPS-001/NFR-CLOUD-004) — Knopf „Verzug
  simulieren (5 s)" im Frontend schickt `"delay"` über `/ws`; `pump_frames`
  bestätigt mit `delay_triggered` und pausiert die Zustellung um 5 s
  (`DELAY_TRIGGER_PAUSE`), rein an der Auslieferungs-Kante — der Frame-Strom
  selbst bleibt unverändert. Test
  `websocket::delay_trigger_pauses_delivery_without_corrupting_the_stream`.
  **M3 ist damit abgeschlossen** (`docs/milestones/M3-live-picture.md`).
- **Häppchen B (ADR 0006-Nachtrag) erledigt:** Transport- und
  Koordinatenfrage geklärt — **UDP-Multicast** + **System-Stereografisch**
  (CAT062 I062/100). Noch nicht umgesetzt; als Zielbild in ADR 0006
  festgehalten.
- **M4 ist abgeschlossen** (`docs/milestones/M4-multi-radar-fusion.md`):
  **4.1**: `Track` (firefly-track) merkt sich die SSR-Identität (Mode-3/A,
  ICAO-Adresse) aus zugeordneten Plots (`Track::update_identity`, sticky), und
  `SystemTrack` (firefly-core) führt sie als `mode_3a: Option<u16>` /
  `icao_address: Option<u32>` mit (FR-TRK-009).
  **4.0**: Architektur-Entscheidung **ADR 0010** — zentrale **Mess-Fusion**
  (Option A): ein Tracker, gemeinsamer Tracking-Frame, Plot-Umrechnung in
  diesen Frame (Position + Kovarianz), Pro-Sensor-Rauschmodell. Begründung:
  Präzision (Rohmessungen) bei gleicher Cloud-Tauglichkeit; Synergie mit dem
  System-Referenzpunkt der CAT062-Ausgabe (ADR 0006).
  **4.A.1**: `firefly-geo` bekommt die Frame-zu-Frame-Transformation für
  Position **und** Kovarianz (FR-GEO-003).
  **4.A.2**: `firefly-track` auf Multi-Sensor umgestellt — gemeinsamer
  `tracking_frame`, `BTreeMap<SensorId, SensorModel>`, sequenzielle
  Sensor-Verarbeitung löst das Geister-Problem (FR-TRK-010).
  **4.A.3**: Ende-zu-Ende-Test (zwei Radare, ein Flugzeug → ein Track,
  `firefly-player/tests/multi_radar.rs`).
  **4.A.4**: `SystemTrack.contributing_sensors` — welche Sensoren im letzten
  Scan beigetragen haben (nicht sticky, leer beim Coasten).
  **4.2**: CAT062-Identitätsfelder I062/060 (Mode 3/A) und I062/380/ADR
  (Mode-S-Adresse) — nur bei vorhandener Identität, automatisch im FSPEC.
- **Häppchen C.1 (ADR-0006-Nachtrag) erledigt:** `firefly-geo` bekommt
  `StereographicProjection` (FR-GEO-004) — konforme WGS84 ↔ System-
  Stereografisch-Projektion (konforme Breite + Gaußsche Hilfskugel +
  sphärische schiefachsige Stereografie, EUROCONTROL/ARTAS-Art).
- **Häppchen C.2 (ADR-0006-Nachtrag) erledigt:** `firefly-asterix` kodiert
  I062/100 (System-Stereografische Position, X/Y, FRN 6) zusätzlich zu
  I062/105 — Projektion über `StereographicProjection`, Referenzpunkt als
  Konstruktorparameter von `Cat062Encoder::new`.
- **Häppchen C.3 (ADR-0006-Nachtrag) erledigt:** neue Crate `firefly-multicast`
  versendet CAT062-Blöcke per UDP-Multicast (FR-NET-002), nach Datenzeit
  getaktet; 12-Factor-Config (`FIREFLY_CAT062_*`), per Default aus.
  `Player::scans()` als geteilte deterministische Basis beider Adapter; im
  Server-`main` als optionaler Task verdrahtet. **ADR 0006 (Transport &
  Koordinatenbezug) ist damit vollständig umgesetzt.**
- **Häppchen D.1 erledigt:** `firefly-asterix::decode_data_block()` dekodiert
  einen CAT062-Block zurück in `DecodedRecord`s (FR-IO-004) — FSPEC-Parsing
  (`fspec::parse`) + alle bisherigen Items außer der Rückprojektion von
  I062/100. Roundtrip-Tests gegen den eigenen Encoder.
- **Häppchen D.2 erledigt:** `unproject_cartesian_position` projiziert
  I062/100 (X/Y in Metern) über `StereographicProjection::unproject` zurück
  nach WGS84; stimmt mit der unabhängig kodierten I062/105-Position auf
  unter 1 m überein.
- **Häppchen D.3 erledigt:** `firefly-multicast::receiver` — `receiver_socket`
  tritt der Multicast-Gruppe bei, `recv_records`/`run` empfangen und
  dekodieren CAT062-Datagramme. Ende-zu-Ende-Test: Sender → Socket →
  Empfänger → Decoder liefert die ursprünglichen Track-Daten zurück.
- **M5 ist abgeschlossen** (`docs/milestones/M5-imm.md`): Häppchen
  **M5.1 – M5.4 (IMM)** —
  `firefly-track::motion` mit `MotionModel` (CV + Coordinated-Turn);
  `LinearKalman::predict_with` (FR-TRK-011). `firefly-track::imm` mit `Imm`:
  Filter-Bank + Markov-Übergangsmatrix, IMM-Mischung (FR-TRK-012) und der
  vollständige IMM-Zyklus mit Likelihood-Gewichtung (FR-TRK-013). Im Tracker
  wirksam (FR-TRK-014): jeder Track trägt eine IMM-Bank, Prädiktion/Update über
  `Imm::predict`/`Imm::update`, Gating über die kombinierte Schätzung.
  **M5.5 – M5.9 (JPDA)** — `firefly-track::pda` berechnet
  Assoziationswahrscheinlichkeiten `β` unter einem Clutter-Modell
  (FR-TRK-015); `LinearKalman::update_pda`/`Imm::update_pda` falten alle
  gegateten Plots gewichtet ein (FR-TRK-016/017); `firefly-track::jpda` löst
  die Exklusivität bei überlappenden Toren über Clustering + Ereignis-
  Aufzählung (FR-TRK-018); im Tracker ersetzt JPDA die harte GNN-Zuordnung je
  Sensor (FR-TRK-019), `TrackerConfig` trägt ein `ClutterModel`. E2E-Test:
  zwei eng benachbarte parallele Ziele bleiben zwei unterscheidbare,
  bestätigte Tracks („Track-Koaleszenz" als dokumentiertes JPDA-Merkmal).
- Qualität: **Alle Tests grün** (Workspace), Clippy sauber, `cargo fmt` ok. Sichtprüfung des
  Frontends im Browser ist ein manueller Schritt.
- **Dokumentation** aufgebaut: Glossar, M1-/M2-Erklärungen, ADRs 0001–0009,
  Anforderungs-Register mit Rückverfolgbarkeit.

## 2. Gesetzte Entscheidungen (Fundament, nicht mehr offen)

| Thema | Entscheidung | Quelle |
|-------|--------------|--------|
| Engine-Sprache | **Rust** (Frontend später JS) | ADR 0001 |
| Datenformat | **ASTERIX** (CAT048/021/062) | ADR 0001 |
| Erster Umfang | Simulator (M1) + Single-Radar-Tracker (M2) | ADR 0001 |
| Darstellung | **2D-Karte** | ADR 0001 |
| Sprache | Code Englisch, Doku/Chat Deutsch | ADR 0002 |
| Architektur | **Cloud-nativ**, Kubernetes, anbieter-neutral | ADR 0003 |
| Assurance | **Zertifizierungs-fähig**, ED-153 + ED-109A/DO-278A | ADR 0004 |
| Integration | Andocken an **Phoenix ASD**; Ausgabe **ASTERIX CAT062**; Kern neutral via Ports & Adapters | ADR 0006 |

## 3. Nächster Schritt (hier geht es weiter!)

✅ **M2 ist abgeschlossen** (inkl. Nachtrag: safety-relevanter `SystemTrack`-Status,
ADR 0008). Der Single-Radar-Tracker steht vollständig: Messung → Filter → Gate →
Zuordnung → Lebenszyklus → Snapshot/Replay → neutraler WGS84-Output mit
Safety-Status → Güte-Metriken.

✅ **Timing-Härtung (NFR-CLOUD-004) erledigt** — `tests/timing.rs` beweist beide
Eigenschaften. Damit ist M2 inkl. aller Veredelungen abgeschlossen.

✅ **M3 Häppchen 3.0–3.5 erledigt — M3 ist abgeschlossen.** ADR 0009 steht;
`firefly-io` → `Frame` als JSON (FR-IO-001); `firefly-player` →
deterministischer Frame-Strom (FR-IO-002); `firefly-server` streamt ihn live
über WebSocket (FR-NET-001); das **MapLibre-Frontend** zeigt die Tracks auf
einer 2D-Karte mit sichtbarem Safety-Status (FR-UI-001); ein „Verzug"-Knopf
macht die Timing-Robustheit (NFR-CLOUD-004) erlebbar (NFR-OPS-001). Komplette
Kette steht: ein Befehl → Live-Lagebild im Browser.
Meilenstein-Doku: `docs/milestones/M3-live-picture.md`.

✅ **Häppchen 3.X — CAT062-Encoder-Adapter abgeschlossen** (binäre
ASTERIX-Ausgabe neben JSON, ADR 0006):
- [x] **3.X.1** Crate `firefly-asterix`, Framing (CAT/LEN) + FSPEC/UAP-Mechanik
  + I062/010, /070, /040 (geometrie-frei). *S3 · Sonnet · Effort mittel*
- [x] **3.X.2** I062/105 (Position WGS84) + I062/185 (Geschwindigkeit kart.) mit
  Skalierungsfaktoren + Zweierkomplement. *S4 · Opus 4.8 · Effort hoch*
- [x] **3.X.3** I062/080 (Track-Status, variable Länge mit FX), I062/290, /500
  (Alter/Unsicherheit) — gegen EUROCONTROL SUR.ET1.ST05.2000-STD-09-01 Ed. 1.10
  verifiziert. *S4 · Opus 4.8 · Effort hoch*
- [x] **3.X.4** Adapter-Abschluss: Entscheidung gegen `Frame → CAT062`
  (ADR 0006-Nachtrag), Meilenstein-Doku `M3X-cat062-encoder.md`.
  *S3 · Sonnet · Effort mittel*

✅ **Häppchen B (ADR 0006-Nachtrag) erledigt:** Transport- und
Koordinatenfrage geklärt — **UDP-Multicast** + **System-Stereografisch**
(CAT062 I062/100). Noch nicht umgesetzt; Folge-Häppchen (Projektion +
I062/100-Encoder, Multicast-Versand) sind als Zielbild in ADR 0006
festgehalten und werden voraussichtlich im Umfeld von M4 eingeplant.

✅ **M4 ist abgeschlossen** (Häppchen 4.0 + 4.1 + 4.A.1–4.A.4 + 4.2,
Meilenstein-Doku `docs/milestones/M4-multi-radar-fusion.md`): SSR-Identität
durchgereicht (FR-TRK-009); Architektur entschieden — **zentrale Mess-Fusion**
(ADR 0010); Frame-Transform, Multi-Sensor-Tracker, E2E-Fusionstest,
Sensor-Provenienz und CAT062-Identitätskodierung alle umgesetzt.

✅ **Häppchen C.1 (ADR-0006-Nachtrag) erledigt:** `StereographicProjection` in
`firefly-geo` (FR-GEO-004) — konforme WGS84 ↔ System-Stereografisch-Projektion,
Referenzpunkt frei wählbar (für CAT062 später: `tracking_frame`-Ursprung).

✅ **Häppchen C.2 (ADR-0006-Nachtrag) erledigt:** I062/100-Encoder in
`firefly-asterix` — `Cat062Encoder::new(source, system_reference_point)`
projiziert jede Track-Position über `StereographicProjection` und kodiert
X/Y zusätzlich zu I062/105 (FRN 6, FSPEC-Bit 0x04 in Oktett 1).

✅ **Häppchen C.3 (ADR-0006-Nachtrag) erledigt:** neue Crate `firefly-multicast`
— `MulticastConfig` (12-Factor, `FIREFLY_CAT062_*`, Default aus),
`sender_socket()` + `run()` versenden je Scan einen CAT062-Block per
UDP-Multicast, nach Datenzeit getaktet (FR-NET-002). `Player::scans()` als
geteilte deterministische Basis; im Server-`main` als optionaler Task
verdrahtet. **Damit ist ADR 0006 (ASD-Integration, Transport &
Koordinatenbezug) vollständig umgesetzt.**

✅ **Häppchen D.1 erledigt:** CAT062-Decoder (`decode_data_block`,
`fspec::parse`, FR-IO-004) — Umkehrung des Encoders für FSPEC + alle Items
außer der I062/100-Rückprojektion.

✅ **Häppchen D.2 erledigt:** `unproject_cartesian_position` projiziert
I062/100 (X/Y in Metern) über `StereographicProjection::unproject`
(FR-GEO-004) zurück nach WGS84; stimmt mit der unabhängig kodierten
I062/105-Position auf unter 1 m überein.

✅ **Häppchen D.3 erledigt:** `firefly-multicast::receiver` —
`receiver_socket` bindet und tritt der Multicast-Gruppe bei, `recv_records`/
`run` empfangen und dekodieren CAT062-Datagramme. Ende-zu-Ende-Test:
Sender → Socket → Empfänger → Decoder liefert Kinematik, Status, Identität
und I062/100-Position originalgetreu zurück. **Häppchen D ist damit
abgeschlossen.**

✅ **Häppchen M5.1 erledigt:** zweites Bewegungsmodell. Neues Modul
`firefly-track::motion` mit `MotionModel` (`ConstantVelocity`,
`CoordinatedTurn { rate }`). Die CT-Übergangsmatrix dreht den
Geschwindigkeitsvektor um `ω·dt` und integriert den Bogen in die Position;
`ω → 0` ergibt exakt CV. `LinearKalman::predict_with(&MotionModel, …)` als Haken
für mehrere parallele Modelle (FR-TRK-011).

✅ **Häppchen M5.2 erledigt:** IMM-Mischungs-Schritt. `firefly-track::imm`
mit der Struktur `Imm` (Filter-Bank + Modellwahrscheinlichkeiten +
Markov-Übergangsmatrix). Implementiert die Interaktions-/Mischungs-Stufe:
`predicted_model_probabilities`, `mixing_probabilities`,
`mixed_initial_conditions` (gemischter Mittelwert + Kovarianz mit
Spread-of-the-Means-Term) — FR-TRK-012.

✅ **Häppchen M5.3 erledigt:** vollständiger IMM-Zyklus. `Imm::step(dt, Q,
Option<&measurement>)` — modellbedingtes Filtern aus der gemischten
Anfangsbedingung, Gauß-Likelihood je Modell
(`LinearKalman::measurement_likelihood`), Modellwahrscheinlichkeits-Update
`μ_j ∝ c_j·Λ_j` (Coasting → Markov-Prädiktion) und die kombinierte Schätzung
`combined_estimate`. Konvergenz nachgewiesen: CV gewinnt auf der Geraden, CT in
der Kurve (FR-TRK-013).

✅ **Häppchen M5.4 erledigt:** IMM im Tracker. Jeder `Track` trägt eine
IMM-Bank statt eines einzelnen `LinearKalman`; `ImmConfig` in `TrackerConfig`,
Prädiktion/Update über `Imm::predict`/`Imm::update`, Gating über
`Track::estimate`. Geradeausflug-Verhalten erhalten; kurvendes Ziel → passendes
CT-Modell dominiert (FR-TRK-014). **IMM-Teil von M5 abgeschlossen.**

✅ **Häppchen M5.5 erledigt:** PDA-Assoziationswahrscheinlichkeiten `β`. Neues
Modul `firefly-track::pda` mit `ClutterModel` und
`association_probabilities` — `β_0` (kein Treffer) und `β_j` je gegatetem
Plot, aus Likelihood vs. Clutter-Term `b` (FR-TRK-015).

✅ **Häppchen M5.6 erledigt:** PDA-gewichtetes Kalman-Update.
`LinearKalman::update_pda` faltet alle Hypothesen (kein Treffer + je Plot)
gewichtet ein, Spread-of-the-Means wie beim IMM-Mixing (FR-TRK-016).

✅ **Häppchen M5.7 erledigt:** PDA-gewichtetes IMM-Update. `Imm::update_pda`
überträgt M5.6 auf die ganze Modell-Bank (Zweig 0 = Bank nach `predict`,
Zweig `1+j` = `Imm::update(measurements[j])` auf Kopie der Bank), mischt
Modellzustände und -wahrscheinlichkeiten `β`-gewichtet (FR-TRK-017).

✅ **Häppchen M5.8 erledigt:** JPDA-Kern. Neues Modul `firefly-track::jpda`
mit `joint_association_probabilities` — Clustering (Union-Find) +
erschöpfende Aufzählung zulässiger gemeinsamer Zuordnungen je Cluster, daraus
marginalisierte `β_ij` mit Exklusivität (FR-TRK-018).

✅ **Häppchen M5.9 erledigt:** JPDA im Tracker. Die harte GNN-Zuordnung ist je
Sensor durch JPDA ersetzt; `Imm::update_pda` faltet alle gegateten Plots eines
Tracks gewichtet ein, der Plot mit größtem `β` liefert die Identität;
`TrackerConfig` trägt ein `ClutterModel` (~1 Falschplot/10 km², getunt gegen
RMSE-Regression). E2E-Test: zwei eng benachbarte parallele Ziele bleiben zwei
unterscheidbare, bestätigte Tracks — „Track-Koaleszenz" als dokumentiertes
JPDA-Merkmal (FR-TRK-019). **M5 (IMM + JPDA) ist damit vollständig
abgeschlossen.**

✅ **M6.1 — Frankfurt-Showcase-Szene erledigt** (`docs/milestones/M6-showcase.md`):
drei Radare, acht Flugzeuge (JPDA-Nahpaar, IMM-Manöver, SSR/primary-only,
Warteschleife, Multi-Radar-Überlappung), acht stabile Track-IDs über 240 s,
`FIREFLY_SCENE=frankfurt` zur Szenenauswahl (12-Factor).

✅ **M6 — Frontend-Showcase + Container ist abgeschlossen:**
- ✅ **M6.2** OpenStreetMap-Hintergrundkarte + Airspace-Overlay (GeoJSON, Layer-Toggle).
- ✅ **M6.3** Roh-Plot-Transparenz-Ebene (zeigt Radar-Plots vor Tracker-Verarbeitung).
- ✅ **M6.4** Docker-Containerisierung (Multi-Stage-Build, docker-compose, DOCKER.md).
- ✅ **M6.5** Nachträge: Server-seitige polar→WGS84-Konvertierung für `plots`,
  entflackerte Coasting-Anzeige, Verzug-Aufholen mit Dead-Reckoning,
  History-Trail (Kometenschweif) und realistische gemischte Scan-Perioden.

✅ **PROJEKT ABGESCHLOSSEN**

Alle Meilensteine (M1–M6) sind implementiert und zu `main` gemergt. Der Radar-Tracker steht end-to-end:
- **M1:** Simulator mit realistischen Szenen (Frankfurt: 3 Radare, 8 Flugzeuge)
- **M2:** Single-Radar-Tracker (Kalman, GNN, Gating, Lebenszyklus)
- **M3:** Live-Lagebild (WebSocket, MapLibre, CAT062-Kodierung)
- **M4:** Multi-Radar-Fusion (zentrale Mess-Fusion, SSR-Identität)
- **M5:** Manöver + dichter Verkehr (IMM, JPDA)
- **M6:** Showcase + Cloud (Docker, realistische Szenen, Dead-Reckoning, History-Trail)

Alle Qualitäts-Gates erfüllt: Tests ✅, Clippy ✅, Doku ✅, Cloud-native ✅, Zertifizierungsfähig ✅.

➡️ **Aktiver nächster Schritt — ADR 0013 umsetzen** (angenommen, Umsetzung
ausstehend): asynchrone Pro-Plot-Verarbeitung + periodischer Ausgabetakt. Der
vollständige Häppchen-Plan (13.1 `process_plot` → … → 13.7 CAT062 aus Snapshot)
und die Wiedereinstiegs-Anker stehen im Abschnitt *„Umsetzungsstand /
Wiedereinstieg"* von `docs/decisions/0013-…md`. Foundation-Commit `6a58a03`
(zurückgenommen via `0959059`).

➡️ **Weitere mögliche Fortsetzungen** (offene Punkte aus Abschnitt 5):
1. Live-OpenAIP-API-Integration statt statische Airspaces-GeoJSON.
2. Sensor-Registrierung / Bias-Korrektur (M4-Nachtrag, S5).
3. FHA / Hazard-Analyse (Sicherheit, S4).
4. Coverage-Werkzeug (Visualisierung, S3).
5. Out-of-Order-Eingang (Robustheit, S3).

### M5-Plan in Häppchen (abgeschlossen)

- [x] **M5.1** Zweites Bewegungsmodell: Coordinated-Turn-Übergangsmatrix neben CV, gemeinsamer 4-D-Zustand (FR-TRK-011) — *S4 · Opus 4.8 · Effort hoch*
- [x] **M5.2** IMM-Grundgerüst: Bank + Markov-Mischung der Modellzustände (FR-TRK-012) — *S5 · Opus 4.8 · Effort max*
- [x] **M5.3** Modellbedingtes Filtern + Modellwahrscheinlichkeits-Update (Likelihood je Modell) + kombinierte Schätzung (FR-TRK-013) — *S5 · Opus 4.8 · Effort max*
- [x] **M5.4** IMM in den `Tracker` einhängen (ersetzt den einzelnen `LinearKalman` je Track, FR-TRK-014) — *S4 · Opus 4.8 · Effort hoch*
- [x] **M5.5** PDA-Assoziationswahrscheinlichkeiten `β` + Clutter-Modell (FR-TRK-015) — *S4 · Opus 4.8 · Effort hoch*
- [x] **M5.6** PDA-gewichtetes Kalman-Update (FR-TRK-016) — *S4 · Opus 4.8 · Effort hoch*
- [x] **M5.7** PDA-gewichtetes IMM-Update (FR-TRK-017) — *S5 · Opus 4.8 · Effort max*
- [x] **M5.8** JPDA-Kern: Clustering + Ereignis-Aufzählung + Marginalisierung (FR-TRK-018) — *S5 · Opus 4.8 · Effort max*
- [x] **M5.9** JPDA im `Tracker` einhängen, ersetzt GNN je Sensor (FR-TRK-019) — *S4 · Opus 4.8 · Effort hoch*

### M4-Plan in Häppchen (Option A, ADR 0010)

- [x] **4.1** SSR-Identität bis zum `SystemTrack` (FR-TRK-009) — *S3 · Sonnet*
- [x] **4.0** Architektur-Entscheidung: zentrale Mess-Fusion (ADR 0010) — *S4 · Opus 4.8*
- [x] **4.A.1** `firefly-geo`: Frame-zu-Frame-Transformation (Position + Kovarianz, FR-GEO-003) — *S4 · Opus 4.8 · Effort hoch*
- [x] **4.A.2** `firefly-track` auf Multi-Sensor: gemeinsamer Tracking-Frame, Plot-Umrechnung + sequenzielle Fusion, Pro-Sensor-Rauschmodell (FR-TRK-010) — *S4–S5 · Opus 4.8 · Effort hoch*
- [x] **4.A.3** Multi-Radar-Szenario (zwei überlappende Radare) + E2E-Test: ein Flugzeug → **ein** Track (FR-TRK-010) — *S4 · Opus 4.8 · Effort hoch*
- [x] **4.A.4** Sensor-Provenienz im `SystemTrack` (welche Sensoren tragen bei, FR-TRK-010) — *S3 · Sonnet · Effort mittel*
- [x] **4.2** CAT062-Identitätsfelder kodieren (I062/060, I062/380/ADR; `firefly-asterix`, FR-IO-003/FR-TRK-009) — *S3–S4 · Opus 4.8 · Effort mittel–hoch*
- [ ] *(später)* Sensor-Registrierung / Bias-Korrektur — *S5 · Fable 5 / Opus 4.8*

Offen/optional: Sichtprüfung des Frontends (inkl. „Verzug"-Knopf) im Browser
durch den Projektverantwortlichen.

Erst Erklärung → Rückfragen/Go → dann kleine, testbare Umsetzung.

### Häppchen C — ADR-0006-Transport (System-Stereografisch + UDP-Multicast)

- [x] **C.1** `firefly-geo`: konforme WGS84 ↔ System-Stereografisch-Projektion
  (`StereographicProjection`, FR-GEO-004) — *S4 · Opus 4.8 · Effort hoch*
- [x] **C.2** I062/100-Encoder (`firefly-asterix`, X/Y 24-Bit Zweierkomplement,
  LSB 0,5 m, zusätzlich zu I062/105) — *S2–S3 · Sonnet · Effort niedrig–mittel*
- [x] **C.3** UDP-Multicast-Versand-Adapter (`firefly-multicast`, FR-NET-002;
  `Player::scans()` als geteilte Basis; im Server-`main` verdrahtet) — *S4 · Opus 4.8 · Effort hoch*

### Häppchen D — CAT062-Multicast-Empfänger/Recorder

- [x] **D.1** CAT062-Decoder (`decode_data_block`, `fspec::parse`, FR-IO-004) —
  FSPEC + alle Items außer I062/100-Rückprojektion — *S3 · Sonnet · Effort mittel*
- [x] **D.2** I062/100 → WGS84 zurückprojizieren (`StereographicProjection::unproject`,
  FR-GEO-004), Vergleich mit I062/105 auf unter 1 m — *S2 · Sonnet · Effort niedrig–mittel*
- [x] **D.3** Echter Multicast-Empfänger (Socket tritt Gruppe bei, empfängt,
  dekodiert; Loopback-Integrationstest analog C.3) — *S3 · Sonnet · Effort mittel*

## 4. M3-Plan in Häppchen (mit Komplexität / Modell)

- [x] **3.0** Architektur-Entscheidung (ADR 0009: Tokio/axum, WebSocket, JSON, MapLibre) — *S2 · Sonnet · Effort niedrig*
- [x] **3.1** JSON-Ausgabe-Adapter (`Frame` = Zeit + Sensor + `SystemTrack[]`, `serde_json`; Crate `firefly-io`, FR-IO-001) — *S2–S3 · Sonnet · Effort niedrig–mittel*
- [x] **3.2** „Player": Szenario → Tracker → Frame-Strom (reine Logik, Tempo getrennt vom Kern; Crate `firefly-player`, FR-IO-002) — *S3 · Sonnet · Effort mittel*
- [x] **3.3** WebSocket-Server (axum/tokio, Health/Readiness, 12-Factor, Shutdown, Logs/NFR-OBS-001; Crate `firefly-server`, FR-NET-001) — *S4 · Opus 4.8 / Fable 5 · Effort hoch*
- [x] **3.4** Frontend 2D-Karte mit Live-Tracks (MapLibre; coasting/Status farbig, Unsicherheits-Ring, Geschwindigkeitsvektor; `static/index.html`, FR-UI-001) — *S3 · Sonnet · Effort mittel*
- [x] **3.5** Demo-Erlebnis (ein Befehl, „Verzug"-Auslöser zeigt Timing-Robustheit) — *S3 · Sonnet · Effort mittel*
- [x] **3.X** CAT062-Encoder-Adapter (3.X.1–3.X.4, `firefly-asterix`, FR-IO-003) — *S4 · Opus 4.8 / Fable 5 · Effort hoch*

## 4b. M2-Plan in Häppchen (abgeschlossen)

- [x] **2.1** Converted Measurement (Plot → kartesisch + Kovarianz) — *S3 · Sonnet*
- [x] **2.2** Kalman-Filter (Constant-Velocity, Predict/Update) — *S4 · Opus*
- [x] **2.3** Gating (Mahalanobis-/χ²-Validierungsregion) — *S3 · Sonnet*
- [x] **2.4** Datenassoziation GNN (Ungarische Methode) — *S4 · Opus*
- [x] **2.5** Track-Lebenszyklus (M-aus-N, Bestätigung, Coasting, Löschung) — *S4 · Opus*
- [x] **2.6** Serialisierbarer Zustand (Snapshot/Replay) — *S3 · Sonnet · Effort mittel*
- [x] **2.7** Neutraler `SystemTrack`-Output in WGS84 (ASD-Port → CAT062) — *S3 · Sonnet · Effort mittel*
- [x] **2.8** Güte-Metriken gegen Ground Truth (RMSE, Track-Kontinuität) — *S3 · Sonnet · Effort mittel*
- [x] **Nachtrag** Safety-Status auf `SystemTrack` (ADR 0008, FR-TRK-008) — *S3 · Sonnet · Effort mittel*
- [x] **Härtung** Timing-Robustheit (NFR-CLOUD-004) — *S3 · Sonnet · Effort mittel*

Jeder Haken wird erst gesetzt, wenn die Qualitäts-Gates (CLAUDE.md §5) erfüllt
sind und die Anforderung im Register rückverfolgbar steht.

### Komplexität künftiger Meilensteine (grobe Orientierung, inkl. Effort)

- **M1.5** ASTERIX CAT048-Codec — *S3 · Sonnet · Effort hoch* (viel Code, aber
  bit-genau und fehleranfällig).
- **M3** WebSocket-Server/Cloud-Anbindung — *S4 · Opus 4.8 / Fable 5 · Effort hoch*;
  Map-Frontend (JS) — *S3 · Sonnet · Effort mittel*; CAT062-Encoder + Transport-
  Adapter — *S4 · Opus 4.8 / Fable 5 · Effort hoch*.
- **M4** Multi-Radar-Fusion + SSR/ADS-B-Korrelation — *S5 · Fable 5 / Opus 4.8 · Effort hoch–max*.
- **M5** IMM / JPDA — *S5 · Fable 5 / Opus 4.8 · Effort max*.
- Reine Doku-/Nachbereitungs-Schritte — *S1–S2 · Haiku · Effort niedrig*.

## 5. Offene Punkte / später entscheiden

- **CAT062 I062/290 / I062/500 gegen Spezifikation geprüft (erledigt):** Anhand
  des vom Projektverantwortlichen bereitgestellten Auszugs aus
  SUR.ET1.ST05.2000-STD-09-01 (Ed. 1.10) verifiziert: Primary-Subfield-Bit für
  PSR-Alter ist Bit 15 (= `0x40`), LSB ¼ s, ein Oktett — passt. Primary-
  Subfield-Bit für APC ist Bit 16 (= `0x80`), Subfeld = X- und Y-Komponente je
  16 Bit, LSB ½ m, vorzeichenlos — passt. Unsere Encoder-Konstanten und der
  Referenz-Dump (`0x40, 0x08` bzw. `0x80, 0x00, 0xC8, 0x00, 0xC8`) stimmen
  exakt. Das Mapping „update_age → PSR-Alter" war eine Single-Sensor-
  Vereinfachung; mit 4.A.4 trägt der `SystemTrack` jetzt `contributing_sensors`
  (welche Sensoren im letzten Scan beigetragen haben). Die CAT062-Kodierung
  dieser Mehr-Sensor-Information ist Teil von 4.2.
- **ASD-Integration (ADR 0006) vollständig umgesetzt (Häppchen C.1–C.3 und
  D.1–D.3):** Transport = **UDP-Multicast** (`firefly-multicast`, FR-NET-002),
  Koordinatenbezug = **System-Stereografisch** (`StereographicProjection`
  FR-GEO-004 → CAT062 I062/100, FR-IO-003) **zusätzlich** zu I062/105. Der
  Tracker-Kern bleibt WGS84-neutral (`SystemTrack`); Projektion und Transport
  sind reine Adapter-Aufgaben. Die **Empfänger-/Recorder-Seite** (Häppchen D)
  ist mit Decoder (D.1), I062/100-Rückprojektion (D.2) und echtem
  Multicast-Empfänger (D.3) abgeschlossen — ADR 0006 ist Ende-zu-Ende bewiesen
  (Sender → Draht → Empfänger → Decoder). Weiterhin offen: konfigurierbarer
  System-Referenzpunkt jenseits des Demo-Ursprungs.
- **Message-Bus-Technologie** (z. B. NATS/Kafka) — erst relevant ab M3, dann ADR.
- **Coverage-Werkzeug** (z. B. `cargo llvm-cov`) — einführen, sobald V&V-Nachweise
  greifbar werden.
- **Sicherheitsanalyse (FHA/Hazards)** — sinnvoll, sobald Tracker-Funktionen
  stehen, gegen die man Gefährdungen bewerten kann.
- **Out-of-order-Daten (Eingangs-Adapter, M3/M4):** Wenn ein *sehr alter* Plot
  *nach* neueren ankommt, kann man nicht sinnvoll rückwärts vorhersagen. Standard:
  am Eingang nach Datenzeit ordnen, kleines Zeitfenster puffern, zu Spätes
  *verwerfen* (nur den Plot, nicht den Track). Bewusst **kein** „Daten alt → Reset".
- **Frische-/Staleness-Anzeige (Ausgabe-Rand, M3):** Aus `SystemTrack.update_age`
  am Anzeige-Rand eine *weiche* Frische-Markierung ableiten — nie zustands-
  zerstörend (ADR 0008). Die Entscheidung selbst liegt schon im Tracker.
- **Vorführbarkeit (NFR-OPS-001):** Ein-Befehl-Demo ohne Programmierkenntnisse
  für Präsentationen — Umsetzung mit dem Frontend in M3.
- **GNN-Assoziationskosten (erledigt durch JPDA):** Die alte Notiz betraf die
  harte 1:1-Zuordnung (Ungarische Methode, reine `d²`-Kosten) bei dichtem
  Verkehr/überlappenden Gates. Mit M5.5–M5.9 ist die harte Zuordnung durch
  **JPDA** ersetzt (FR-TRK-018/019), das genau dieses Problem über
  Assoziationswahrscheinlichkeiten `β` und Exklusivität löst — kein separater
  Kostenterm mehr nötig.
- **Manöver-Handling (erledigt durch IMM):** Ein einzelnes `Q` deckt nur einen
  Manöver-Bereich ab. M5.1–M5.4 lösen das mit **IMM** (mehrere Bewegungsmodelle
  parallel, FR-TRK-011–014).
- **Höhen-Projektionsfehler bei `horizontal_from` (M6.1-Befund — BEHOBEN):**
  Trat ein hoch fliegendes Ziel *mitten im Flug* in die Reichweite eines
  zweiten Radars ein, während sein Track vom ersten Radar bereits eng
  eingerastet war, lag die erste Messung des zweiten Radars knapp außerhalb des
  engen Tores — `horizontal_from` projizierte die Bodenmessung im *Quellrahmen*
  (`up=0`) entlang der je Standort verschiedenen lokalen „Oben"-Richtung
  (wenige zehn bis ~100 m Versatz bei 10 km Höhe) → „Geister"-Spur. **Behoben:**
  `horizontal_from(source, z, height, r)` hebt jetzt den vollständigen
  3D-Punkt in den Tracking-Frame und projiziert erst dort auf den Boden →
  sensor-unabhängig (FR-GEO-003, Regressionstest
  `airborne_target_maps_to_one_point_from_two_sensors`). Frankfurt läuft damit
  wieder mit realistischen, überlappenden Reichweiten (100 km) und einem echten
  Nord-Handover auf 8 km Höhe — stabil bei acht Tracks.
- **Multi-Radar-Geister-Tracks bei Gate 0,99 (M6.1-Befund — BEHOBEN, ADR 0011):**
  Bei Gate `0,99` entstanden zwei „Geister"-Spuren. Diagnose: **kein**
  IMM-Manöver, sondern Fusions-Artefakte — (a) sequenzielle Tor-Verengung
  (Sensor A aktualisiert → Tor enger → Sensor Bs Plot fällt heraus → Duplikat)
  und (b) ein einzelner 3σ-Ausreißer-Plot, der sofort einen Track gebärt.
  **Behoben:** eine zu Scan-Beginn eingefrorene Fusions-Referenz (alle Sensoren
  gaten gegen die Prädiktion, keine Zwischen-Verengung) + ein getrenntes,
  weiteres Initiierungs-Sperr-Tor (`init_gate`, Default `0,9999`). FR-TRK-020,
  Test `tracker::outlier_plot_does_not_spawn_a_ghost`. Frankfurt läuft damit
  wieder mit dem Standard-Tor `0,99` und acht Tracks.
- **Instabilität bei asynchronen Radar-Scans (`scan_offset`, M6.1-Befund —
  BEHOBEN, ADR 0012):** In der dichten 8-Flugzeug/3-Radar-Szene führte ein
  Scan-Versatz zu massiver Track-ID-Instabilität (50–90 statt 8 IDs).
  **Ursache:** Der Lebenszyklus buchte Treffer/Fehltreffer **pro
  `process_scan`-Aufruf**; `Player::scans` gruppiert Plots nach exakt gleicher
  Zeit, also trug mit Versatz jeder Aufruf nur **einen** Sensor bei — ein
  Flugzeug, das gerade nur Radar B sieht, kassierte beim Offset-Scan von Radar A
  einen falschen „Miss" → Löschung → Respawn. **Behoben:** der **adaptive
  Lebenszyklus** (FR-TRK-021) zählt Bestätigung/Löschung in
  `coast_reference = max(revisit_interval, cadence)` Sekunden statt in
  Scan-Aufrufen — `revisit_interval` (EWMA der Treffer-Zeitlücken je Track) und
  `cadence` (geschätzte Feed-Taktung) wachsen auf die wahre Sensor-Periode, ein
  einzelner kurzer Versatz zwischen zwei Radaren zählt nicht als verpasste
  Wiederkehr. Frankfurt läuft jetzt dauerhaft mit `scan_offset = 0/1.3/2.6 s`
  und genau acht Track-IDs (`scene::frankfurt_scene_keeps_one_identity_per_aircraft`).

## 6. So steige ich wieder ein (Kurzbefehle)

```bash
cargo test --workspace                     # alles grün?
cargo run --example demo -p firefly-sim    # M1-Simulator live sehen
```

Doku-Einstieg: `docs/README.md` → Glossar, Meilensteine, ADRs, Requirements.
