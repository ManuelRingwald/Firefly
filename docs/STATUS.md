# Arbeitsstand (Handover-Notiz) — Firefly

> **Zweck:** Diese Datei beschreibt den **aktuellen IST-Stand** von Firefly.
> Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

> 🗺️ **Roadmap & Arbeitspakete:** siehe `docs/ROADMAP.md` im **Wayfinder-Repo**
> (zentrale Quelle für beide Repos). Cross-Project-Abhängigkeiten in
> `docs/cross-project/todo-for-firefly.md`.

---

## 🎯 Stand 2026-07-10 (QW.2 Fuzzing — echter FSPEC-Bug gefunden & gefixt)

- **Zuletzt aktualisiert:** 2026-07-10 (Abend)
- **QW.2 — Coverage-geführtes Fuzzing der Vertrauensgrenzen (NFR-SAFE-002):**
  Neues `fuzz/`-Workspace (cargo-fuzz/libFuzzer, bewusst außerhalb des
  stabilen Workspace) mit fünf Targets: CAT048/062/063/065-Decoder +
  `FIREFLY_SOURCES`-Parser; Seed-Korpus aus den Referenz-Dumps; zeitgeboxter
  CI-Job „Fuzz" (60 s je Target, Crash-Artefakt-Upload). Bedienung:
  `fuzz/README.md`.
- **Erster Ertrag — echter Bug in Sekunden gefunden:** u8-Überlauf in der
  gemeinsamen FSPEC-FRN-Arithmetik (`fspec::parse`) — eine feindliche
  FX-Kette > 36 Oktette panickte (Debug) bzw. las stillschweigend falsche
  FRNs (Release), in **allen vier** ASTERIX-Decodern. Fix: Kette hart auf
  `MAX_FSPEC_OCTETS` = 36 begrenzt (FRN ≤ 252, jenseits jeder realen UAP),
  Überlänge ⇒ neue Fehler-Variante `FspecTooLong` je Decoder. 6 eingefrorene
  Regressionstests; Original-Crash-Eingaben verifiziert sauber; frischer
  Fuzz-Lauf ohne Funde; `sources_parse` > 5 Mio. Läufe ohne Befund. **Kein
  Wire-Bruch** (nur ohnehin undekodierbare Eingaben werden abgelehnt), ICD
  unverändert. **Wayfinder-Folge:** gleiche FSPEC-Härtung + Fuzzing für den
  Go-Decoder empfohlen (`from-firefly`-Issue). Roadmap-Stand: **31,5 %**.
- **Nächster Schritt:** QW.3 (I062/295 + I062/080-Bit-Ausbau, S2) ankündigen.

## 🎯 Stand 2026-07-10 (ARTAS-Gap-Roadmap + QW.1 Track-Nummern-Pool)

- **Zuletzt aktualisiert:** 2026-07-10
- **ARTAS-Gap-Analyse & Roadmap (`docs/design/artas-gap-roadmap.md`):** Firefly
  wurde vollständig (Code + Doku) gegen EUROCONTROL **ARTAS** als vollwertiges
  SDPS inventarisiert. Ergebnis: **≈ 30 % Fähigkeits-Abdeckung** (gewichtetes
  Modell im Dokument); die fünf größten Abstände sind Sensoreingang
  (CAT034/021/020, Mode-S-DAPs), **Sensor-Registrierung/Bias** (kritischster
  Punkt vor echten Radaren), 2-D-Tracker (Höhe/RoCD/QNH/MoM fehlen),
  Flugplan-Korrelation (I062/390 — bisher nirgends im Backlog!) und HA/
  Kapazitätsnachweis. Roadmap mit 10 Arbeitspaketen (AP-QW … AP-ASSUR) und
  kumulierten Prozent je Häppchen bis 100 %.
