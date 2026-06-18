# ADR 0020 — Live-Tracker-Modus für Echtzeit-ADS-B

- **Status:** Vorschlag — zur Freigabe (noch nicht akzeptiert)
- **Datum:** 2026-06-18
- **Schnittstellen-relevant:** nein (CAT062-Draht-Vertrag bleibt unverändert;
  ICD 2.4.0 deckt das ES-Age-Subfeld bereits ab). Diese Entscheidung betrifft
  ausschließlich die **interne Laufzeit-Architektur** des `firefly-server`.
- **Baut auf:** ADR 0003 (Cloud-native / Determinismus), ADR 0013 (asynchrone
  Pro-Plot-Verarbeitung + periodischer Ausgabetakt), ADR 0019 (ADS-B via
  OpenSky).

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
Gegenteil: wanduhr-getrieben, extern, nicht reproduzierbar. Beide Modelle lassen
sich nicht im selben Pfad vereinen, ohne den Determinismus des bestehenden
Demo-/Showcase-Betriebs zu zerstören.

## Entscheidung (Vorschlag)

### Grundsatz: Zwei klar getrennte Betriebsmodi

Der Server bekommt einen **expliziten Modus-Schalter** statt einer Vermischung:

| Modus | Quelle | Determinismus | Aktivierung |
|-------|--------|---------------|-------------|
| **Replay** (heute) | simuliertes `Scenario`, vorausberechnet | ✅ deterministisch | Default (`Scene::Demo`/`Frankfurt`) |
| **Live** (neu) | OpenSky-ADS-B, Echtzeit | ❌ bewusst nicht-deterministisch | `FIREFLY_MODE=live` (impliziert `FIREFLY_OPENSKY_ENABLED=true`) |

Die beiden Modi schließen sich **gegenseitig aus**. Ein gemischtes Bild aus
simulierten Demo-Flügen und echten ADS-B-Tracks wäre fachlich irreführend
(Lotse könnte Sim nicht von Real unterscheiden) und wird daher nicht gebaut.
Die **Determinismus-Grenze** verläuft exakt am Modus-Schalter: Replay bleibt
unverändert reproduzierbar; Live ist als Echtzeit-Pfad ausdrücklich von der
Reproduzierbarkeits-Anforderung (NFR-REPRO-001) ausgenommen.

### Live-Modus: Architektur

```text
OpenSkyPoller ──plots──▶ mpsc::channel ──▶ LiveTracker-Task
  (wall-clock, 10 s)                          │  tracker.process_plots(&plots)
                                              │  periodisch (ADR 0013 t_out):
                                              │  tracker.system_tracks()
                                              ▼
                                      ArcSwap<Vec<SystemTrack>>  ◀── live snapshot
                                       │                    │
                            ┌──────────┘                    └──────────┐
                            ▼                                          ▼
                    WS-Frame-Pump (web)                      CAT062-Multicast-Feed
                    (Frame aus Snapshot)                     (encode aus Snapshot)
```

1. **Lebender Tracker als Task:** Eine langlebige `Tracker`-Instanz läuft in
   einem eigenen Tokio-Task, gefüttert per `mpsc`-Channel vom OpenSky-Poller.
   Der Poller ruft nicht mehr `tracing::info!`, sondern sendet die Plots in den
   Channel.
2. **Datenzeit-Treiber (ADR 0013):** Der Tracker arbeitet weiter nach
   **Datenzeit** (Plot-Timestamp), nicht Wanduhr — die Pro-Plot-Verarbeitung und
   der periodische Ausgabetakt aus ADR 0013 bleiben gültig. Nur die *Ankunft*
   der Plots ist wanduhr-getrieben.
3. **Geteilter Live-Snapshot:** Der Task veröffentlicht nach jedem Ausgabetakt
   einen frischen `Vec<SystemTrack>` über einen lock-freien `ArcSwap` (oder
   `tokio::sync::watch`). Leser (WS-Pump, CAT062-Feed) lesen den jeweils neuesten
   Snapshot ohne den Tracker zu blockieren.
4. **`AppState` wird modusabhängig:** Statt `frames: Arc<Vec<Frame>>` trägt
   `AppState` eine Quelle, die entweder den vorausberechneten Vektor abspielt
   (Replay) oder den Live-Snapshot in `Frame`s wandelt (Live). Kapselung über
   ein kleines `enum FrameSource { Replay(Arc<Vec<Frame>>), Live(watch::Receiver<…>) }`.
5. **CAT062-Feed im Live-Modus:** Statt `demo_scans()` abzuspielen, kodiert der
   Feed bei jedem Ausgabetakt den aktuellen Live-Snapshot. Der Encoder und der
   Draht-Vertrag (ICD 2.4.0) bleiben **unverändert** — nur die Quelle der
   `SystemTrack`s wechselt.

### Was sich NICHT ändert

- Der **CAT062-Draht-Vertrag** (kein Schnittstellen-Impact, keine ICD-Änderung).
- Der **Tracker-Kern** (`firefly-track`) — er kann bereits live gefüttert werden
  (`process_plots`); es kommt kein neuer Tracking-Code hinzu.
