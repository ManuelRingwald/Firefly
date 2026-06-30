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
| `FIREFLY_SYSTEM_REF_LAT` | f64 | Bbox-Mitte¹ | **Nur Live-Modus.** Breitengrad des System-Referenzpunkts (ADR 0021) — speist Tracking-Frame **und** I062/100-Projektion |
| `FIREFLY_SYSTEM_REF_LON` | f64 | Bbox-Mitte¹ | **Nur Live-Modus.** Längengrad des System-Referenzpunkts |

¹ Default = Mitte der OpenSky-Bounding-Box (`FIREFLY_OPENSKY_LAT/LON_*`). Im
**Replay-Modus** ist der System-Referenzpunkt fest der Szenen-Ursprung
(Demo: 48/11, Frankfurt: 50,04/8,56) und **nicht** über Env überschreibbar —
so bleibt I062/100 kohärent mit der Szene (ADR 0021). I062/105 (WGS84) ist davon
unabhängig und immer absolut.

### 1.3 CAT065-Heartbeat (Feed-Liveness)

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_CAT065_ENABLED` | bool | `true` | Heartbeat aktivieren (nur wirksam wenn CAT062 auch aktiv). Ermöglicht Wayfinder, leeren Himmel von totem Feed zu unterscheiden |
| `FIREFLY_CAT065_PERIOD` | f64 | `1.0` | Heartbeat-Intervall in Wanduhrsekunden |
| `FIREFLY_CAT065_SERVICE_ID` | u8 | `1` | Service-ID in I065/015 |

### 1.4 CAT063-Sensor-Status (Per-Sensor-Liveness)

CAT063 meldet je registriertem Sensor, ob er noch Plots liefert (operationell)
oder ausgefallen ist (degradiert) — damit Wayfinder einen **Sensor-Ausfall** von
einem **leeren Himmel** unterscheidet (ADR 0022, Firefly #32). Ein Block je Tick,
ein Record je Sensor (I063/010 SAC/SIC, I063/030 ToD, I063/060 NOGO). Läuft mit,
sobald **Feed *und* Heartbeat** aktiv sind — kein eigener Enable-Schalter.

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_CAT063_PERIOD` | f64 | `5.0` | Intervall der Sensor-Status-Blöcke in Wanduhrsekunden. Langsamer als der Heartbeat, weil Sensor-Liveness sich auf der Skala der Antennenumläufe (4–12 s) ändert |

**Degradiert-Kriterium:** Ein Sensor gilt als aktiv, solange er innerhalb von
`2.5 × scan_period` einen Plot lieferte, sonst degradiert (NOGO `0x40`). Im
**Replay-Modus** sind alle Szenen-Sensoren dauerhaft aktiv (deterministische
Wiedergabe meldet keine Degradierung); im **Live-Modus** folgt die Liveness dem
echten OpenSky-Plot-Eingang.

### 1.5 OpenSky Network ADS-B-Adapter

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_OPENSKY_ENABLED` | bool | `false` | Adapter aktivieren. Deaktiviert → kein ausgehender HTTP-Verkehr |
| `FIREFLY_OPENSKY_LAT_MIN` | f64 | `47.0` | Südliche Begrenzung der Bounding Box (Grad) |
| `FIREFLY_OPENSKY_LAT_MAX` | f64 | `55.0` | Nördliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_LON_MIN` | f64 | `5.0` | Westliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_LON_MAX` | f64 | `16.0` | Östliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_POLL_INTERVAL_SECS` | u64 | `10` | Abfrageintervall in Sekunden (≥ 10 ohne Account, ≥ 5 mit Account) |
| `FIREFLY_OPENSKY_CLIENT_ID` | string | — | OAuth2 Client-ID (optional; ADR 0024). Mit `_CLIENT_SECRET` zusammen → authentifiziert, sonst anonym |
| `FIREFLY_OPENSKY_CLIENT_SECRET` | string | — | OAuth2 Client-Secret (optional; ADR 0024) |
| `FIREFLY_OPENSKY_TOKEN_URL` | string | OpenSky-Keycloak-Realm | OAuth2-Token-Endpoint (Client-Credentials); überschreibbar für Test/Realm-Wechsel |
| `FIREFLY_OPENSKY_SENSOR_ID` | u16 | `200` | Sensor-ID, die ADS-B-Plots im Tracker zugeordnet werden |

> **Standalone-/Dev-Pfad.** Die `FIREFLY_OPENSKY_*`-Variablen konfigurieren **eine**
> OpenSky-Quelle. Im **orchestrierten** Betrieb wird stattdessen `FIREFLY_SOURCES`
> gesetzt (Abschnitt 1.5.1) — dann haben die `FIREFLY_OPENSKY_*`-Variablen **keinen**
> Effekt (Vorrang von `FIREFLY_SOURCES`).