- **QW.1 — Track-Nummern-Pool für I062/040 (FR-TRK-035, ICD 3.1.1):** Erster
  Roadmap-Punkt umgesetzt. Die Draht-Track-Nummer war eine stille
  `u32→u16`-Trunkierung der internen `TrackId` (`cat062.rs`) — nach 65 536
  Track-Geburten drohten Draht-Kollisionen (zwei Flieger unter einer Nummer,
  TSE löscht beim Konsumenten den falschen Track). Jetzt: verwalteter Pool
  (`firefly-track::track_number::TrackNumberPool`) — frische Nummern ab 1
  (`0` nie), bei Löschung **60 s Datenzeit-Quarantäne** vor FIFO-
  Wiederverwendung, bei Erschöpfung (> 65 535 gleichzeitige Tracks) wird die
  Initiierung mit Warn-Log abgelehnt (ehrliche Grenze, TECHNICAL §11).
  `Track.number`/`SystemTrack.track_number` additiv; Encoder nutzt nie mehr
  die ID. Pool ist Teil des serialisierbaren Tracker-Zustands (ADR 0007,
  HA-Vorbau). **Kein Wire-Bruch** (u16 BE unverändert, ICD 3.1.1 rein
  dokumentarisch, Abschnitt 4.6 mit Konsumenten-Garantie); Wayfinder muss
  nichts nachziehen. 7 neue Tests (Pool, Tracker-Lebenszyklus, Encoder-
  Regression); Milestone `Track-Number-Pool_I062-040.md`. Gates grün
  (`cargo test --workspace`, clippy, fmt).
- **Nächster Schritt:** Roadmap-Reihenfolge — **QW.2** (echtes Fuzzing für
  CAT048/`FIREFLY_SOURCES`, S2–S3) ankündigen, nach „Go" umsetzen.

## 🎯 Stand 2026-07-06 (Nachmittag)

- **Zuletzt aktualisiert:** 2026-07-06
- **ADR 0033 — CAT063 per-Quelle-Fehlergrund (`SRC-REASON` im I063/RE, ICD 3.1.0,
  additiv):** Aufbauend auf ADR 0032 trägt ein **degradierter** Sensor mit
  bekanntem Grund den Ausfallgrund im **Reserved Expansion Field** (FRN 13, FSPEC
  dann `0xB9 0x04`): Vendor-Subfeld `SRC-REASON` (`1=unreachable`/`2=auth`/
  `3=rate_limited`), Layout `[LEN=0x03][0x80][code]`. **Nur** bei Degradierung
  mit Grund gesendet — operationelle Records bleiben 9 Oktette (additiv, kein
  Wire-Bruch; RE ist selbst-begrenzend). `SensorReason`/`SensorReport` in
  `firefly-asterix`; `SensorHealthMonitor::record_failure`/`record_activity`
  führen bzw. löschen den Grund pro Sensor; Klassifikation über die neuen
  `PollError::is_auth()` (OpenSky/adsbagg, HTTP 401/403) + bestehendes
  `is_rate_limited()`; sonst `unreachable`. FLARM/Radar liefern keinen Grund
  (ehrliche Grenze). Antwort auf Wayfinder #197 (Firefly #55, H3). Byte-genaue
  Referenz-Vektoren + Monitor-Tests; ICD Abschnitt 9 + Changelog 3.1.0; ADR 0033;
  FR-IO-007 erweitert. **Wayfinder-Folge H4:** RE-Reason dekodieren + Feed-Health-
  Chip → **Fixes #197** (rein additiv, kein Lockstep-Zwang).

## 🎯 Stand 2026-07-06

