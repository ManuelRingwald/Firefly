# Firefly — Technisches Handbuch (Betriebsführung)

> **Zielgruppe:** Systembetreiber und Entwickler, die Firefly im laufenden
> Betrieb überwachen, konfigurieren und debuggen.
> Vorausgesetzt wird ein laufendes System (siehe `docs/INSTALLATION.md`).

---

## 1. Alle Umgebungsvariablen

### 1.1 Server-Grundkonfiguration

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_PORT` | u16 | `8080` | TCP-Port des HTTP/WebSocket-Servers |
| `FIREFLY_MODE` | string | `replay` | Betriebsmodus: `replay` (vorberechnete Szene) oder `live` (Echtzeit-ADS-B via OpenSky). `live` impliziert OpenSky als Plot-Quelle. |
| `FIREFLY_SCENE` | string | `demo` | Szene (nur Replay-Modus): `demo` (2 Flugzeuge, 1 Radar) oder `frankfurt` (8 Flugzeuge, 3 Radare) |
| `FIREFLY_SPEED` | f64 | `1.0` | Wiedergabe-Geschwindigkeit (nur Replay-Modus). `2.0` = doppelte Geschwindigkeit. Muss positiv und endlich sein. |

### 1.2 CAT062-Multicast-Feed (Wayfinder-Schnittstelle)

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_CAT062_ENABLED` | bool | `false` | Feed aktivieren (`true`/`1`/`yes`). Deaktiviert → kein UDP-Verkehr |
| `FIREFLY_CAT062_GROUP` | IPv4 | `239.255.0.62` | Multicast-Gruppe (muss im Bereich 224.0.0.0–239.255.255.255 liegen) |
| `FIREFLY_CAT062_PORT` | u16 | `8600` | UDP-Port |
| `FIREFLY_CAT062_SAC` | u8 | `25` | System Area Code (I062/010) |
| `FIREFLY_CAT062_SIC` | u8 | `2` | System Identification Code (I062/010) |
| `FIREFLY_CAT062_REF_LAT` | f64 | `48.0` | Referenzpunkt-Breitengrad für I062/100 (stereografisch) |
| `FIREFLY_CAT062_REF_LON` | f64 | `11.0` | Referenzpunkt-Längengrad für I062/100 |

### 1.3 CAT065-Heartbeat (Feed-Liveness)

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_CAT065_ENABLED` | bool | `true` | Heartbeat aktivieren (nur wirksam wenn CAT062 auch aktiv). Ermöglicht Wayfinder, leeren Himmel von totem Feed zu unterscheiden |
| `FIREFLY_CAT065_PERIOD` | f64 | `1.0` | Heartbeat-Intervall in Wanduhrsekunden |
| `FIREFLY_CAT065_SERVICE_ID` | u8 | `1` | Service-ID in I065/015 |

### 1.4 OpenSky Network ADS-B-Adapter

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_OPENSKY_ENABLED` | bool | `false` | Adapter aktivieren. Deaktiviert → kein ausgehender HTTP-Verkehr |
| `FIREFLY_OPENSKY_LAT_MIN` | f64 | `47.0` | Südliche Begrenzung der Bounding Box (Grad) |
| `FIREFLY_OPENSKY_LAT_MAX` | f64 | `55.0` | Nördliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_LON_MIN` | f64 | `5.0` | Westliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_LON_MAX` | f64 | `16.0` | Östliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_POLL_INTERVAL_SECS` | u64 | `10` | Abfrageintervall in Sekunden (≥ 10 ohne Account, ≥ 5 mit Account) |
| `FIREFLY_OPENSKY_USERNAME` | string | — | HTTP-Basic-Auth Benutzername (optional) |
| `FIREFLY_OPENSKY_PASSWORD` | string | — | HTTP-Basic-Auth Passwort (optional) |
| `FIREFLY_OPENSKY_SENSOR_ID` | u16 | `200` | Sensor-ID, die ADS-B-Plots im Tracker zugeordnet werden |

### 1.5 Logging

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `RUST_LOG` | string | `info` | Log-Verbosity. Formate: `debug`, `info`, `warn`, `error`, `firefly_server=debug,info` |

---

## 2. Log-Inspektion

### 2.1 Log-Format

Firefly schreibt strukturierte Logs im **JSON-Format** (via `tracing`). Jede
Zeile ist ein eigenständiges JSON-Objekt:

```json
{"timestamp":"2026-06-18T10:23:01.442Z","level":"INFO","target":"firefly_server","message":"starting Firefly server","port":8080,"speed":1.0,"scene":"Frankfurt","frames":312}
```

### 2.2 Verbosity steuern

```bash
# Nur Fehler und Warnungen:
RUST_LOG=warn ./target/release/firefly-server

# Debug-Ausgaben für den OpenSky-Adapter:
RUST_LOG=firefly_opensky=debug,info ./target/release/firefly-server

