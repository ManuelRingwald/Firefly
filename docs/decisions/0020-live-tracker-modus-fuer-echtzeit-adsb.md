# ADR 0020 — Live-Tracker-Modus und Plot-Aufzeichnung

- **Status:** akzeptiert (2026-06-18; Umsetzung in AP9.4c-0…6 läuft)
- **Datum:** 2026-06-18
- **Schnittstellen-relevant:** nein (CAT062-Draht-Vertrag bleibt unverändert;
  ICD 2.4.0 deckt das ES-Age-Subfeld bereits ab). Diese Entscheidung betrifft
  ausschließlich die **interne Laufzeit-Architektur** des `firefly-server`
  sowie eine neue Aufzeichnungsschicht im `firefly-recorder`-Crate.
- **Baut auf:** ADR 0003 (Cloud-native / Determinismus), ADR 0013 (asynchrone
  Pro-Plot-Verarbeitung + periodischer Ausgabetakt), ADR 0019 (ADS-B via
  OpenSky), SDPS-005 (`firefly-recorder` — Ausgangs-Aufzeichnung).

## Kontext

ADR 0019 hat die ADS-B-Integration entschieden, und AP9.1–AP9.7 haben sie bis
zum Encoder gebaut: Der Tracker kann ADS-B-Plots fusionieren (`Measurement::
Geodetic`, ICAO-Vorsortierung), der `firefly-opensky`-Crate pollt die OpenSky-
REST-API und erzeugt `Plot::adsb`-Objekte, und der Poller läuft als
Hintergrund-Task im Server (AP9.4b).