- **Zuletzt aktualisiert:** 2026-07-06
- **ADR 0032 — CAT063-UAP-Standardisierung (ICD 3.0.0, BREAKING):** Die
  CAT063-Sensor-Status-Records folgen jetzt den **echten EUROCONTROL-FRN-Slots**
  (spiegelt die CAT062-Korrektur aus ADR 0015). (1) I063/010 trägt die
  **SDPS**-Identität (SAC/SIC = `FIREFLY_CAT062_SAC`/`_SIC`, Default 25/2), nicht
  mehr den Sensor. (2) Neues I063/050 (FRN 4) trägt die **Sensor**-Identität
  (SAC 0, SIC = `sensor_id`). (3) I063/030 → FRN 3, I063/060 → FRN 5. FSPEC
  `0xE0` → **`0xB8`**, Record 7 → 9 Oktette; CON-Werte auf Standard korrigiert
  (`0` op / `1` degradiert / `2` init / `3` not-connected). Anlass: sauberes
  Fundament für den Grund-Code je ausgefallener Quelle (#197 → ADR 0033, RE-Feld,
  additiv). `Cat063Encoder::new(data_source, sensor_sac)`; `DecodedSensorStatus`
  trennt `data_source` (SDPS) und `sensor` (I063/050). **Wayfinder zieht in
  lockstep nach (H2)** — Firefly-first mergen+deployen, Wayfinder unmittelbar
  danach; Cross-Project via Firefly #55 (`from-wayfinder`). Byte-Referenz-Dumps
  + ICD-Abschnitt 9 auf 3.0.0-Form; FR-IO-007 erweitert.

## 🎯 Stand 2026-07-05