#### FLARM/OGN-Adapter (`FIREFLY_FLARM_*`, ADR 0026)

Zweiter Live-Quell-Adapter: FLARM-Positionen über das Open Glider Network via
APRS-IS. Im Live-Modus per `FIREFLY_FLARM_ENABLED=true` zuschaltbar (standalone)
oder als `flarm_aprs`-Eintrag in `FIREFLY_SOURCES` (orchestriert). Plots fließen in
denselben Tracker wie OpenSky (Fusion).

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_FLARM_ENABLED` | bool | `false` | Adapter im Standalone-Live-Modus aktivieren |
| `FIREFLY_FLARM_LAT_MIN` / `_MAX` | f64 | `47.0` / `55.0` | Bounding Box Süd/Nord (Grad) → APRS-IS-Area-Filter |
| `FIREFLY_FLARM_LON_MIN` / `_MAX` | f64 | `5.0` / `16.0` | Bounding Box West/Ost (Grad) |
| `FIREFLY_FLARM_SERVER` | string | `aprs.glidernet.org` | APRS-IS-Server-Host |
| `FIREFLY_FLARM_PORT` | u16 | `14580` | APRS-IS-Port (Filter-Feed) |
| `FIREFLY_FLARM_CALLSIGN` | string | — | APRS-IS-Login-Callsign (fehlt → read-only anonym) |
| `FIREFLY_FLARM_PASSCODE` | i32 | `-1` | APRS-IS-Passcode (`-1` = read-only) |
| `FIREFLY_FLARM_SENSOR_ID` | u16 | `210` | Sensor-ID der FLARM-Plots |
| `FIREFLY_FLARM_SIGMA_M` | f64 | `20.0` | 1σ-Positionsgenauigkeit (m), isotrop |
| `FIREFLY_FLARM_RECONNECT_MIN_SECS` / `_MAX_SECS` | u64 | `5` / `300` | Reconnect-Backoff (min/max) |

> **Sicherheit:** APRS-IS-Daten sind öffentlich und nicht authentifiziert; Firefly
> sendet nie (read-only). Vertrauensgrenze = Netz-/Quellen-Isolation (ADR 0017).

### 1.5.1 Quell-Eingangs-Kontrakt (`FIREFLY_SOURCES`, ADR 0023)

Maßgeblich: `docs/source-input-contract.md` v1.0.0. Im **Live-Modus** liest Firefly
seine Quellen aus einer JSON-Liste, die ein Orchestrator (Wayfinder) je Instanz
setzt — ein Eintrag je Quelle, mehrere Adapter speisen denselben Live-Tracker.

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_SOURCES` | JSON-Array | — | Quell-Liste. Gesetzt → **Vorrang** vor `FIREFLY_OPENSKY_*`/`FIREFLY_FLARM_*`. Eintrag: `{type, bbox?, sac?, sic?, sensor_id?, cred_env?}`. `type` ∈ `adsb_opensky` / `flarm_aprs` (beide unterstützt) / `radar_asterix` (reserviert → WARN + übersprungen). Unbekannter `type` oder malformes JSON → **Start-Abbruch**. |
| `FIREFLY_SOURCE_<n>_SECRET` o. ä. | string | — | Beliebig **benannte** Credential-Env, von einem Eintrag per `cred_env` referenziert. Wert quellenabhängig: `client_id:client_secret` (`adsb_opensky`) bzw. `callsign:passcode` (`flarm_aprs`), Split am ersten `:`; nie im JSON-Blob. |

Beispiel: siehe `docs/source-input-contract.md` §2. Referenzpunkt = Mittelpunkt der
**Union** aller Quell-BBoxen (`FIREFLY_SYSTEM_REF_*` überschreibt); Ausgabe-Takt =
**min** Poll-Intervall der Quellen. Jede Quelle stempelt ihre `sensor_id` auf ihre
Plots; die Sensor-Liveness (CAT063) verfolgt alle Quellen.

### 1.6 WebSocket-Zugangskontrolle (NFR-SEC-001, ADR 0017)

