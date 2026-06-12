# M6 — Showcase Frankfurt + Container-Setup

> Verständliche Erklärung des sechsten Meilensteins. Begriffe stehen
> ausführlicher im [Glossar](../glossary.md).

Mit M1–M5 steht der ganze Tracker-Kern: Simulator, Single- und Multi-Radar-
Tracking, IMM für Manöver, JPDA für dichten Verkehr. M6 zeigt das Ergebnis
**her** — als „Tag im Leben" einer Flugsicherungsstelle: ein dichter
Flughafen mit mehreren Radaren, transparent dargestellt, lokal startbar wie
in der Cloud. Vier Häppchen:

- **M6.1** — Frankfurt-Szene: drei Radare, acht Flugzeuge (dieses Häppchen).
- **M6.2** — OpenStreetMap-Hintergrundkarte im Frontend.
- **M6.3** — Roh-Plot-Ebene: zeigt zusätzlich, *was* der Tracker empfängt
  (nicht nur, was er ausgibt).
- **M6.4** — Container-Setup (Dockerfile/docker-compose) für den lokalen
  Start analog zur Cloud.

---

## Häppchen M6.1 — Frankfurt-Szene

**Status:** ✅ umgesetzt · Anforderung `NFR-OPS-001` (Showcase-Erweiterung)

### Das Problem (fachlich)

Die bisherige Demo-Szene (`scene::demo_frames`) zeigt zwei Flugzeuge unter
einem Radar — gut zum Verstehen der Grundlagen, aber zu klein, um zu zeigen,
*warum* M4 (Multi-Radar-Fusion) und M5 (IMM/JPDA) überhaupt nötig sind. Die
Frankfurt-Szene bildet ein „Tag im Leben" nach: drei Radarstandorte mit sich
überlappender Reichweite und acht Flugzeuge, die typische Situationen
durchspielen:

- **Zwei West-Anflüge** (`arrival_west_a`/`b`), die nur ~150 m parallel
  auseinander liegen — die Tore (Gates) der beiden Tracks überlappen sich.
  Genau für diese Situation wurde JPDA (M5.5–M5.9) gebaut: die beiden Spuren
  dürfen sich nicht zu einer verschmelzen oder die Identität tauschen.
- **Zwei Abflüge**, einer geradeaus beschleunigend, einer mit einer
  2°/s-Kurve nach dem Steigflug — letzterer ist das **IMM-Schaufenster**
  (Manöver-Erkennung, M5.1–M5.4).
- **Zwei Überflüge**, einer SSR-ausgerüstet (mit Mode-3/A und ICAO-Adresse),
  einer **primary-only** (kein Transponder) — letzterer zeigt in M6.3 nur als
  Roh-Plot ohne Identität, weil der Tracker für ihn nie eine SSR-Antwort
  bekommt.
- **Eine Warteschleife** (zwei 180°-Kurven bei 3°/s) und ein **Nordanflug**,
  der durchgehend von zwei Radaren gleichzeitig gesehen wird — ein
  Multi-Radar-Überlappungsgebiet.

### Die Lösung (technisch)

Neue Funktionen in `crates/firefly-server/src/scene.rs`, nach demselben Muster
wie `demo_player`/`demo_frames`/`demo_scans`:

- `frankfurt_player()` baut das Szenario: Ursprung am Frankfurter
  Flughafen-Referenzpunkt (`FRANKFURT_ORIGIN = (50.0379, 8.5622)`), drei
  Radare (`radar_center`, `radar_west`, `radar_northeast`) mit
  `TrackerConfig::new(...).with_sensor(...)` für jeden Standort, und acht
  `Target`s (siehe oben).
- `frankfurt_frames()` / `frankfurt_scans()` liefern denselben deterministischen
  Player-Lauf für das Web-Frontend (JSON/WebSocket) bzw. die CAT062-Multicast-
  Ausgabe — exakt wie bei der bisherigen Demo (ADR 0006).
- Zwei Regressionstests (`frankfurt_scene_is_non_trivial`,
  `frankfurt_scene_keeps_one_identity_per_aircraft`) prüfen über den ganzen
  240-s-Lauf: acht Flugzeuge → **acht** Track-IDs, nie mehr als acht Tracks
  gleichzeitig in einem Frame — kein Zerbrechen, keine Geister.
