# Firefly — Installationshandbuch

> **Zielgruppe:** Systembetreiber, die Firefly erstmalig einrichten.
> Dieses Handbuch deckt den Weg von Null bis zum laufenden System ab —
> Quellcode-Build, Docker-Betrieb, Frankfurt-Szenario und ADS-B-Echtbetrieb.

---

## 1. Voraussetzungen

### Build aus dem Quellcode

| Werkzeug | Mindestversion | Prüfen |
|----------|----------------|--------|
| Rust Toolchain | 1.82 | `rustc --version` |
| Cargo | kommt mit Rust | `cargo --version` |
| Git | beliebig aktuell | `git --version` |

```bash
# Rust installieren (falls noch nicht vorhanden):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### Docker-Betrieb (optional)

| Werkzeug | Mindestversion | Prüfen |
|----------|----------------|--------|
| Docker Engine | 24.x | `docker --version` |
| Docker Compose | 2.x (optional) | `docker compose version` |

### Netzwerk

- UDP-Multicast erfordert, dass das Host-Netz Multicast-Pakete weiterleitet.
  Auf `localhost` (Loopback) funktioniert Multicast ohne besondere Konfiguration.
- Für den ADS-B-Echtbetrieb (OpenSky) wird ausgehender HTTPS-Zugang
  (Port 443) zu `opensky-network.org` benötigt.

---

## 2. Quellcode beschaffen

```bash
git clone https://github.com/manuelringwald/firefly.git
cd firefly
```

---

## 3. Build aus dem Quellcode

```bash
# Alle Tests ausführen (Qualitäts-Gate):
cargo test --workspace

# Release-Binary für den Server bauen:
cargo build --release -p firefly-server

# Binary liegt danach unter:
./target/release/firefly-server
```

---

## 4. Schnellstart: Demo-Modus (ohne Netz)

Der einfachste Einstieg — zwei simulierte Flugzeuge, ein Radar, keine externe
Abhängigkeit.

```bash
cargo run --release -p firefly-server
# oder nach dem Build direkt:
./target/release/firefly-server
```

Firefly startet auf Port **8080**. Im Browser öffnen:

```
http://localhost:8080
```

Die Karte zeigt zwei sich bewegende Tracks (Demo-Szenario).
Die Konsole gibt strukturierte JSON-Logs aus (Verbosity: `info`).

---

## 5. Frankfurt-Szenario einrichten

Das Frankfurt-Szenario zeigt drei Radare und acht Luftfahrzeuge im Raum
Frankfurt/Main — deutlich realistischer als der Demo-Modus.

```bash
FIREFLY_SCENE=frankfurt cargo run --release -p firefly-server
```

Oder als separate Env-Variable beim bereits gebauten Binary:

```bash
FIREFLY_SCENE=frankfurt ./target/release/firefly-server
```

Wahlweise die Wiedergabegeschwindigkeit erhöhen (4× Echtzeit):

```bash
FIREFLY_SCENE=frankfurt FIREFLY_SPEED=4.0 ./target/release/firefly-server
```

---

## 6. Docker-Betrieb

### Image bauen

```bash
docker build -t firefly-server:latest .
```

### Demo starten

```bash
docker run --rm -p 8080:8080 firefly-server:latest
```

### Frankfurt-Szenario im Container

```bash
docker run --rm \
  -p 8080:8080 \
  -e FIREFLY_SCENE=frankfurt \
  -e FIREFLY_SPEED=1.0 \
  firefly-server:latest
```

### Mit CAT062-Multicast-Feed (für Wayfinder)

Der Multicast-Feed ist standardmäßig deaktiviert (kein unbeabsichtigter
UDP-Verkehr). Für den Verbund mit Wayfinder:

```bash
docker run --rm \
  -p 8080:8080 \
  --network host \                         # Multicast benötigt Host-Netz
  -e FIREFLY_SCENE=frankfurt \
  -e FIREFLY_CAT062_ENABLED=true \
  -e FIREFLY_CAT062_GROUP=239.255.0.62 \
  -e FIREFLY_CAT062_PORT=8600 \
  firefly-server:latest