Beide Variablen sind **opt-in** — ohne Konfiguration ist kein Schutz aktiv
(geeignet für lokales Demo/Entwicklung). Für Produktionsbetrieb wird mindestens
ein Token empfohlen.

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_WS_TOKEN` | string | — | Wenn gesetzt, muss jede `/ws`-Verbindung den Token via `Authorization: Bearer <token>` oder `?token=<wert>` vorlegen. Fehlend oder falsch → 401 |
| `FIREFLY_WS_ALLOWED_ORIGIN` | string | — | Wenn gesetzt, muss der `Origin`-Header exakt mit diesem Wert übereinstimmen. Fehlt oder stimmt nicht → 403. Ergänzt Token-Auth (fail-closed) |

**Hinweis Browser-API:** `WebSocket` im Browser unterstützt keine Custom-Header.
Für Browser-Clients daher den `?token=`-Queryparameter verwenden.

### 1.7 Logging

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
| `CAT063 sensor status sender enabled destination=... period_s=5 sensors_total=3` | Sensor-Status aktiv; zeigt Sensor-Anzahl |
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
| `firefly_live_plots_ingested_total` | counter | **Live-Modus:** Plots insgesamt in den Tracker eingespeist |
| `firefly_plot_records_written_total` | counter | **Live-Modus:** In `.ffplots`-Datei geschriebene Records |
| `firefly_opensky_poll_errors_total` | counter | **Live-Modus:** HTTP/Netz-Fehler beim OpenSky-Poll |
| `firefly_flarm_plots_received_total` | counter | **Live-Modus:** Empfangene FLARM/OGN-Plots (APRS-IS, ADR 0026) |
| `firefly_cat063_status_sent_total` | counter | Gesendete CAT063-Sensor-Status-Blöcke |
| `firefly_sensors_total` | gauge | Anzahl registrierter Sensoren (statisch) |
| `firefly_sensors_active` | gauge | Anzahl aktuell aktiver Sensoren (Plot innerhalb `2.5 × scan_period`) |

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

Prüft, ob der Server Traffic verarbeiten kann.

- **Replay-Modus:** Immer `200 ready` — die Szene ist beim Start vollständig geladen.
- **Live-Modus:** `503 not ready` bis zum ersten erfolgreichen OpenSky-Poll (d. h.
  mindestens ein Luftfahrzeug gemeldet). Danach `200 ready`. Kubernetes sendet damit
  keinen Traffic an einen Pod, der noch kein Luftlagebild hat (ADR 0020, AP9.4c-4).

```bash
# Replay-Modus oder nach erstem Poll im Live-Modus:
curl http://localhost:8080/ready
# → ready  (HTTP 200)

# Live-Modus vor erstem Poll:
curl -o /dev/null -w "%{http_code}" http://localhost:8080/ready
# → 503
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

## 8. Betriebsmodus: Replay vs. Live (AP9.4c-0…4, ✅ implementiert)

| Modus | Beschreibung | Aktivierung |
|-------|--------------|-------------|
| **Replay** (Standard) | Vorberechnete Szene (`demo` oder `frankfurt`) in Wanduhrzeit abgespielt | `FIREFLY_MODE=replay` (Default, auch wenn `FIREFLY_MODE` nicht gesetzt) |
| **Live** ✅ | OpenSky ADS-B-Daten, Tracker läuft in Echtzeit; CAT062 + WebSocket-Feed live | `FIREFLY_MODE=live` (impliziert OpenSky als Plot-Quelle) |

Im Live-Modus startet Firefly ohne vorberechnete Szene. OpenSky-Plots werden
in den Tracker eingespeist, Snapshots über `watch`-Kanal an den WebSocket-
und CAT062-Ausgang geliefert. Gleichzeitig werden alle Plots in eine
`.ffplots`-Datei aufgezeichnet (ADR 0020) — für deterministisches Replay und
Debugging.

> **Hinweis:** `FIREFLY_OPENSKY_ENABLED=true` aktiviert den OpenSky-Poller
> im Replay-Modus als **Log-only**-Pfad (Plots werden geloggt, aber nicht in
> den Tracker eingespeist). Für echten Live-Betrieb ausschließlich
> `FIREFLY_MODE=live` verwenden.

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
            - name: FIREFLY_OPENSKY_CLIENT_ID
              valueFrom:
                secretKeyRef:
                  name: opensky-credentials
                  key: client_id
            - name: FIREFLY_OPENSKY_CLIENT_SECRET
              valueFrom:
                secretKeyRef:
                  name: opensky-credentials
                  key: client_secret
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
| Multicast ohne Authentifizierung | ADR 0017 | Netz-Isolation + anwendungsseitige Absicherung |
| OpenSky-Passwort nur via Env-Variable | ADR 0003 | Kubernetes Secret (bereits empfohlen) |

---

## Weiterführend

- **Installationshandbuch** (`docs/INSTALLATION.md`): Erstinbetriebnahme.
- **ICD CAT062** (`docs/ICD-CAT062.md`): Byte-genauer Draht-Vertrag mit Wayfinder.
- **ADR-Verzeichnis** (`docs/decisions/`): Alle Architekturentscheide.
- **Anforderungsregister** (`docs/requirements/README.md`): Rückverfolgbarkeit.
