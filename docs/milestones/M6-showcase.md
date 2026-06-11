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

### Tuning-Entscheidungen — ehrlich dokumentiert

Eine Szene mit drei Radaren und acht Flugzeugen ist deutlich anspruchsvoller
für den Tracker als die Zwei-Flugzeug-Demo. Beim Aufbau sind zwei reale
Fusions-Phänomene aufgetreten, die hier bewusst **durch Szenen-Design
umschifft** statt im Tracker-Kern behoben wurden — beides sind aber
interessante, eigenständige Folge-Themen (siehe „Offene Punkte" in
`docs/STATUS.md`):

1. **Höhenfehler beim späten Eintritt in ein zweites Radar
   („Höhen-Projektionsfehler")** — Wenn ein hoch fliegendes Flugzeug erst
   *mitten im Flug* in die Reichweite eines zweiten Radars eintritt, während
   sein Track vom ersten Radar bereits eng eingerastet ist (enges Tor), kann
   die erste Messung des zweiten Radars knapp **außerhalb** dieses engen
   Tores liegen — Ursache ist, dass `horizontal_from` eine Bodenmessung
   entlang der jeweiligen lokalen „Oben"-Richtung projiziert, und diese
   Richtung sich zwischen zwei ~50–90 km entfernten Radarstandorten leicht
   unterscheidet (wenige zehn bis ~100 m Unterschied in der Bodenposition bei
   10 km Höhe). Das reicht, um eine zusätzliche, bestätigte „Geister"-Spur zu
   erzeugen. In der Realität korrigieren ATC-Systeme genau das über
   höhenabhängige „System-Error"-Korrekturen.
   - **Workaround in M6.1:** Die Reichweiten von `radar_west` (80 km) und
     `radar_northeast` (65 km) sowie die Startposition von `arrival_north`
     sind so gewählt, dass kein Flugzeug erst mitten im Flug in die
     Reichweite eines zweiten Radars eintritt — jedes Flugzeug ist von Anfang
     an entweder im Überlappungsbereich oder gar nicht.
2. **Asynchrone Radar-Scans (`scan_offset`)** — Ein realistisches Setup hätte
   die drei Radare zeitversetzt scannen lassen (z. B. 0 s/1,3 s/2,6 s
   Versatz bei 4-s-Scan-Periode). In dieser dichten 8-Flugzeug-Szene führte
   das jedoch zu massiver Track-Instabilität (50–90 statt 8 Track-IDs) — die
   genaue Ursache ist noch nicht analysiert.
   - **Workaround in M6.1:** Alle drei Radare scannen synchron
     (`scan_period = 4.0`, kein `scan_offset`).
3. **JPDA-Tor leicht geweitet:** `tracker.gate = Gate::from_probability(0.999)`
   statt des Standards `0.99` — behebt zwei verbleibende „Geister"-Spuren, die
   genau an den Manöver-Übergängen von `departure_turning` (AC4, IMM-Schaufenster)
   entstanden (kurzer Gate-Verlust beim Wechsel Beschleunigung → Kurve → Geradeausflug).

Mit allen drei Anpassungen läuft die Szene über die vollen 240 s mit exakt
acht Track-IDs und nie mehr als acht Tracks pro Frame.

---

## Ausblick

- **M6.2** — OpenStreetMap als Hintergrundkarte (statt der MapLibre-Demo-Kacheln).
- **M6.3** — Roh-Plot-Transparenz-Ebene: zeigt im Frontend zusätzlich die
  Radar-Plots, *bevor* sie der Tracker zu Tracks verarbeitet — inkl. des
  primary-only-Überflugs (`overflight_primary`), der nie eine SSR-Identität
  bekommt.
- **M6.4** — Dockerfile/docker-compose für den lokalen Start analog zur Cloud.
