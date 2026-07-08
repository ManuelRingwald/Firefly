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

Die eingebaute Karte (http://localhost:8080) ist Fireflys **Diagnose-Sicht**
auf den laufenden Tracker. Was sie zeigt, hängt von den konfigurierten
**Quellen** ab (ADR 0023/0030): ohne Quelle bleibt der Himmel **leer** — das
ist kein Fehler, sondern der ehrliche Zustand (der CAT065-Heartbeat läuft
trotzdem). Mit einer Quelle (z. B. OpenSky-ADS-B, siehe unten) erscheint jedes
Luftfahrzeug als Punkt mit:

- einem **Pfeil** (Richtung und Geschwindigkeit über Grund),
- einer **Farbe**, die den Status anzeigt (bestätigter Track = grün,
  noch unbestätigt/„tentativ" = gelb, „coasting" = grau — siehe
  [Glossar](docs/glossary.md)),
- einem **gestrichelten Unsicherheits-Ring**, wenn der Track gerade
  „coastet" (extrapoliert wird, weil grade keine frische Messung da war).

### Bedienelemente im HUD (oben rechts)

| Knopf | Was er tut |
|-------|-----------|
| **„airspaces"** | Blendet eine Luftraum-Übersicht (TMA, CTR, Sperrgebiet) als violette Flächen über die Karte. Beispieldaten rund um Frankfurt (`crates/firefly-server/static/airspaces.geojson`). |
| **„raw plots"** | Blendet die **rohen Messungen** ein (kleine rote Punkte) — das, was der Tracker als *Input* bekommt, bevor daraus glatte Tracks werden. Gut zum Vergleich: „was misst der Sensor" vs. „was berechnet der Tracker". |

### Quellen konfigurieren (Beispiel: OpenSky-ADS-B)

Der Tracker wird von **Live-Quellen** gespeist — orchestriert über
`FIREFLY_SOURCES` (JSON-Kontrakt, ADR 0023) oder standalone über die
Adapter-Envs. Alle Adapter sind **Opt-in** (kein Überraschungs-Egress beim
nackten Start):

```bash
FIREFLY_OPENSKY_ENABLED=true \
FIREFLY_OPENSKY_LAT_MIN=49.0 FIREFLY_OPENSKY_LAT_MAX=51.0 \
FIREFLY_OPENSKY_LON_MIN=7.0  FIREFLY_OPENSKY_LON_MAX=10.0 \
FIREFLY_OPENSKY_CLIENT_ID="client_id" FIREFLY_OPENSKY_CLIENT_SECRET="client_secret" \
cargo run -p firefly-server
```

(OpenSky-Konto: kostenlos; OAuth2-Client-Credentials, ADR 0024. Weitere
Adapter: FLARM/OGN `FIREFLY_FLARM_*` (ADR 0026), Radar-ASTERIX
`FIREFLY_RADAR_*` (ADR 0028). Die frühere eingebaute Demo-Szene wurde
entfernt — ADR 0030.)

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
überraschender Netzwerkverkehr bei `cargo run`). Der empfohlene Weg für den
End-to-End-Test ist der **orchestrierte Wayfinder-Stack** (Wayfinders
`docker-compose.orchestrated.yml` bzw. Codespace, siehe Wayfinders
`docs/CODESPACES.md`): dort spawnt der Orchestrator je Feed automatisch eine
Firefly-Instanz mit den im Admin-UI konfigurierten Quellen.

Standalone (Firefly allein, mit aktiviertem CAT062-Feed und einer Quelle):

```bash
FIREFLY_CAT062_ENABLED=true \
FIREFLY_OPENSKY_ENABLED=true FIREFLY_OPENSKY_CLIENT_ID="id" FIREFLY_OPENSKY_CLIENT_SECRET="secret" \
cargo run -p firefly-server
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

Apache-2.0