- **Szenenauswahl** (`crates/firefly-server/src/config.rs`): neues
  `Scene`-Enum (`Demo` | `Frankfurt`) und Feld `ServerConfig::scene`,
  12-Factor wie alle anderen Einstellungen — `FIREFLY_SCENE=frankfurt` schaltet
  um, alles andere (inkl. unbekannter Werte) bleibt beim Default `Demo`.
  `main.rs` wählt anhand von `config.scene` zwischen `scene::demo_*` und
  `scene::frankfurt_*` für *beide* Ausgabe-Adapter (WebSocket und CAT062).

### Multi-Radar-Handover auf Flughöhe — und der dafür behobene Höhenfehler

Eine Szene mit drei Radaren und acht Flugzeugen ist deutlich anspruchsvoller
für den Tracker als die Zwei-Flugzeug-Demo. Beim Aufbau trat ein reales
Fusions-Phänomen auf, das die Frankfurt-Szene erst sauber machte, **nachdem
es im Tracker-Kern behoben** war:

1. **Höhen-Projektionsfehler (behoben, FR-GEO-003).** Tritt ein hoch fliegendes
   Flugzeug erst *mitten im Flug* in die Reichweite eines zweiten Radars ein,
   während sein Track vom ersten Radar bereits eng eingerastet ist (enges Tor),
   lag die erste Messung des zweiten Radars früher knapp **außerhalb** dieses
   Tores — Ursache: `horizontal_from` projizierte die Messung im *Quellrahmen*
   auf den Boden (`up = 0`), und diese „Senkrechte" zeigt an zwei zig Kilometer
   entfernten Radarstandorten leicht verschieden (wenige zehn bis ~100 m
   Versatz bei 10 km Höhe) → zusätzliche, bestätigte „Geister"-Spur. **Die
   Lösung** (eigener Schritt, vor diesem Häppchen): `horizontal_from` bekommt
   die Zielhöhe, rekonstruiert den **vollständigen 3D-Punkt** und projiziert
   **erst im gemeinsamen Tracking-Frame** auf den Boden — damit ist das
   Horizontalergebnis sensor-unabhängig. Details im Glossar
   („Höhen-Projektionsfehler") und im Regressionstest
   `airborne_target_maps_to_one_point_from_two_sensors`.
   - **Wirkung in M6.1:** Die Radare laufen wieder mit **realistischen,
     überlappenden Reichweiten** (Center 120 km, West/Nordost je 100 km), und
     `arrival_north` ist bewusst ein **Handover auf 8 km Höhe** — es wird zuerst
     nur vom Nordost-Radar gesehen und tritt mitten im Flug in das Zentrum-Radar
     ein. Genau der Fall, der vorher Geister erzeugte; jetzt bleibt es **eine**
     Spur.

2. **Geister-Spuren bei Gate `0,99` (behoben, ADR 0011).** Anfangs war das
   Assoziations-Tor auf `0,999` geweitet, um zwei „Geister"-Spuren zu
   unterdrücken. Eine Diagnose zeigte: das sind **Multi-Radar-Fusions-Artefakte**
   (kein IMM-Manöver), mit zwei Ursachen — (a) die **sequenzielle Tor-Verengung**
   (Sensor A aktualisiert → Tor enger → Sensor Bs Plot fällt heraus → Duplikat)
   und (b) ein einzelner **3σ-Ausreißer-Plot**, der sofort einen Track gebärt.
   Beide sind jetzt an der Wurzel behoben: eine **zu Scan-Beginn eingefrorene
   Fusions-Referenz** und ein **getrenntes, weiteres Initiierungs-Sperr-Tor**
   (FR-TRK-020). Die Szene läuft damit wieder mit dem **Standard-Tor `0,99`**.

3. **Asynchrone Radar-Scans (behoben, ADR 0012).** Ein realistisches Setup
   lässt die drei Radare zeitversetzt scannen (`scan_offset = 0 / 1.3 / 2.6 s`
   bei `scan_period = 4.0`). Mit dem alten, scan-zählenden Lebenszyklus führte
   das zu massiver Track-Instabilität (28–90 statt 8 IDs): er buchte
   Treffer/Fehltreffer **pro `process_scan`-Aufruf**, aber mit Versatz trägt
   jeder Aufruf nur **einen** Sensor — ein Flugzeug, das nur Radar B sieht,
   kassierte beim Offset-Scan von Radar A einen falschen „Miss". **Der
   adaptive Lebenszyklus** (FR-TRK-021) zählt Bestätigung/Löschung stattdessen
   in `coast_reference = max(revisit_interval, cadence)` Sekunden — dem
   Maximum aus dem per-Track-EWMA der Treffer-Zeitlücken (`revisit_interval`)
   und der vom Tracker geschätzten Feed-Taktung (`cadence`). Ein kurzer
   Versatz zwischen zwei Radaren wird so nicht mehr als verpasste Wiederkehr
   gewertet.

Mit dem Höhenfix, dem Geister-Fix und dem adaptiven Lebenszyklus läuft die
Szene — jetzt mit den realistischen, versetzten Radar-Scanzeiten
(`scan_offset = 0 / 1.3 / 2.6 s`) — über die vollen 240 s mit exakt acht
Track-IDs und nie mehr als acht Tracks pro Frame.

---

## M6.2 — OpenStreetMap + Lufträume-Overlay (erledigt)

Die Kartendarstellung nutzt jetzt **echte OpenStreetMap-Tiles** statt der
MapLibre-Demo-Kacheln, mit inline MapLibre-Style (schneller, wartbarer). Neue
**Airspace-Overlay-Ebene** für Lufträume (TMA, CTR, Restricted Areas) mit
GeoJSON-Quelle (`/airspaces.geojson`); der Nutzer kann die Layer mit einem
Button im HUD umschalten. Beispiel-Lufträume für Frankfurt sind in der
statischen `airspaces.geojson` definiert. Die echte Integration mit der
OpenAIP-API (für live Luftraumdaten) bleibt ein zukünftiger Schritt (M6.3 oder
später).

---

## M6.3 — Roh-Plot-Transparenz-Ebene (erledigt)

Das Frontend zeigt jetzt zusätzlich die **rohen Radar-Plots** — das, was der
Tracker als *Input* bekommt, bevor daraus Tracks werden. Dazu trägt das
`Frame` (`firefly-io`, FR-IO-001) neben den Tracks ein neues Feld
`plots: Vec<FramePlot>` (Position in Grad + `has_ssr`-Flag). Im HUD blendet
der Button „raw plots" kleine rote Punkte über die Karte. Besonders sichtbar
beim primary-only-Überflug (`overflight_primary`): er erscheint nur als
Roh-Plot, nie als identifizierter Track, weil der Tracker für ihn nie eine
SSR-Antwort bekommt. Die Umrechnung der Plot-Positionen von
Sensor-Polarkoordinaten (Entfernung/Azimut/Elevation relativ zum Radar) nach
WGS84 (Breite/Länge) übernimmt [`Player::frames`](../../crates/firefly-player/src/lib.rs)
serverseitig über die geometrischen Bausteine aus `firefly-geo`
(`Polar::to_enu` + `LocalFrame::enu_to_geodetic`), je nach Sensor des Plots.

### Coasting-Anzeige bei mehreren Radaren

Bei mehreren Radaren mit gestaffelten Scan-Zyklen (`scan_offset`, ADR 0012)
liefert jeder Scan ein eigenes `Frame` — und ein Track, der gerade nur von
*einem* der Radare aktualisiert wurde, hätte im jeweils anderen Frame
formal den rohen „coasting"-Status (`Track::is_coasting`), obwohl er insgesamt
aktuell ist. Damit das nicht als ständiges Blinken zwischen Blau und Orange
sichtbar wird, zeigt das Frontend „coasting" erst, wenn `update_age_s` länger
als eine Scan-Periode (`COAST_DISPLAY_THRESHOLD_S = 5.0 s`,
`crates/firefly-server/static/index.html`) zurückliegt — also wirklich seit
mehr als einem vollen Umlauf keine frische Messung mehr da war.

## M6.4 — Container-Setup (erledigt)

Ein `Dockerfile` (Multi-Stage-Build: `rust:1.82-bookworm` zum Kompilieren,
`debian:bookworm-slim` als ~50-MB-Laufzeit-Image) und ein `docker-compose.yml`
erlauben den Start per `docker-compose up` — ohne lokale Rust-Installation,
identisch zum Cloud-Deployment. Konfiguration ausschließlich über
Umgebungsvariablen (`FIREFLY_SCENE`, `RUST_LOG`, 12-Factor), Healthcheck gegen
`/health`. Details und Cloud-Hinweise (Kubernetes, Cloud Run, ECS) in
[`DOCKER.md`](../../DOCKER.md).

## Ausblick

- Live-Anbindung der OpenAIP-API für reale Luftraumdaten (statt der
  statischen Beispiel-`airspaces.geojson`).