- Der **Replay-Modus** — Demo/Frankfurt-Showcase laufen byte-genau wie bisher,
  inkl. aller bestehenden Tests.
- Die **Determinismus-Garantie für Replay** — sie wird nicht aufgeweicht,
  sondern der nicht-deterministische Pfad wird sauber daneben gestellt.

### Health/Readiness im Live-Modus

`/ready` wird im Live-Modus erst „ready", wenn der erste OpenSky-Poll erfolgreich
war (analog zu Wayfinders Feed-Staleness-Logik). Bleibt OpenSky aus, meldet der
Server „not ready" statt ein leeres Bild als gesund auszugeben (ADR 0003).
Neue Metriken: `firefly_live_plots_ingested_total`, `firefly_live_tracks_current`,
`firefly_opensky_poll_errors_total`.

## Alternativen

- **A — Live ersetzt Replay vollständig:** Verworfen. Der Demo-/Showcase-Betrieb
  (M6, Tests, Onboarding) hängt am deterministischen Replay; er darf nicht
  wegfallen.
- **B — Live + Replay im selben Bild gemischt:** Verworfen (fachlich
  irreführend, Sim vs. Real nicht unterscheidbar; siehe oben).
- **C — Nur der CAT062-Feed wird live, Web-Karte bleibt Demo:** Verworfen als
  inkonsistent — die Karte würde etwas anderes zeigen als der Feed.
- **D — Status quo lassen (Poller logging-only):** Tragfähig als Dauerzustand,
  aber dann liefert die ADS-B-Integration nie ein echtes End-to-End-Bild. Dieser
  ADR existiert, um D bewusst zu überwinden.

## Konsequenzen

### Positiv
- Echte ADS-B-Tracks erscheinen end-to-end auf Karte und im CAT062-Strom.
- Firefly wird zum echten Multi-Source-Live-System (Schritt Richtung ADR 0006).
- Klare, dokumentierte Determinismus-Grenze statt schleichender Vermischung.
- Kein Schnittstellen-Impact auf Wayfinder.

### Negativ / Einschränkungen
- **Neue Laufzeit-Komplexität:** lebender Tracker-Task, geteilter Snapshot,
  modusabhängiges `AppState` — mehr bewegliche Teile als der reine Replay.
- **Live-Pfad nicht reproduzierbar** (bewusst, dokumentiert; NFR-REPRO-001 gilt
  nur für Replay).
- **OpenSky-Abhängigkeit zur Laufzeit:** Rate Limits, Latenz ~5–10 s,
  Netzausfälle müssen robust (degradiert, nicht abstürzend) behandelt werden.
- **Testaufwand:** Der Live-Pfad braucht Tests mit einem Fake-Plot-Producer
  (kein echter Netzzugriff im Test); der Replay-Pfad muss unverändert grün
  bleiben.

## Umsetzungsplan (nach Freigabe, kleine Häppchen)

| Schritt | Inhalt | Komplex. | Modell |
|---------|--------|----------|--------|
| **AP9.4c-1** | `FrameSource`-Abstraktion + `AppState` modusfähig; Replay-Pfad unverändert grün | S3 | Sonnet 4.6 |
| **AP9.4c-2** | LiveTracker-Task: Channel vom Poller, `process_plots` nach Datenzeit, Snapshot-Publish (`watch`/`ArcSwap`) | S4 | Opus 4.8 |
| **AP9.4c-3** | WS-Pump + CAT062-Feed lesen Live-Snapshot; Mode-Switch in `main.rs` | S4 | Opus 4.8 |
| **AP9.4c-4** | Readiness/Metriken im Live-Modus; Robustheit bei OpenSky-Ausfall | S3 | Sonnet 4.6 |
| **AP9.4c-5** | Tests (Fake-Producer, Replay-Regression), Milestone-Doku, ADR auf „akzeptiert" | S3 | Sonnet 4.6 |

## Offene Fragen (vor/zur Freigabe)

1. **Snapshot-Primitive:** `tokio::sync::watch` (in der Tokio-Abhängigkeit
   enthalten, kein neuer Crate) vs. `arc-swap` (lock-frei, minimal). Empfehlung:
   `watch` — keine neue Abhängigkeit, passt zum „letzter Wert gewinnt".
2. **Modus-Schalter-Name:** `FIREFLY_MODE=live|replay` (explizit) vs. „Live an,
   sobald `FIREFLY_OPENSKY_ENABLED=true`" (implizit). Empfehlung: expliziter
   `FIREFLY_MODE`, damit der Betriebsmodus eindeutig und auditierbar ist.
3. **Geo-Referenzpunkt im Live-Modus:** Der `LocalFrame`-Ursprung muss zur
   konfigurierten OpenSky-Bounding-Box passen (verknüpft mit dem offenen Punkt
   „Konfigurierbarer System-Referenzpunkt", Roadmap). Default: Box-Mittelpunkt.
