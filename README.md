# Firefly

Ein web-basierter **Radar-Tracker** — das Rechen-Herzstück einer
Luftlagedarstellung (*air-situation picture*). Firefly verwandelt die
verrauschten, lückenhaften Meldungen von Primär- (PSR) und Sekundärradar (SSR)
in saubere, durchgehende **Tracks**: Position, Geschwindigkeit und Identität
jedes Flugzeugs im Überwachungsbereich — live als 2D-Karte im Browser.

> **Status:** Lern- und Demonstrationsprojekt, kein zertifiziertes
> Betriebssystem. Die Algorithmen orientieren sich aber an echten
> Flugsicherungsverfahren (EUROCONTROL-/ASTERIX-Standards).

---

## Loslegen — in 2 Minuten zur Live-Karte

Es gibt zwei Wege, Firefly zu starten. Beide enden am selben Ort: einem
Browser-Fenster mit einer 2D-Karte, auf der Flugzeuge live als Punkte mit
Geschwindigkeitspfeil über den Bildschirm wandern.

### Weg A — mit Docker (empfohlen, keine Rust-Installation nötig)

Voraussetzung: [Docker](https://www.docker.com/) ist installiert.

```bash
docker-compose up
```

Dann im Browser öffnen: **http://localhost:8080**

### Weg B — lokal mit Rust

Voraussetzung: ein aktueller [Rust-Toolchain](https://www.rust-lang.org/) ist
installiert.

```bash
cargo run -p firefly-server
```

Dann im Browser öffnen: **http://localhost:8080**

---

## Was du im Browser siehst

Nach dem Start zeigt die Karte eine **Demo-Szene**: zwei Flugzeuge, die unter
einem einzelnen Radar fliegen. Jedes Flugzeug erscheint als Punkt mit:

- einem **Pfeil** (Richtung und Geschwindigkeit über Grund),
- einer **Farbe**, die den Status anzeigt (bestätigter Track = grün,
  noch unbestätigt/„tentativ" = gelb, „coasting" = grau — siehe
  [Glossar](docs/glossary.md)),
- einem **gestrichelten Unsicherheits-Ring**, wenn der Track gerade
  „coastet" (extrapoliert wird, weil grade keine frische Messung da war).

### Bedienelemente im HUD (oben rechts)

| Knopf | Was er tut |
|-------|-----------|
| **„Verzug simulieren (5 s)"** | Simuliert eine 5 Sekunden lange Zustellverzögerung — die Berechnung läuft unverändert weiter, nur die Anzeige hängt kurz. Zeigt: Firefly ist *robust* gegenüber holpriger Netzwerkzustellung (siehe [M3 — Vom Tracker zum Live-Lagebild](docs/milestones/M3-live-picture.md)). |
| **„airspaces"** | Blendet eine Luftraum-Übersicht (TMA, CTR, Sperrgebiet) als violette Flächen über die Karte. Beispieldaten rund um Frankfurt (`crates/firefly-server/static/airspaces.geojson`). |
| **„raw plots"** | Blendet die **rohen Radar-Messungen** ein (kleine rote Punkte) — das, was der Tracker als *Input* bekommt, bevor daraus glatte Tracks werden. Gut zum Vergleich: „was misst das Radar" vs. „was berechnet der Tracker". |

### Die Frankfurt-Showcase-Szene

Die Demo-Szene ist bewusst klein und einfach. Für ein realistischeres Bild —
**drei Radarstandorte, acht Flugzeuge**, mit überlappenden Reichweiten,
Kurven, Warteschleife und einem Flugzeug ohne Transponder — gibt es die
**Frankfurt-Szene**. Sie läuft 240 Sekunden lang und zeigt typische
Situationen, für die der Tracker extra Mechanik braucht:

- zwei parallele Anflüge mit nur ~150 m Abstand (JPDA — die Tracks dürfen
  sich nicht vertauschen oder verschmelzen),
- ein Abflug mit Kurve (IMM — Manöver-Erkennung),
- ein Flugzeug ohne SSR-Transponder (zeigt nur als Roh-Plot, nie als
  identifizierter Track),
- ein Nordanflug, der mitten im Flug von einem zweiten Radar übernommen wird
  (Multi-Radar-Handover).

**Mit Docker:**

```bash
FIREFLY_SCENE=frankfurt docker-compose up
```

**Lokal:**

```bash
FIREFLY_SCENE=frankfurt cargo run -p firefly-server
```

Tipp: Schalte „raw plots" ein und beobachte das primary-only-Flugzeug — du
siehst die roten Mess-Punkte, aber nie einen grünen Track mit Identität dafür.

---

## Was Firefly tut (Verarbeitungskette)

Von der Radarmessung zum Track:

1. **Sensor-/Messmodell** — PSR liefert Entfernung + Azimut (keine Identität,
   keine Höhe aus Mode C); SSR liefert zusätzlich Mode 3/A (Squawk), Mode C
   (Flughöhe) und Mode S (ICAO-Adresse). Messungen sind polar und
   sensor-bezogen.
2. **Track-Initiierung** — aus nicht zugeordneten Plots entstehen vorläufige
   Tracks.
3. **Prädiktion** — Bewegungsmodelle: konstante Geschwindigkeit,
   koordinierte Kurve, kombiniert über IMM für manövrierende Ziele.
4. **Gating & Datenassoziation** — Plots werden Tracks zugeordnet
   (Validierungstor, JPDA bei dichtem Verkehr).
5. **Filterung** — Kalman-Filter-Zustandsschätzung.
6. **Track-Pflege** — Bestätigung, Coasting bei Fehltreffern, Löschung.
7. **Multi-Radar-Fusion** — mehrere Radare tragen zu einem gemeinsamen
   Lagebild bei (zentrale Mess-Fusion).
8. **Web-Anzeige** — Live-2D-Karte über WebSocket, inkl. Roh-Plots und
   Luftraum-Overlay.

## Architektur

Ein Rust-Workspace (Rechen-Kern + Server) plus ein JavaScript/MapLibre-Frontend.
Aus-/Eingabe orientiert sich am **ASTERIX**-Format echter Radarsysteme
(CAT048 Mono-Radar-Zielmeldungen, CAT062 System-Tracks).

```
firefly-geo        Geodäsie: WGS84 ↔ ECEF ↔ lokales ENU ↔ polar, Projektionen
firefly-core       Gemeinsame Domänentypen: Plots, Sensoren, Zeit, Identitäten
firefly-sim        Szenario- + Radar-Plot-Simulator (M1)
firefly-track      Gating + Assoziation (GNN/JPDA) + Kalman/IMM + Track-Lebenszyklus (M2, M5)
firefly-asterix    ASTERIX CAT062 Encoder/Decoder (M3.X, Häppchen C/D)
firefly-io         Neutrales Ausgabe-Format „Frame" (Zeit + Tracks + Roh-Plots, JSON) (M3, M6.3)
firefly-player     Szenario → Tracker → Frame-Strom, deterministisch (M3)
firefly-multicast  UDP-Multicast-Versand/-Empfang von CAT062 (ADR 0006)
firefly-server     axum-WebSocket-Server + eingebettetes Web-Frontend (M3, M6)
```

## Bauen & Testen

```bash
cargo test --workspace          # alle Tests
cargo run -p firefly-server     # Server starten (siehe „Loslegen")
cargo run --example demo -p firefly-sim   # nur den M1-Simulator sehen
```

## Container & Cloud-Deployment

Details zu Docker, docker-compose und Cloud-Deployment (Kubernetes, Cloud
Run, ECS) stehen in [DOCKER.md](DOCKER.md).

## Zusammen mit Wayfinder testen (End-to-End-ASD)

Firefly sendet seinen Live-Track-Strom als ASTERIX CAT062 über UDP-Multicast
(ADR 0006) — das **Wayfinder**-Projekt (eigenes Repo) ist der produktive ASD-
Konsument dafür. Standardmäßig sendet Firefly keinen Multicast (kein
überraschender Netzwerkverkehr bei `cargo run`). Empfohlen für den
End-to-End-Test ist das **Frankfurt-Szenario** (siehe oben, drei Radare, acht
Flugzeuge) mit aktiviertem CAT062-Feed:

```bash
FIREFLY_SCENE=frankfurt FIREFLY_CAT062_ENABLED=true cargo run -p firefly-server
# oder mit Docker:
FIREFLY_SCENE=frankfurt FIREFLY_CAT062_ENABLED=true docker-compose up
```

Mit Wayfinder parallel gestartet (siehe dessen README) erscheinen die Tracks
live auf Wayfinders Karte. Details zum Wire-Vertrag stehen in
[docs/ICD-CAT062.md](docs/ICD-CAT062.md).

> Auf **macOS/Windows (Docker Desktop)**: Zwei separat gestartete
> `docker-compose up`-Stacks sehen sich wegen `network_mode: host` nicht.
> Wayfinders `DOCKER.md` beschreibt dafür eine Bridge-Netzwerk-Variante mit
> gemeinsamem Master-Compose (beide Repos als Geschwister-Ordner).

## Mehr erfahren

- [docs/README.md](docs/README.md) — Dokumentations-Wegweiser (Glossar,
  Meilenstein-Erklärungen, Architektur-Entscheidungen, Anforderungs-Register).
- [docs/STATUS.md](docs/STATUS.md) — aktueller Arbeitsstand & nächste Schritte.
- [CLAUDE.md](CLAUDE.md) — Arbeitsregeln dieses Projekts.

## Lizenz

MIT
