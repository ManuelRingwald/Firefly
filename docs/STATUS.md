# Arbeitsstand (Handover-Notiz)

> **Zweck:** Diese Datei ist der schnelle Wiedereinstieg — egal ob am PC oder
> Handy. Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

- **Zuletzt aktualisiert:** 2026-06-10
- **Branch:** `claude/next-steps-ft3t3n`
- **Letzter Commit:** Häppchen **4.A.1** — Geo-Baustein in `firefly-geo`:
  `LocalFrame::horizontal_from` transformiert eine horizontale Messung
  (Position + 2×2-Kovarianz) von einem Frame in einen anderen (Position via
  geodätische Verkettung, Kovarianz via Frame-Rotation `R'=T·R·Tᵀ`); dazu
  `horizontal_rotation_from` und private Richtungs-Rotationen. `nalgebra` zu
  `firefly-geo` hinzugefügt (ADR 0005). 5 neue Tests, FR-GEO-003. Über ~67 km
  Basislinie: Position auf ~1 m, Kovarianz-Invarianten auf ~1e-4 genau.
- **PR:** keiner offen.

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
- **M4 läuft:** Häppchen **4.1 + 4.0 erledigt.**
  **4.1**: `Track` (firefly-track) merkt sich die SSR-Identität (Mode-3/A,
  ICAO-Adresse) aus zugeordneten Plots (`Track::update_identity`, sticky), und
  `SystemTrack` (firefly-core) führt sie als `mode_3a: Option<u16>` /
  `icao_address: Option<u32>` mit (FR-TRK-009).
  **4.0**: Architektur-Entscheidung **ADR 0010** — zentrale **Mess-Fusion**
  (Option A): ein Tracker, gemeinsamer Tracking-Frame, Plot-Umrechnung in
  diesen Frame (Position + Kovarianz), Pro-Sensor-Rauschmodell. Begründung:
  Präzision (Rohmessungen) bei gleicher Cloud-Tauglichkeit; Synergie mit dem
  System-Referenzpunkt der CAT062-Ausgabe (ADR 0006). Noch offen:
  Umsetzung in 4.A.1–4.A.4 und CAT062-Kodierung der Identität (4.2).
- Qualität: **102 Tests grün**, Clippy sauber, `cargo fmt` ok. Sichtprüfung des
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

✅ **M4 Häppchen 4.1 + 4.0 erledigt:** SSR-Identität durchgereicht
(FR-TRK-009); Architektur entschieden — **zentrale Mess-Fusion** (ADR 0010).

➡️ **Als Nächstes:** **4.A.2** — `firefly-track` auf Multi-Sensor umstellen:
`TrackerConfig` mit gemeinsamem Tracking-Frame + Rauschmodell **je** `SensorId`;
`process_scan` rechnet jeden Plot via `convert_plot` (Sensor-Frame) und dann
`LocalFrame::horizontal_from` (4.A.1) in den gemeinsamen Frame, *bevor* gegated/
assoziiert wird. `system_tracks` nutzt dann den gemeinsamen Frame direkt.
*S4–S5 · Opus 4.8 / Fable 5 · Effort hoch.*

### M4-Plan in Häppchen (Option A, ADR 0010)

- [x] **4.1** SSR-Identität bis zum `SystemTrack` (FR-TRK-009) — *S3 · Sonnet*
- [x] **4.0** Architektur-Entscheidung: zentrale Mess-Fusion (ADR 0010) — *S4 · Opus 4.8*
- [x] **4.A.1** `firefly-geo`: Frame-zu-Frame-Transformation (Position + Kovarianz, FR-GEO-003) — *S4 · Opus 4.8 · Effort hoch*
- [ ] **4.A.2** `firefly-track` auf Multi-Sensor: gemeinsamer Tracking-Frame, Plot-Umrechnung vor Assoziation, Pro-Sensor-Rauschmodell — *S4–S5 · Opus 4.8 / Fable 5 · Effort hoch*
- [ ] **4.A.3** Multi-Radar-Szenario (zwei überlappende Radare) + E2E-Test: ein Flugzeug → **ein** Track — *S4 · Opus 4.8 · Effort hoch*
- [ ] **4.A.4** Sensor-Provenienz im `SystemTrack` (welche Sensoren tragen bei) — *S3 · Sonnet · Effort mittel*
- [ ] **4.2** CAT062-Identitätsfelder kodieren (`firefly-asterix`, unabhängig) — *S3–S4 · Opus 4.8 · Effort mittel–hoch*
- [ ] *(später)* Sensor-Registrierung / Bias-Korrektur — *S5 · Fable 5 / Opus 4.8*

Offen/optional: Sichtprüfung des Frontends (inkl. „Verzug"-Knopf) im Browser
durch den Projektverantwortlichen.

Erst Erklärung → Rückfragen/Go → dann kleine, testbare Umsetzung.

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
  exakt. Offen bleibt weiterhin: Das Mapping „update_age → PSR-Alter" ist eine
  Single-Sensor-Vereinfachung (Mehr-Sensor-Provenienz erst in M4).
- **ASD-Integration (ADR 0006), Transport & Koordinatenbezug entschieden:**
  Transport = **UDP-Multicast**, Koordinatenbezug = **System-Stereografisch**
  (CAT062 I062/100 statt I062/105). Noch **nicht umgesetzt** — offene
  Folge-Häppchen: (1) Projektion WGS84 → System-Stereografisch +
  I062/100-Encoder in `firefly-asterix`, (2) UDP-Multicast-Versand-Adapter.
  Voraussichtlich im Umfeld von M4 (hängt mit Multi-Sensor-Provenienz
  zusammen). Design-Hinweis bleibt: Der `Tracker` führt die geodätische
  Frame-Referenz des Sensors mit, damit Tracks neutral als **WGS84**
  (`SystemTrack`) ausgegeben werden — die Stereo-Projektion ist reine
  Adapter-Aufgabe.
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
- **GNN-Assoziationskosten (latent, später):** Die Ungarische Methode nutzt heute
  reine `d²`-Kosten. Statistisch korrekt wäre die negative Log-Likelihood
  `d² + ln(det S)`, die *unsichere* Tracks bestraft. Beim Identitäts-Bugfix
  geprüft: hier **nicht** ursächlich (jede Dublette entstand aus einem Plot
  *außerhalb* des Gates, nicht aus Fehlzuordnung). Lohnt erst bei dichtem Verkehr
  / überlappenden Gates (M5/JPDA). Kein eigener Commit jetzt.
- **Manöver-Handling (M5):** Ein einzelnes `Q` deckt nur einen Manöver-Bereich ab.
  Für starke Manöver ist **IMM** (mehrere Modelle parallel) die saubere Antwort —
  geplant für M5. Bis dahin: `Q` je Szenario passend wählen.

## 6. So steige ich wieder ein (Kurzbefehle)

```bash
cargo test --workspace                     # alles grün?
cargo run --example demo -p firefly-sim    # M1-Simulator live sehen
```

Doku-Einstieg: `docs/README.md` → Glossar, Meilensteine, ADRs, Requirements.