- **Zuletzt aktualisiert:** 2026-07-05
- **ADR 0031 — Community-Aggregator-ADS-B-Adapter (`adsb_aggregator`, #53):**
  Vierter Live-Quell-Adapter, Crate `firefly-adsbagg` — auth-freier ADS-B-Bezug
  über adsb.lol (Default) / adsb.fi (ADSBEx-v2-kompatibles API). Anlass: OpenSky
  verwirft Datacenter-IPs (Codespaces-Diagnose 2026-07-05); OpenSky bleibt
  vollwertig daneben (Anbieterwahl pro Quelle, kein Ersatz). BBox→Umkreis-Query
  (max 250 NM, Clamp mit WARN) + Rückfilter auf die BBox; `"ground"`/Staleness/
  `~`-Hex-Robustheit; 429-Backoff (Muster #49); Sensor-Default 230; Metriken
  `firefly_adsbagg_*`/`firefly_sources_adsbagg`. Kontrakt v1.5.0 (additiv,
  neues Feld `provider`; `cred_env` ignoriert). airplanes.live zurückgestellt
  (Radius-Einheit unverifiziert, ADR 0031). **Wayfinder zieht nach (#201):**
  Store-Vokabular + Orchestrator-Pass-through (`provider`) + UI-Typ
  „ADS-B (Community-Aggregator)" ohne Credential-Block.

## 🎯 Stand 2026-07-04

- **Zuletzt aktualisiert:** 2026-07-04
- **ADR 0030 — Replay-/Szenen-Modus ausgebaut:** Der Server läuft nur noch als
  quellen-getriebener Live-Tracker (`FIREFLY_SOURCES`/Opt-in-Adapter-Envs);
  `FIREFLY_MODE`/`FIREFLY_SCENE`/`FIREFLY_SPEED` werden ignoriert (Warn-Log).
  Ohne Quellen: leerer Himmel + CAT065-Heartbeat, `/ready` sofort bereit.
  OpenSky im Standalone-Fallback jetzt Opt-in (`FIREFLY_OPENSKY_ENABLED`) —
  kein Überraschungs-Egress beim nackten Start. Frankfurt-Regressionstests als
  Fixture nach `firefly-player/tests/frankfurt_regression.rs` umgezogen
  (Nachweise FR-TRK-018…023 lückenlos); `.ffplots`-Replay-Engine und
  `firefly_multicast::run` (Wire-Level-Tests) bewusst unangetastet. ICD 2.6.1
  (rein dokumentarisch, kein Wire-Bruch). **Wayfinder zieht nach** (eigener
  PR: `WAYFINDER_FIREFLY_SCENE`-Platzhalter + `docker-compose.bridge.yml`
  entfallen; Feed ohne Quellen → leerer Himmel statt Fake-Szene).

## 🎯 Stand 2026-07-03

- **Zuletzt aktualisiert:** 2026-07-03
- **Ist-/Gap-Analyse Service-Orientierung & HA (repo-übergreifend, Doku im
  Wayfinder-Repo):** `docs/design/gap-analyse-service-orientierung-ha.md`
  (Wayfinder) analysiert beide Systeme: System-Ebene bereits service-orientiert
  (CAT062-Vertrag, 1 Instanz pro Feed), Binnen-Ebene modulare Monolithen.
  **Firefly-relevante Befunde:** (a) 1 Instanz pro Feed = Single Point of
  Failure → **SDPS-002** (HA/State-Sync) bleibt die wichtigste betriebliche
  Lücke; (b) der `PlotRecorder` (ADR 0020, `.ffplots`-Replay als
  Wiederherstellungs-Weg) ist im Live-Pfad **nicht verdrahtet**
  (`crates/firefly-server/src/main.rs:329`, `LiveTracker::new(tracker, None)`)
  — als SDPS-002-Vorstufe einplanen (S3–S4); (c) Tracker-Strukturen sind
  serialisierbar, aber kein Snapshot/Restore-Codepfad existiert; (d) keine
  K8s-Manifeste (Probes/SIGTERM/12-Factor sind fertig vorbereitet). Empfohlene
  Reihenfolge und Backlog-Anker (WF2-52/53, ORCH-6, SDPS-002) im Dokument.
  Reine Doku, kein Code.

## 🎯 Stand 2026-07-02

- **Zuletzt aktualisiert:** 2026-07-02
- **OpenSky 429-Backoff (Issue #49, Branch `claude/wayfinder-tenant-radius-bug-w99r8q`):**
  Folge-Härtung zu ADR 0029 aus dem Wayfinder-E2E — ein rate-limitierter Feed wurde
  im festen Takt weitergepollt und provozierte weitere 429. Jetzt: `HTTP 429` als
  eigener `PollError::RateLimited` (erkannt vor `error_for_status`, `is_rate_limited()`,
  testbar); `OpenSkyPoller::run` nutzt eine kleine, reine `Backoff`-Zustandsmaschine
  (base=`poll_interval_secs`; bei Fehler ×2 wachsend, Cap 300 s bzw. ≥ base; Reset
  bei Erfolg); 429 bekommt eigenen Warn-Log + Metrik `firefly_opensky_rate_limited_total`
  (Teilmenge der Poll-Fehler, in der `on_error`-Closure gebumpt). **Rein
  Firefly-intern** — kein Wire-/Kontrakt-Change, kein ADR nötig. FR-NET-004 +
  FR-OBS-003 + TECHNICAL.md aktualisiert. Gates: `cargo test -p firefly-opensky`
  (22, +7) + `-p firefly-server metrics`, `clippy`/`fmt` grün.
- **Konfigurierbares OpenSky-Poll-Intervall (ADR 0029, Kontrakt v1.4.0, Branch
  `claude/wayfinder-tenant-radius-bug-w99r8q`):** Antwort auf Wayfinder-Wunsch #3
  (Poll-Schutz) — der E2E-Lauf lief anonym in **HTTP 429**, weil das Poll-Intervall
  fix bei 10 s lag und über `FIREFLY_SOURCES` nicht steuerbar war. Jetzt trägt
  `adsb_opensky` ein optionales **`poll_interval_secs`** (ganze Sekunden):
  `SourceSpec.poll_interval_secs: Option<u64>` (`#[serde(default)]`, additiv),
  `opensky_config_from_spec` übernimmt nur `> 0` (sonst Default 10 s — kein
  Heiß-Lauf, spiegelt `OpenSkyConfig::from_env`); die Ausgabe-Kadenz zieht via
  `representative_config` automatisch nach. Nur für `adsb_opensky` (FLARM ist Push,
  Radar hat eigene Scan-Periode). Kontrakt-Doku v1.4.0 + Changelog, ADR 0029,
  FR-NET-011 + Cross-Project-Todo aktualisiert. **Additiv & bidirektional
  kompatibel** (kein `deny_unknown_fields`) → Merge-Reihenfolge zu Wayfinder
  entkoppelt. Gates: `cargo test -p firefly-server` (26 sources-Tests, +3),
  `clippy`/`fmt` grün.
- **Hotfix (2026-07-02) — FLARM-Epoch-Zeitstempel (Wayfinder #120):** Ein
  **kombinierter ADS-B+FLARM-Live-Feed** lieferte keine Tracks, obwohl beide
  Quellen einzeln laufen. Root Cause: OpenSky stempelt Plot-Zeit als
  **Unix-Epoch** (`resp.time`), FLARM stempelte **Sekunden-seit-Mitternacht** —
  der gemeinsame monotone Datenzeit-Wasserstand des Multi-Source-Trackers verwarf
  daraufhin alle FLARM-Plots als „out-of-order". Fix in `firefly-flarm`
  (`position_to_plot`/`aprsis`): FLARM stempelt jetzt **Epoch-UTC** (OGN-Tageszeit
  an den Empfangstag verankert, Tageswechsel-Korrektur, Fallback Empfangszeit).
  Kein CAT062-Wire-Change. Doku: `docs/milestones/FLARM-Epoch-Time_Multi-Source-Fusion.md`,
  FR-NET-012. Alle Gates grün (`cargo test --workspace`, clippy, fmt).

## 🎯 Stand 2026-06-30

- **Zuletzt aktualisiert:** 2026-06-30
- **Großes Bild:** Die **Firefly-Seite des Quell-Eingangs-Kontrakts (#35)** ist
  **vollständig** — **alle drei** Vokabular-Typen haben Adapter: `adsb_opensky`
  (ADR 0019/0024), `flarm_aprs` (ADR 0026) und jetzt **`radar_asterix`** (ADR 0028,
  CAT048/UDP). Zusätzlich ist die **Per-Track-Provenienz** (#30, ADR 0027, CAT062
  I062/290 per-Technologie-Alter, ICD **v2.6.0**) geliefert und der erste
  **Betriebs-Härtung**-Block (Live-Pipeline-Observability). **#35 und #30 sind
  geschlossen.** Alles auf `main`, alle Gates grün (44 Test-Suites, clippy sauber).

- **Letzte Arbeit (2026-06-30, Vier-Themen-Batch):**
  1. **ADR 0027 — Per-Track-Provenienz** (#30, PR #43): `SourceKind` am Plot,
     `SystemTrack.source_ages` + abgeleitete `Provenance`; CAT062 I062/290 additiv
     um SSR/Mode-S/FLARM-Alter (ICD v2.6.0); JSON-Pfad führt `provenance`+`source_ages`.
     Bugfix: Treffer-Buchung fehlte an JPDA-Best/Track-Geburt. FR-TRK-034.
     Wayfinder-Folge #90.
  2. **ADR 0028 — `radar_asterix`-Adapter** (#35, PR #44): CAT048-Decoder
     (`firefly-asterix::cat048`, robust/fuzz-getestet, FR-IO-005) + Crate
     `firefly-radar` (FR-NET-013) + Verdrahtung (Radar-Sensor mit eigenem
     Standort-Frame). Kontrakt **v1.3.0** (`lat`/`lon` Pflicht). Wayfinder-Folge #91.
  3. **Wayfinder #57** (Wayfinder PR #92): View-Config-Formular-Captions
     (Zentrum/Zoom, AOI als harte Grenze, FL-Einheit + fail-open), FR-UI-013.
  4. **Betriebs-Härtung — Live-Pipeline-Observability** (NFR-OBS-003): Counter
     `firefly_live_plot_batches_dropped_total` (Back-Pressure-Verlust) + Gauges
     `firefly_sources_{opensky,flarm,radar}` (konfigurierter Quell-Mix).

- **Nächste Schritte:**
  1. **Zero-Touch-/Komplett-Setup-Abnahme** durch den Betreiber (steht an).
  2. **Wayfinder-Folge-Issues** #90 (I062/290-Decoder/Provenienz) und #91
     (Docker-Backend serialisiert `radar_asterix` lat/lon/listen) drüben umsetzen.
  3. **Betriebs-Härtung** weiter ausbauen (Lastfestigkeit/Deployment) nach Bedarf.

> 🗺️ Roadmap zentral im **Wayfinder-Repo** (`docs/ROADMAP.md`). Cross-Project:
> `docs/cross-project/todo-for-wayfinder.md`; offene `from-firefly`-Issues bei
> Wayfinder: #90 (Provenienz-Decoder), #91 (Radar-Quell-Serialisierung).

---

## ✅ Abgeschlossene Meilensteine

| Meilenstein | Inhalt | Status |
|---|---|---|
| **M1** | Simulator (ASTERIX-Szenarien, Track-Injection) | ✅ |
| **M2** | Single-Radar-Tracker (Kalman, Gate, JPDA, Lebenszyklus) | ✅ |
| **M3** | WebSocket-Server + JSON-Ausgabe (Live-Karte) | ✅ |
| **M4** | Multi-Radar-Fusion (Mess-Fusion, Sensormodell) | ✅ |
| **M5** | IMM/JPDA (Bewegungsmodelle, Assoziationen) | ✅ |
| **M6** | Showcase + Container (Deployment-ready) | ✅ |

---

## 📦 Produktions-Phase (laufend, ADR 0014)

### ✅ Fertig

| Feature | Status | Verweis |
|---|---|---|
| **UTC Time-of-Day** | ✅ I062/070 echte UTC-Tageszeit | Issue #9, geschlossen |
| **Multicast-Feed-Sicherheit** | ✅ ADR 0017 + WebSocket-Auth `/ws` | PR #27 |
| **System-Referenzpunkt** | ✅ I062/100 konfigurierbar via `FIREFLY_SYSTEM_REF_*` | ADR 0021 |
| **CAT062-ICD versioniert** | ✅ `docs/ICD-CAT062.md` v2.5.0 | Schnittstellen-Vertrag |
| **ADR 0013** | ✅ Asynchrone Pro-Plot + periodischer Ausgabetakt | 13.1–13.7 erledigt |
| **ADR 0015** | ✅ CAT062 Vertikallage I062/136 + UAP-Standard (FRN 27) | ICD 2.0.0 |
| **AP7/AP8** | ✅ CAT062 Callsign I062/245 | ICD 2.1.0, PR #15 |
| **ADR 0016** | ✅ CAT062 Track-Ende (I062/080 TSE) | ICD 2.2.0, PR #16 |
| **ADR 0018** | ✅ CAT065 SDPS-Heartbeat | ICD 2.3.0 |
| **ADR 0022** | ✅ CAT063 Sensor-Status (Per-Sensor-Liveness) | ICD 2.5.0, #32 |

### 🚧 Offen

Siehe zentrale **Wayfinder `ROADMAP.md`** für aktuelle Priorisierung (Prio 1 / Prio 2).

---

## 📋 Cross-Project-Abhängigkeiten (zu Wayfinder)

Siehe `docs/cross-project/todo-for-firefly.md`:

- **ORCH-5 (Live-Quell-Ingestion)** — generische Input-Adapter, Firefly-Arbeit
- **Per-Track-Sensor-Provenienz** — erfordert CAT062-ICD-Änderung
- **SWIM-Integration** — Abhängigkeit von Wayfinder EFS/IMS (Prio 2)
- **Ende-zu-Ende-HA** — Wayfinder WF2-52/53 ↔ Firefly SDPS-002

---

## 🔧 Technologie-Stack (ratifiziert)

- **Sprache:** Rust (ADR 0001)
- **Tracking:** Kalman-Filter + IMM/JPDA
- **Ausgabe:** CAT062 über UDP-Multicast (ADR 0006)
- **Deployment:** Docker + Kubernetes-ready (ADR 0003)

---

## 📚 Wichtige Dateien

- `docs/ICD-CAT062.md` — Schnittstellen-Vertrag mit Wayfinder (maßgeblich, versioniert)
- `docs/decisions/` — ADRs (0001–0022)
- `CLAUDE.md` — Arbeitsregeln