**Die Lücke (AP9.4c):** Der Poller wirft seine Plots derzeit nur ins Log
(`main.rs`, „live-tracker injection pending") — es gibt zur Laufzeit **keinen
lebenden Tracker**, in den sie fließen könnten. Der Grund ist architektonisch:

Der `firefly-server` ist heute ein **deterministischer Vorausberechnungs-
Replay**:

- Beim Start baut `demo_player()` / `frankfurt_player()` aus einem **simulierten**
  `Scenario` einmalig einen `Player`.
- `Player::periodic_frames()` / `periodic_snapshots()` berechnen das **komplette**
  Luftlagebild vorab als `Vec<Frame>` bzw. `Vec<(Timestamp, Vec<SystemTrack>)>`.
- `AppState.frames` ist ein **unveränderliches** `Arc<Vec<Frame>>`, das an jeden
  WebSocket-Client abgespielt wird (gepaced über `speed`).
- Der CAT062-Multicast-Feed spielt analog `demo_scans()` ab.

Diese Architektur ist bewusst **rein deterministisch** (ADR 0003: „gleicher
Input → gleicher Output", Reproduzierbarkeit, Replay). Echtzeit-ADS-B ist das
Gegenteil: wanduhr-getrieben, extern — der zeitliche Anfall der Plots ist nicht
vorhersagbar. Beide Modelle lassen sich nicht im selben Pfad vereinen, ohne den
Determinismus des bestehenden Demo-/Showcase-Betriebs zu zerstören.

**Wichtige Unterscheidung:** *Nicht-deterministisch* (Plots treffen
wanduhr-getrieben ein) ist nicht dasselbe wie *nicht-reproduzierbar* (man kann
den Lauf nie wieder nachstellen). Der Tracker ist bereits deterministisch
gegeben demselben Plot-Input — der Live-Modus wird reproduzierbar, sobald
alle eingehenden Plots aufgezeichnet werden. Diese Unterscheidung prägt die
gesamte Architektur dieses ADR.

## Zwei Aufzeichnungsebenen

Bevor die Live-Modus-Architektur beschrieben wird, ist die bestehende
**Ausgangs-Aufzeichnung** (SDPS-005) vom neuen Konzept der
**Eingangs-Aufzeichnung** zu trennen:

| Ebene | Was | Zweck | Format | Vorhanden? |
|-------|-----|-------|--------|-----------|
| **Ausgangs-Aufzeichnung** | CAT062/CAT065-UDP-Datagrams (Firefly → Wayfinder) | Legal Recording; Feed-Replay an ASD nach Verbindungsunterbrechung | `.ffrec` (`firefly-record` / `firefly-replay`) | ✅ SDPS-005 |
| **Eingangs-Aufzeichnung** | Plot-Strom (alle Sensoren → Tracker) | Produktions-Fehlerreproduktion; Tracker-Regression; Audit-Nachweis | `.ffplots` (neu, s. u.) | ❌ dieser ADR |

Die Ausgangs-Aufzeichnung macht den *Ausgabe-Feed* reproduzierbar — ein Wayfinder
der den Feed verpasst hat, bekommt dieselben Bytes erneut. Die
**Eingangs-Aufzeichnung** macht das *Tracking selbst* reproduzierbar: jeder
Produktions-Betrieb kann bit-genau in einer Testumgebung nachgestellt werden,
Tracker-Bugs werden nachvollziehbar.

Der CAT062-Ausgabe-Strom (Ausgangs-Aufzeichnung) spiegelt nicht vollständig,
was im Tracker vorging — er enthält keine Plots, keine Assoziation, keine
Kalman-Zwischenzustände. Nur die Plot-Eingangs-Aufzeichnung erlaubt echte
Fehlerreproduktion.

## Entscheidung (Vorschlag)

### Grundsatz: Zwei klar getrennte Betriebsmodi

Der Server bekommt einen **expliziten Modus-Schalter** statt einer Vermischung:

| Modus | Quelle | Timing | Reproduzierbarkeit | Aktivierung |
|-------|--------|--------|--------------------|-------------|
| **Replay** (heute) | simuliertes `Scenario`, vorausberechnet | deterministisch | ✅ vollständig deterministisch | Default |
| **Live** (neu) | OpenSky-ADS-B (+ zukünftig PSR/SSR, FLARM) | wanduhr-getrieben | ✅ via Plot-Aufzeichnung reproduzierbar | `FIREFLY_MODE=live` |

Die beiden Modi schließen sich **gegenseitig aus**. Ein gemischtes Bild aus
simulierten Demo-Flügen und echten ADS-B-Tracks wäre fachlich irreführend
(Lotse könnte Sim nicht von Real unterscheiden) und wird daher nicht gebaut.

Im Live-Modus gilt: **NFR-REPRO-001** (Reproduzierbarkeit) ist nicht aufgegeben,
sondern auf einem anderen Weg erfüllt — durch die lückenlose Aufzeichnung aller
eingehenden Plots statt durch Voraus-Berechnung.

### Live-Modus: Laufzeit-Architektur

```text
                    ┌─── Plot-Aufzeichnung (immer aktiv im Live-Modus) ───┐
                    │  PlotRecorder → .ffplots-Datei                      │
                    │  (wall-clock ts + serde_json(Plot) pro Record)      │
                    └─────────────────────────────────────────────────────┘
                                          ▲
OpenSkyPoller ──plots──▶ mpsc::channel ──┼──▶ LiveTracker-Task
  (wall-clock, 10 s)       (zukünftig:   │      tracker.process_plots(&plots)
  zukünftig: PSR/SSR,      auch PSR/SSR, │      periodisch (ADR 0013 t_out):
  FLARM Adapter)           FLARM)        │      tracker.system_tracks()
                                         │              │
                                         │              ▼
                                         │    ArcSwap<Vec<SystemTrack>>  ◀── live snapshot
                                         │      │                    │
                                         │  ┌───┘                    └───┐
                                         │  ▼                            ▼
                                         │  WS-Frame-Pump (web)   CAT062-Multicast-Feed
                                         │  (Frame aus Snapshot)  (encode aus Snapshot)
                                         │
                                         └─── Ausgangs-Aufzeichnung (SDPS-005, optional) ───▶ .ffrec
```

1. **Lebender Tracker als Task:** Eine langlebige `Tracker`-Instanz läuft in
   einem eigenen Tokio-Task, gefüttert per `mpsc`-Channel von allen
   Sensor-Adaptern. Der OpenSky-Poller sendet seine Plots in den Channel statt
   `tracing::info!` zu rufen. Zukünftige Adapter (PSR/SSR, FLARM) senden in
   denselben Channel.
2. **Plot-Aufzeichnung (immer aktiv im Live-Modus):** Alle Plots, die in den
   Channel geschrieben werden, werden **zuvor** durch einen `PlotRecorder` geleitet:
   wall-clock-Timestamp (Unix-ns) + `serde_json::to_vec(&plot)` → Record in
   `.ffplots`-Datei. Die Aufzeichnung ist transparent für den Tracker (er sieht
   dieselben Plots). Die Datei ermöglicht bit-genaue Reproduktion des gesamten
   Tracking-Laufs.
3. **Datenzeit-Treiber (ADR 0013):** Der Tracker arbeitet nach **Datenzeit**
   (Plot-Timestamp), nicht Wanduhr. Die Pro-Plot-Verarbeitung und der periodische
   Ausgabetakt aus ADR 0013 bleiben gültig. Nur die *Ankunft* der Plots ist
   wanduhr-getrieben; der Tracker selbst bleibt deterministisch gegeben demselben
   Plot-Strom.
4. **Geteilter Live-Snapshot:** Der Task veröffentlicht nach jedem Ausgabetakt
   einen frischen `Vec<SystemTrack>` über `tokio::sync::watch`. Leser (WS-Pump,
   CAT062-Feed) lesen den jeweils neuesten Snapshot ohne den Tracker zu blockieren.
5. **`AppState` wird modusabhängig:** `AppState` trägt statt `frames: Arc<Vec<Frame>>`
   eine `FrameSource`-Abstraktion: `Replay(Arc<Vec<Frame>>)` (heute) oder
   `Live(watch::Receiver<…>)` (neu). WS-Pump und CAT062-Feed lesen aus dieser
   gemeinsamen Abstraktion.
6. **CAT062-Feed im Live-Modus:** Statt `demo_scans()` abzuspielen, kodiert der
   Feed bei jedem Ausgabetakt den aktuellen Live-Snapshot. Der Encoder und der
   Draht-Vertrag (ICD 2.4.0) bleiben **unverändert** — nur die Quelle der
   `SystemTrack`s wechselt.

### `.ffplots`-Dateiformat (Eingangs-Aufzeichnung)

Analoges Minimal-Format zu `.ffrec`, quellenagnostisch auf `Plot`-Ebene:

```text
┌─────────────────────────────────────────────────────┐
│  Datei-Header (16 Bytes)                            │
│    magic:    8 bytes = b"FFPLOTS\x00"               │
│    version:  1 byte  = 0x01                         │
│    reserved: 7 bytes = 0x00…                        │
├─────────────────────────────────────────────────────┤
│  Record pro Plot                                    │
│    timestamp_unix_ns: u64 big-endian (8 B)          │
│    payload_len:       u16 big-endian (2 B)          │
│    payload:           serde_json(Plot), UTF-8       │
└─────────────────────────────────────────────────────┘
```

Begründung für JSON statt Binär-Format:
- `Plot` leitet `serde::Serialize/Deserialize` bereits ab (für andere Zwecke).
- JSON ist selbstbeschreibend und mit Standard-Tools inspeziierbar (Audit-Vorteil).
- Die Datenmenge ist überschaubar: ADS-B ~100 Flugzeuge × alle 10 s × ~200 Bytes
  JSON ≈ 2 KB/s; PSR/SSR-Radar bei 1 s-Scan und 100 Plots ≈ ~20 KB/s. Für
  operative Speicherdauern (24 h) sind das ~1,7 GB/Tag (PSR). Akzeptabel für
  einen Produktionsserver.

Neue Binaries in `firefly-recorder/src/bin/`:
- `firefly-record-plots`: Empfängt Plots per lokalem Channel vom Server, schreibt
  `.ffplots`.
- `firefly-replay-plots`: Liest `.ffplots`, gibt Plots mit originalem Zeitabstand
  in einen Fake-Tracker (oder realen Tracker-Task) — reproduziert den Lauf
  deterministisch. Unterstützt `FIREFLY_REPLAY_SPEED`-Skalierung wie das
  bestehende `firefly-replay`.

### `.ffplots`-Replay ergibt deterministischen Tracker-Output

Das ist der entscheidende Punkt: Der Tracker ist bereits deterministisch
gegeben derselben Plot-Sequenz (ADR 0013, ADR 0003). Beim Replay aus `.ffplots`:

1. Gleiche Plot-Objekte (gleiche Messwerte, gleiche Timestamps) → gleicher
   Kalman-Verlauf.
2. Gleiche JPDA-Zuordnung (gleiche Mahalanobis-Distanzen, gleiche β-Werte).
3. Gleiche Track-Lebenszyklusentscheidungen (Birth/Confirm/Coast/Delete).
4. Gleicher CAT062-Ausgabe-Strom (optional gegen `.ffrec`-Referenz prüfbar).

Damit ist **jeder Produktions-Fehler reproduzierbar**: `.ffplots`-Datei aus
Produktion kopieren, lokal gegen Debugger / Testharness abspielen.

> **Bit-Genauigkeit beim JSON-Parse (AP9.4c-2):** `serde_json` serialisiert f64
> bereits über `ryu` exakt, der **Parser** ist per Default aber nur näherungsweise
> (kann ein einzelnes ULP danebenliegen). Für bit-genaue Wiedergabe ist deshalb
> das `float_roundtrip`-Feature von `serde_json` workspace-weit aktiviert — sonst
> würde ein Replay-Plot in seltenen Fällen um 1 ULP von der Aufzeichnung
> abweichen und der Determinismus (Punkt 1 oben) wäre verletzt.

### Erweiterbarkeit auf zukünftige Quellen

Das Design ist absichtlich quellenagnostisch auf der Plot-Ebene. Zukünftige
Sensoren (PSR/SSR via ASTERIX CAT001/CAT048, FLARM über OGFLARM, andere
ADS-B-Empfänger) schreiben in denselben mpsc-Channel. Die Plot-Aufzeichnung
läuft automatisch mit — ohne Änderung am Recorder. Das `.ffplots`-Format kennt
keine Quellenfelder (die Quelle steckt bereits im `Plot::sensor`-Feld).

### Health/Readiness im Live-Modus

`/ready` wird im Live-Modus erst „ready", wenn der erste OpenSky-Poll
erfolgreich war (analog zu Wayfinders Feed-Staleness-Logik). Bleibt OpenSky
aus, meldet der Server „not ready" statt ein leeres Bild als gesund auszugeben.
Neue Metriken: `firefly_live_plots_ingested_total`, `firefly_live_tracks_current`,
`firefly_opensky_poll_errors_total`, `firefly_plot_records_written_total`.

### Was sich NICHT ändert

- Der **CAT062-Draht-Vertrag** (kein Schnittstellen-Impact, keine ICD-Änderung).
- Der **Tracker-Kern** (`firefly-track`) — er kann bereits live gefüttert werden
  (`process_plots`); es kommt kein neuer Tracking-Code hinzu.
- Der **Replay-Modus** — Demo/Frankfurt-Showcase laufen byte-genau wie bisher,
  inkl. aller bestehenden Tests.
- Die **Determinismus-Garantie für Replay-Modus** — sie wird nicht aufgeweicht.
- Die **Ausgangs-Aufzeichnung** (SDPS-005) — sie bleibt unverändert, ist aber
  komplementär zur neuen Eingangs-Aufzeichnung.

## Alternativen

- **A — Live ersetzt Replay vollständig:** Verworfen. Der Demo-/Showcase-Betrieb
  (M6, Tests, Onboarding) hängt am deterministischen Replay.
- **B — Live + Replay im selben Bild gemischt:** Verworfen (fachlich
  irreführend, Sim vs. Real nicht unterscheidbar).
- **C — Nur der CAT062-Feed wird live, Web-Karte bleibt Demo:** Verworfen als
  inkonsistent.
- **D — Status quo lassen (Poller logging-only):** Tragfähig als Dauerzustand,
  aber ADS-B-Integration liefert nie ein echtes End-to-End-Bild.
- **E — Eingangs-Aufzeichnung auf Roh-Ebene (HTTP-Payload / UDP-Datagramm):**
  Verworfen zugunsten Plot-Ebene. Roh-Aufzeichnung würde quellenspezifische
  Replay-Logik benötigen (OpenSky-JSON-Parser ≠ ASTERIX-Decoder ≠ FLARM-Parser).
  Die Plot-Ebene ist quellenagnostisch und direkt replay-fähig durch den
  bestehenden Tracker.

## Konsequenzen

### Positiv
- Echte ADS-B-Tracks erscheinen end-to-end auf Karte und im CAT062-Strom.
- **Jeder Produktions-Lauf ist reproduzierbar** via `.ffplots`-Datei — kein
  „nicht reproduzierbar weil Live" mehr.
- Firefly wird zum echten Multi-Source-Live-System (Schritt Richtung ADR 0006).
- Klare, dokumentierte Trennung der beiden Modi; Determinismus-Garantie für
  Replay bleibt unverändert.
- Eingangs-Aufzeichnung ist von Anfang an quellenagnostisch — PSR/SSR, FLARM
  kommen gratis mit, wenn die Adapter implementiert werden.
- Kein Schnittstellen-Impact auf Wayfinder.

### Negativ / Einschränkungen
- **Neue Laufzeit-Komplexität:** lebender Tracker-Task, geteilter Snapshot,
  modusabhängiges `AppState`, PlotRecorder — mehr bewegliche Teile als der
  reine Replay.
- **Speicherplatz `.ffplots`:** ~1,7 GB/Tag bei PSR-Dichte; Rotation /
  Aufbewahrungspolitik muss konfigurierbar sein (FIREFLY_PLOT_RECORD_MAX_BYTES
  o.ä.) — nicht Teil dieses ADR, aber vorherzuplanen.
- **OpenSky-Abhängigkeit zur Laufzeit:** Rate Limits, Latenz ~5–10 s,
  Netzausfälle müssen robust (degradiert, nicht abstürzend) behandelt werden.
- **Testaufwand:** Der Live-Pfad braucht Tests mit einem Fake-Plot-Producer;
  der Replay-Pfad muss unverändert grün bleiben; der `.ffplots`-Recorder
  braucht eigene Round-Trip-Tests.

## Umsetzungsplan (nach Freigabe, kleine Häppchen)

| Schritt | Inhalt | Komplex. | Modell |
|---------|--------|----------|--------|
| **AP9.4c-0** ✅ | `.ffplots`-Format in `firefly-recorder`: `write_plot_file_header`/`write_plot_record`/`read_plot_record`; `Plot` serde-fähig; 6 Round-Trip-Tests | S2 | Opus 4.8 |
| **AP9.4c-1** ✅ | `FrameSource`-Abstraktion + `AppState` modusfähig; Replay-Pfad unverändert grün | S3 | Sonnet 4.6 |
| **AP9.4c-2** ✅ | LiveTracker-Task: Channel vom Poller, `process_plots` nach Datenzeit, Snapshot-Publish (`watch`); `PlotRecorder` schreibt parallel | S4 | Opus 4.8 |
| **AP9.4c-3** | WS-Pump + CAT062-Feed lesen Live-Snapshot; Mode-Switch in `main.rs` (`FIREFLY_MODE`) | S4 | Opus 4.8 |
| **AP9.4c-4** | Readiness/Metriken im Live-Modus; Robustheit bei OpenSky-Ausfall | S3 | Sonnet 4.6 |
| **AP9.4c-5** | `firefly-replay-plots` Binary; Integration-Test: Replay aus `.ffplots` → gleicher CAT062-Strom wie Live-Lauf | S3 | Sonnet 4.6 |
| **AP9.4c-6** | Tests (Fake-Producer, Replay-Regression), Milestone-Doku, ADR auf „akzeptiert" | S2 | Sonnet 4.6 |

## Entschiedene Fragen (bei Freigabe)

1. **Snapshot-Primitive:** ✅ `tokio::sync::watch` — keine neue Abhängigkeit,
   „letzter Wert gewinnt"-Semantik passt zum Live-Snapshot.
2. **Modus-Schalter-Name:** ✅ expliziter `FIREFLY_MODE=live|replay` — der
   Betriebsmodus ist eindeutig und auditierbar.
3. **Geo-Referenzpunkt im Live-Modus:** ✅ Default = Mittelpunkt der
   konfigurierten OpenSky-Bounding-Box (verknüpft mit „Konfigurierbarer
   System-Referenzpunkt", Roadmap).
4. **Aufbewahrungspolitik `.ffplots`:** ✅ zunächst keine Auto-Rotation (YAGNI),
   eine Datei pro Lauf; Rotation/Größenlimit als eigenes Vorhaben, wenn der
   operative Bedarf entsteht.