# Alles auf debug:
RUST_LOG=debug ./target/release/firefly-server
```

### 2.3 Wichtige Log-Nachrichten

| Nachricht | Bedeutung |
|-----------|-----------|
| `starting Firefly server` | Server startet; zeigt Port, Speed, Scene, Frame-Anzahl |
| `listening; open http://{addr} in a browser` | Server ist bereit |
| `CAT062 multicast feed enabled destination=...` | Multicast-Feed aktiv; zeigt Ziel-Adresse |
| `CAT062 multicast feed disabled` | Feed aus (Normal bei `FIREFLY_CAT062_ENABLED` nicht gesetzt) |
| `CAT065 heartbeat enabled destination=... period_s=1` | Heartbeat aktiv |
| `OpenSky ADS-B poller enabled lat_min=... lat_max=...` | ADS-B-Adapter läuft |
| `OpenSky ADS-B poller disabled` | Adapter aus (Normal-Zustand) |
| `OpenSky plots received count=42` | Erfolgreiche ADS-B-Abfrage |
| `shutdown signal received` | Graceful-Shutdown eingeleitet |
| `shutdown complete` | Server sauber beendet |

### 2.4 Logs mit `jq` filtern (Empfehlung)

```bash
# Nur Fehlermeldungen:
./target/release/firefly-server 2>&1 | jq 'select(.level == "ERROR")'

# OpenSky-Abfragen zählen:
./target/release/firefly-server 2>&1 | jq 'select(.message | contains("OpenSky plots"))'

# Track-Anzahl über Zeit:
./target/release/firefly-server 2>&1 | jq 'select(.message | contains("CAT062")) | .scans'
```

---

## 3. Prometheus-Metriken

### 3.1 Endpunkt

```
GET http://localhost:8080/metrics
Content-Type: text/plain; version=0.0.4
```

### 3.2 Verfügbare Metriken

| Metrik | Typ | Bedeutung |
|--------|-----|-----------|
| `firefly_scene_frames_total` | gauge | Anzahl Frames in der aktuell geladenen Szene |
| `firefly_ws_clients_connected` | gauge | Aktuell verbundene WebSocket-Clients |
| `firefly_ws_clients_total` | counter | Gesamt-WebSocket-Verbindungen seit Start |
| `firefly_cat062_scans_sent_total` | counter | Gesendete CAT062-Datenblöcke (Scans) |
| `firefly_cat062_send_errors_total` | counter | Fehlgeschlagene CAT062-Sends |
| `firefly_cat065_heartbeats_sent_total` | counter | Gesendete CAT065-Heartbeats |
| `firefly_tracks_active` | gauge | Tracks im zuletzt gesendeten CAT062-Scan |

### 3.3 Prometheus scrape-Konfiguration

```yaml
# prometheus.yml (Ausschnitt)
scrape_configs:
  - job_name: firefly
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: /metrics
    scrape_interval: 10s
```

### 3.4 Nützliche PromQL-Abfragen

```promql
# Tracks pro Sekunde (Rate):
rate(firefly_cat062_scans_sent_total[1m])

# Fehlerrate des CAT062-Feeds:
rate(firefly_cat062_send_errors_total[5m])

# Aktuelle Track-Anzahl:
firefly_tracks_active
```

---

## 4. Health- und Readiness-Probes

### 4.1 Liveness-Probe (`/health`)

Prüft, ob der HTTP-Server antwortet:

```bash
curl http://localhost:8080/health
# → {"status":"ok"}
```

Kubernetes-Konfiguration:

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 10
```

### 4.2 Readiness-Probe (`/ready`)

Prüft, ob der Server Traffic verarbeiten kann:

```bash
curl http://localhost:8080/ready
# → {"status":"ready"}
```

Kubernetes-Konfiguration:

```yaml
readinessProbe:
  httpGet:
    path: /ready
    port: 8080
  initialDelaySeconds: 3
  periodSeconds: 5