```

> **Hinweis `--network host`:** Docker-NAT blockiert UDP-Multicast.
> Auf Linux ist `--network host` die einfachste Lösung für den Einzelrechner-Betrieb.
> In Kubernetes wird Multicast über einen dedizierten Netz-Plugin konfiguriert.

---

## 7. ADS-B-Echtbetrieb mit OpenSky Network (`FIREFLY_MODE=live`)

Im **Live-Modus** bezieht Firefly Echtzeit-ADS-B-Positionen vom OpenSky Network
REST API, speist sie direkt in den Tracker und sendet kontinuierlich CAT062-
und WebSocket-Daten an Wayfinder bzw. den Browser. Alle eingehenden Plots werden
parallel in einer `.ffplots`-Datei aufgezeichnet (ADR 0020).

> **Wichtig:** Der Live-Modus wird ausschließlich über `FIREFLY_MODE=live`
> aktiviert — nicht über `FIREFLY_OPENSKY_ENABLED`. Letztere Variable aktiviert
> den OpenSky-Poller nur als **Log-only**-Sonde im Replay-Modus (Plots werden
> geloggt, aber nicht verarbeitet).

### Schritt 1: OpenSky-Account anlegen (optional, empfohlen)

Anonymer Zugang ist möglich, aber auf **1 Anfrage / 10 Sekunden** gedrosselt.
Mit einem kostenlosen OpenSky-Account sinkt die Mindest-Wartezeit auf **5 Sekunden**.

Registrierung: https://opensky-network.org/index.php?option=com_users&view=registration

### Schritt 2: Bounding Box konfigurieren

Die Bounding Box legt fest, welche geographische Region abgefragt wird.
Standardwert: Deutschland (47°N–55°N, 5°O–16°O).

Für den Raum Frankfurt (enger Ausschnitt):

```bash
export FIREFLY_OPENSKY_LAT_MIN=49.0
export FIREFLY_OPENSKY_LAT_MAX=51.0
export FIREFLY_OPENSKY_LON_MIN=7.0
export FIREFLY_OPENSKY_LON_MAX=10.0
```

### Schritt 3: Authentifizierung (falls OpenSky-Account vorhanden)

```bash
export FIREFLY_OPENSKY_USERNAME=mein_benutzer
export FIREFLY_OPENSKY_PASSWORD=mein_passwort
export FIREFLY_OPENSKY_POLL_INTERVAL_SECS=5   # mit Account: 5 s möglich
```

> **Sicherheitshinweis:** Passwörter nie direkt in Shell-Skripten hartkodieren.
> Im Produktionsbetrieb über Kubernetes Secrets oder ein Vault-System injizieren.

### Schritt 4: Im Live-Modus starten

```bash
FIREFLY_MODE=live \
FIREFLY_OPENSKY_LAT_MIN=49.0 \
FIREFLY_OPENSKY_LAT_MAX=51.0 \
FIREFLY_OPENSKY_LON_MIN=7.0 \
FIREFLY_OPENSKY_LON_MAX=10.0 \
./target/release/firefly-server
```

Im Log erscheinen dann Meldungen wie:

```
INFO firefly starting mode=live
INFO OpenSky ADS-B poller started lat_min=49 lat_max=51 lon_min=7 lon_max=10
INFO live tracker tick tracks=12 plots_ingested=47
```

Die Readiness-Probe (`/ready`) gibt `503` zurück, bis der erste OpenSky-Poll
erfolgreich war — danach `200 ready`. Wayfinder zeigt Tracks sobald `/ready`
positiv antwortet.

---

## 8. CAT062-Multicast-Feed für Wayfinder aktivieren

Damit Wayfinder die Tracks empfangen kann, müssen CAT062 (Tracks) und CAT065
(Heartbeat) aktiviert sein:

```bash
export FIREFLY_CAT062_ENABLED=true
export FIREFLY_CAT062_GROUP=239.255.0.62   # Default
export FIREFLY_CAT062_PORT=8600             # Default
export FIREFLY_CAT065_ENABLED=true          # Default: an (wenn CAT062 an)
```

Wayfinder muss derselben Multicast-Gruppe beitreten:

```bash
# Wayfinder-Seite (Beispiel):
export WAYFINDER_MULTICAST_GROUP=239.255.0.62
export WAYFINDER_MULTICAST_PORT=8600
```

---

## 9. Health-Check und Readiness verifizieren

Nach dem Start stehen folgende Endpunkte zur Verfügung:

```bash
# Liveness: Server antwortet?
curl http://localhost:8080/health
# → {"status":"ok"}

# Readiness: Server bereit für Traffic?
curl http://localhost:8080/ready
# → {"status":"ready"}  oder  {"status":"not ready","reason":"..."}

# Prometheus-Metriken:
curl http://localhost:8080/metrics
```

---

## 10. Vollständiges Beispiel: Frankfurt + ADS-B + Wayfinder

```bash
# Alle Optionen kombiniert (Produktionsnahes Demo):
FIREFLY_SCENE=frankfurt \
FIREFLY_SPEED=1.0 \
FIREFLY_PORT=8080 \
FIREFLY_CAT062_ENABLED=true \
FIREFLY_CAT062_GROUP=239.255.0.62 \
FIREFLY_CAT062_PORT=8600 \
FIREFLY_CAT065_ENABLED=true \
FIREFLY_OPENSKY_ENABLED=true \
FIREFLY_OPENSKY_USERNAME=mein_benutzer \
FIREFLY_OPENSKY_PASSWORD=mein_passwort \
FIREFLY_OPENSKY_LAT_MIN=49.0 \
FIREFLY_OPENSKY_LAT_MAX=51.0 \
FIREFLY_OPENSKY_LON_MIN=7.0 \
FIREFLY_OPENSKY_LON_MAX=10.0 \
RUST_LOG=info \
./target/release/firefly-server
```

---

## 11. Fehlerdiagnose beim Start

| Symptom | Wahrscheinliche Ursache | Lösung |
|---------|-------------------------|--------|
| `failed to bind` auf Port 8080 | Port belegt | `FIREFLY_PORT=9090` setzen oder belegenden Prozess beenden |
| `failed to open CAT062 multicast socket` | Netz-Rechte oder falsches Interface | Auf Linux: `sudo setcap cap_net_admin+ep firefly-server` oder Root-Rechte prüfen |
| `OpenSky plots received count=0` dauerhaft | Bounding Box enthält keine Luftfahrzeuge oder API-Limit | Box vergrößern; Zeitpunkt prüfen (wenig Verkehr nachts); Account für mehr Rate |
| Browser zeigt keine Karte | Statische Assets fehlen | Binary muss mit `static/`-Ordner im selben Verzeichnis liegen; bei Docker: prüfen ob `COPY static /app/static/` korrekt ausgeführt wurde |

---

## Weiterführend

- **Technisches Handbuch** (`docs/TECHNICAL.md`): alle Umgebungsvariablen,
  Log-Inspektion, Prometheus-Metriken, Aufzeichnung und Replay.
- **ICD CAT062** (`docs/ICD-CAT062.md`): der Draht-Vertrag mit Wayfinder.
- **ADR 0019** (`docs/decisions/0019-adsb-opensky.md`): Architekturentscheid ADS-B.
- **ADR 0020** (`docs/decisions/0020-live-tracker-modus-fuer-echtzeit-adsb.md`): Live-Tracker-Architektur.