```

---

## 5. Track-Aufzeichnung (`.ffrec`)

### 5.1 Was ist `.ffrec`?

Das `.ffrec`-Format (**Firefly Recording**) speichert den vollständigen
Simulator-Output (Scans mit SystemTracks) für deterministische Replay-Tests.

Aufzeichnungen entstehen automatisch bei Nutzung des `firefly-recorder`-Crates
(konfigurierbar). Das Format ist JSON-Lines (ein `Frame` pro Zeile).

### 5.2 Replay einer Aufzeichnung

```bash
# Aufzeichnung abspielen (Beispiel, nach AP9.4c-5):
firefly-replay-plots --input session.ffrec
```

> **Hinweis:** Das Replay-Binary (`firefly-replay-plots`) wird in AP9.4c-5
> implementiert. Bis dahin sind Aufzeichnungen über den `firefly-player`-Crate
> nutzbar (programmatisch).

---

## 6. Plot-Aufzeichnung für Live-Betrieb (`.ffplots`)

### 6.1 Was ist `.ffplots`?

Das `.ffplots`-Format speichert jeden eingehenden ADS-B-Plot mit
Wall-Clock-Zeitstempel (Unix-Nanosekunden). Zweck: Der nicht-deterministisch
ankommende Live-Datenstrom wird für deterministisches Replay aufgezeichnet
(ADR 0020 — „Non-deterministic arrival ≠ non-reproducible").

Format: JSON-Lines. Jede Zeile:
```json
{"ts_unix_ns":1750243381000000000,"plot":{...}}
```

### 6.2 PlotRecorder-Pfad konfigurieren

Im Live-Modus (AP9.4c-3, geplant) wird der Recorder über eine
Umgebungsvariable aktiviert. Bis dahin ist er programmatisch konfigurierbar:

```rust
let recorder = PlotRecorder::create("/var/log/firefly/session.ffplots")?;
```

### 6.3 Wichtig: Float-Genauigkeit

`.ffplots`-Dateien nutzen `serde_json` mit `float_roundtrip`-Feature, das
`f64`-Werte bit-exakt rund-reist (Standard-serde_json ist nur näherungsweise).
Ohne dieses Feature würde ein Replay leicht abweichen. (Aktiviert
workspace-weit in `Cargo.toml`.)

---

## 7. Graceful Shutdown

Firefly beendet sich sauber auf:

- **Ctrl-C** (SIGINT)
- **SIGTERM** (von Kubernetes, `docker stop`, systemd)

Der laufende HTTP-Server wartet auf den Abschluss offener Anfragen, bevor der
Prozess endet. Im Log erscheint:

```
INFO shutdown signal received
INFO shutdown complete
```

Kubernetes-Empfehlung: `terminationGracePeriodSeconds: 30` im Pod-Spec.

---

## 8. Betriebsmodus: Replay vs. Live (ab AP9.4c-3)

| Modus | Beschreibung | Aktivierung |
|-------|--------------|-------------|
| **Replay** (Standard) | Vorberechnete Szene (`demo` oder `frankfurt`) in Wanduhrzeit abgespielt | kein `FIREFLY_OPENSKY_ENABLED` |
| **Live** (geplant) | OpenSky ADS-B-Daten, Tracker läuft in Echtzeit | `FIREFLY_OPENSKY_ENABLED=true` + `FIREFLY_MODE=live` |

> **Stand AP9.4c-2:** Der Live-Tracker-Kern (`LiveTracker`, `run_live_tracker`,
> `PlotRecorder`) ist implementiert. Die Integration in den Server-Hauptpfad
> (AP9.4c-3) ist noch ausstehend. OpenSky-Plots werden bereits abgerufen und
> geloggt.

---

## 9. CAT062-Strom verifizieren (Wireshark)

Zum Verifizieren des Ausgabestroms auf dem Netz:

```
# Wireshark Filter:
udp.dstport == 8600

# ASTERIX Plugin: in Wireshark unter Analyze → Decode As → ASTERIX auswählen,
# oder manuell: Payload enthält 0x3E (CAT062) oder 0x41 (CAT065)
# als erstes Byte, gefolgt von 2 Byte Länge (Big Endian).
```

Mit `tcpdump` auf der Konsole:

```bash
sudo tcpdump -i lo udp port 8600 -X
```

---

## 10. Kubernetes-Deployment (Kurzreferenz)

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: firefly-server
spec:
  replicas: 1
  selector:
    matchLabels:
      app: firefly-server
  template:
    metadata:
      labels:
        app: firefly-server
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      containers:
        - name: firefly-server
          image: firefly-server:latest
          ports:
            - containerPort: 8080
          env:
            - name: FIREFLY_SCENE
              value: "frankfurt"
            - name: FIREFLY_CAT062_ENABLED
              value: "true"
            - name: FIREFLY_OPENSKY_ENABLED
              value: "true"
            - name: FIREFLY_OPENSKY_USERNAME
              valueFrom:
                secretKeyRef:
                  name: opensky-credentials
                  key: username
            - name: FIREFLY_OPENSKY_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: opensky-credentials
                  key: password
            - name: RUST_LOG
              value: "info"
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /ready
              port: 8080
            initialDelaySeconds: 3
            periodSeconds: 5
          resources:
            requests:
              cpu: "100m"
              memory: "64Mi"
            limits:
              cpu: "500m"
              memory: "256Mi"
      terminationGracePeriodSeconds: 30
```

---

## 11. Bekannte Einschränkungen (Stand 2026-06-18)

| Einschränkung | ADR / Issue | Geplante Lösung |
|---------------|-------------|-----------------|
| Live-Tracker noch nicht an Server-Hauptpfad angeschlossen | ADR 0020, AP9.4c-3 | Nächster Implementierungsschritt |
| CAT062-Referenzpunkt fest (Frankfurt-Demo-Ursprung) | ADR 0006 | Konfigurierbarer System-Referenzpunkt |
| Multicast ohne Authentifizierung | ADR 0017 | Netz-Isolation + anwendungsseitige Absicherung |
| OpenSky-Passwort nur via Env-Variable | ADR 0003 | Kubernetes Secret (bereits empfohlen) |

---

## Weiterführend

- **Installationshandbuch** (`docs/INSTALLATION.md`): Erstinbetriebnahme.
- **ICD CAT062** (`docs/ICD-CAT062.md`): Byte-genauer Draht-Vertrag mit Wayfinder.
- **ADR-Verzeichnis** (`docs/decisions/`): Alle Architekturentscheide.
- **Anforderungsregister** (`docs/requirements/README.md`): Rückverfolgbarkeit.
