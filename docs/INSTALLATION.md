# Firefly — Installationshandbuch

> **Zielgruppe:** Systembetreiber, die Firefly erstmalig einrichten.
> Dieses Handbuch deckt den Weg von Null bis zum laufenden System ab —
> Quellcode-Build, Docker-Betrieb und quellen-getriebener Echtbetrieb
> (OpenSky-ADS-B, FLARM, Radar-ASTERIX).

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

## 4. Schnellstart: nackter Start (leerer Himmel)

Firefly läuft immer als **quellen-getriebener Live-Tracker** (ADR 0030). Ohne
konfigurierte Quelle startet der Server mit **leerem Himmel** — er dient die
Karte, beantwortet `/health`/`/ready` und sendet (falls aktiviert) den
CAT065-Heartbeat, verarbeitet aber keine Plots. Kein Überraschungs-Egress:
alle Quell-Adapter sind Opt-in.

```bash
cargo run --release -p firefly-server
# oder nach dem Build direkt:
./target/release/firefly-server
```

Firefly startet auf Port **8080**. Im Browser öffnen:

```
http://localhost:8080
```

Die Karte bleibt leer, die Konsole gibt strukturierte JSON-Logs aus
(Verbosity: `info`). Tracks erscheinen, sobald eine Quelle konfiguriert ist —
Abschnitt 7 (OpenSky) ist der schnellste Weg.

> **Frühere Demo-Szenen:** `FIREFLY_SCENE`/`FIREFLY_SPEED`/`FIREFLY_MODE`
> wurden mit ADR 0030 ausgebaut und werden ignoriert (Warn-Log). Die
> Frankfurt-Mehrradar-Szene lebt als Regressions-Fixture in
> `firefly-player/tests/frankfurt_regression.rs` weiter.

---

## 6. Docker-Betrieb

### Image bauen

```bash
docker build -t firefly-server:latest .
```

### Start ohne Quellen (leerer Himmel)

```bash
docker run --rm -p 8080:8080 firefly-server:latest
```

### Mit OpenSky-Quelle und CAT062-Multicast-Feed (für Wayfinder)

Der Multicast-Feed ist standardmäßig deaktiviert (kein unbeabsichtigter
UDP-Verkehr). Für den Verbund mit Wayfinder:

```bash
docker run --rm \
  --network host \                         # Multicast benötigt Host-Netz
  -e FIREFLY_OPENSKY_ENABLED=true \
  -e FIREFLY_OPENSKY_CLIENT_ID=client_id \
  -e FIREFLY_OPENSKY_CLIENT_SECRET=client_secret \
  -e FIREFLY_CAT062_ENABLED=true \
  -e FIREFLY_CAT062_GROUP=239.255.0.62 \
  -e FIREFLY_CAT062_PORT=8600 \
  firefly-server:latest
```

> **Hinweis `--network host`:** Docker-NAT blockiert UDP-Multicast.
> Auf Linux ist `--network host` die einfachste Lösung für den Einzelrechner-Betrieb.
> In Kubernetes wird Multicast über einen dedizierten Netz-Plugin konfiguriert.

---

## 7. ADS-B-Echtbetrieb mit OpenSky Network

Mit aktivierter OpenSky-Quelle (`FIREFLY_OPENSKY_ENABLED=true`, ADR 0030:
alle Adapter sind Opt-in) bezieht Firefly Echtzeit-ADS-B-Positionen vom
OpenSky Network REST API, speist sie direkt in den Tracker und sendet
kontinuierlich CAT062- und WebSocket-Daten an Wayfinder bzw. den Browser.
Ist `FIREFLY_PLOT_RECORD_PATH` gesetzt (opt-in, QW.4), werden alle eingehenden
Plots parallel in die dort benannte `.ffplots`-Datei aufgezeichnet — die
Grundlage für deterministisches Replay/Wiederanlauf (ADR 0020; Details und
Nicht-Fatal-Verhalten in `docs/TECHNICAL.md` §6.2).

Mit mindestens einer Radar-Quelle kann zusätzlich die **Sensor-Registrierung**
aktiviert werden (ADR 0034): `FIREFLY_REGISTRATION_ENABLED=true` startet den
Monitor, der die systematischen Radar-Messfehler (Range-/Azimut-Bias) laufend
aus dem Datenstrom schätzt (Logs + Metriken, Schattenmodus);
`FIREFLY_REGISTRATION_APPLY=true` zieht die geschätzten Biases zusätzlich
**vor der Fusion** von den Radar-Messungen ab — abgesichert durch ein
Anwendungs-Gate (Beobachtbarkeit, Residuen-Gewinn, Plausibilität) und
geglättete Übergänge (`docs/TECHNICAL.md` §1.5.2).

> **Orchestrierter Betrieb (ADR 0023).** Dieser Abschnitt beschreibt die
> **Standalone**-Konfiguration über `FIREFLY_OPENSKY_*` (eine Quelle, von Hand
> gesetzt). Wird Firefly von Wayfinder **auto-orchestriert** (eine Instanz pro
> Feed), setzt der Orchestrator stattdessen `FIREFLY_SOURCES` (JSON-Quell-Liste)
> + benannte Credential-Envs; diese haben dann **Vorrang** vor `FIREFLY_OPENSKY_*`.
> Den Vertrag beschreibt `docs/source-input-contract.md`.

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

OpenSky akzeptiert nur noch **OAuth2 Client-Credentials** (Basic Auth ist
abgeschaltet, ADR 0024). Auf der OpenSky-Account-Seite einen **API-Client**
anlegen und `client_id` + `client_secret` abrufen — Firefly tauscht sie selbst
gegen ein kurzlebiges Token (kein manueller Token-Schritt nötig):

```bash
export FIREFLY_OPENSKY_CLIENT_ID=mein_client_id
export FIREFLY_OPENSKY_CLIENT_SECRET=mein_client_secret
export FIREFLY_OPENSKY_POLL_INTERVAL_SECS=5   # mit Account: 5 s möglich
```

> **Sicherheitshinweis:** Client-Secrets nie direkt in Shell-Skripten hartkodieren.
> Im Produktionsbetrieb über Kubernetes Secrets oder ein Vault-System injizieren.

### Schritt 4: Mit aktivierter OpenSky-Quelle starten

```bash
FIREFLY_OPENSKY_ENABLED=true \
FIREFLY_OPENSKY_LAT_MIN=49.0 \
FIREFLY_OPENSKY_LAT_MAX=51.0 \
FIREFLY_OPENSKY_LON_MIN=7.0 \
FIREFLY_OPENSKY_LON_MAX=10.0 \
./target/release/firefly-server
```

Im Log erscheinen dann Meldungen wie:

```
INFO starting Firefly server (sources-driven live tracker)
INFO OpenSky ADS-B poller started lat_min=49 lat_max=51 lon_min=7 lon_max=10
INFO live tracker tick tracks=12 plots_ingested=47
```

Die Readiness-Probe (`/ready`) gibt `503` zurück, bis der erste OpenSky-Poll
erfolgreich war — danach `200 ready`. Wayfinder zeigt Tracks sobald `/ready`
positiv antwortet.

### Schritt 4a (optional): ADS-B ohne Account — Community-Aggregator

Wenn OpenSky nicht erreichbar ist (verbreitete **Datacenter-IP-Sperre**, z. B.
aus GitHub Codespaces/Azure) oder kein OAuth2-Client gewünscht ist, liefert der
Community-Aggregator-Adapter (ADR 0031) dieselbe crowdgesourcte ADS-B-Lage
**ohne jede Anmeldung** — über adsb.lol (Default) oder adsb.fi:

```bash
FIREFLY_ADSBAGG_ENABLED=true \
FIREFLY_ADSBAGG_LAT_MIN=49.0 \
FIREFLY_ADSBAGG_LAT_MAX=51.0 \
FIREFLY_ADSBAGG_LON_MIN=7.0 \
FIREFLY_ADSBAGG_LON_MAX=10.0 \
./target/release/firefly-server
```

Weitere Variablen mit Defaults: `FIREFLY_ADSBAGG_PROVIDER` (`adsb_lol` |
`adsb_fi`), `FIREFLY_ADSBAGG_POLL_INTERVAL_SECS` (`10`),
`FIREFLY_ADSBAGG_SENSOR_ID` (`230`), `FIREFLY_ADSBAGG_BASE_URL` (Override für
Tests/self-hosted). Die BBox wird intern als Umkreis abgefragt (API-Limit
250 NM; größere BBoxen werden mit Warnung geclampt) und die Antwort auf die
BBox zurückgefiltert. OpenSky- und Aggregator-Quelle können auch **gleichzeitig**
laufen — der Tracker fusioniert beide.

> **Orchestrierter Betrieb:** als `adsb_aggregator`-Eintrag in `FIREFLY_SOURCES`
> (Vertrag `docs/source-input-contract.md` v1.5.0), ohne `cred_env`.

### Schritt 4b (optional): FLARM/OGN als zusätzliche Quelle

FLARM-getragene Luftfahrzeuge (Segelflieger, UL, tiefe GA) sind über das **Open
Glider Network (OGN)** via APRS-IS verfügbar (ADR 0026). Der Adapter ist im
Live-Modus per `FIREFLY_FLARM_ENABLED=true` zuschaltbar und speist seine Plots in
denselben Tracker wie OpenSky — beide Quellen werden fusioniert.

```bash
export FIREFLY_FLARM_ENABLED=true
export FIREFLY_FLARM_LAT_MIN=49.0
export FIREFLY_FLARM_LAT_MAX=51.0
export FIREFLY_FLARM_LON_MIN=7.0
export FIREFLY_FLARM_LON_MAX=10.0
# Standard: read-only anonym (kein Login nötig — Firefly sendet nie).
# Optional ein benannter APRS-IS-Account:
# export FIREFLY_FLARM_CALLSIGN=EDXY
# export FIREFLY_FLARM_PASSCODE=12345
```

Weitere Variablen mit Defaults: `FIREFLY_FLARM_SERVER` (`aprs.glidernet.org`),
`FIREFLY_FLARM_PORT` (`14580`), `FIREFLY_FLARM_SENSOR_ID` (`210`),
`FIREFLY_FLARM_SIGMA_M` (`20`), `FIREFLY_FLARM_RECONNECT_MIN_SECS`/`_MAX_SECS`
(`5`/`300`). APRS-IS-Daten sind **öffentlich und nicht authentifiziert** — die
Vertrauensgrenze ist die Netz-/Quellen-Isolation (ADR 0017), wie bei ADS-B.

> **Orchestrierter Betrieb:** Im auto-orchestrierten Pfad (ADR 0023) setzt der
> Orchestrator FLARM stattdessen als `flarm_aprs`-Eintrag in `FIREFLY_SOURCES`
> (Vertrag `docs/source-input-contract.md` v1.2.0); `FIREFLY_SOURCES` hat dann
> **Vorrang** vor `FIREFLY_FLARM_*`.

### Schritt 4c (optional): Radar-ASTERIX-Quelle (CAT048 über UDP)

Ein **realer Monoradar** kann seine Ziel-Meldungen als **ASTERIX CAT048 über UDP**
senden (ADR 0028). Der Adapter ist im Live-Modus per `FIREFLY_RADAR_ENABLED=true`
zuschaltbar und speist **Polar-Plots** in denselben Tracker wie ADS-B/FLARM —
alle Quellen werden fusioniert. Da CAT048 polar **relativ zum Radar** ist und
den Standort nicht trägt, **muss** der Radar-Standort (`LAT`/`LON`) gesetzt werden.

```bash
export FIREFLY_RADAR_ENABLED=true
export FIREFLY_RADAR_SAC=1
export FIREFLY_RADAR_SIC=4
export FIREFLY_RADAR_LAT=50.03      # Radar-Standort (Pflicht)
export FIREFLY_RADAR_LON=8.57
# Listen-Endpoint: Multicast-Gruppe (wird beigetreten) oder 0.0.0.0 (Unicast).
export FIREFLY_RADAR_GROUP=239.255.0.48
export FIREFLY_RADAR_PORT=8048
```

Weitere Variablen mit Defaults: `FIREFLY_RADAR_HEIGHT_M` (`0`),
`FIREFLY_RADAR_SENSOR_ID` (`220`), `FIREFLY_RADAR_SCAN_SECS` (`4`),
`FIREFLY_RADAR_SIGMA_RANGE_M` (`50`), `FIREFLY_RADAR_SIGMA_AZ_DEG` (`0.1`).
ASTERIX-UDP ist **nicht authentifiziert** — die Vertrauensgrenze ist die
Netz-/Quellen-Isolation (ADR 0017), wie bei den anderen Feeds.

Der Eingang versteht neben CAT048/CAT034 auch die **Legacy-Generation
CAT001/CAT002** (FEP.4) — ein älterer Radarkopf wird mit denselben Variablen
konfiguriert, ohne weiteres Zutun. Voraussetzung: Der Kopf sendet CAT002
mit (Standard bei realen Anlagen), denn daraus kommt die volle Tageszeit,
an der die trunkierten CAT001-Zeitstempel verankert werden.

> **Orchestrierter Betrieb:** Im auto-orchestrierten Pfad (ADR 0023) setzt der
> Orchestrator den Radar stattdessen als `radar_asterix`-Eintrag in
> `FIREFLY_SOURCES` (Vertrag `docs/source-input-contract.md` v1.3.0, Felder
> `sac`/`sic`/`lat`/`lon`/`listen`); `FIREFLY_SOURCES` hat dann **Vorrang** vor
> `FIREFLY_RADAR_*`.

### Schritt 4d (optional): ADS-B-Bodenstation (CAT021 über UDP)

Eine **eigene ADS-B-Bodenstation** kann ihre Zielmeldungen als **ASTERIX CAT021
über UDP** senden (FEP.3) — der Produktions-Bezugsweg für ADS-B, statt der
Internet-REST-Quellen (Schritt 4/4b). Der Adapter ist per
`FIREFLY_ADSB021_ENABLED=true` zuschaltbar und speist **geodätische Plots** in
denselben Tracker; die Messunsicherheit kommt aus dem **NACp** jeder Meldung.
Boden-/Simulations-/Testziele werden verworfen. Kein Stations-Standort nötig
(CAT021-Positionen sind WGS84-Selbstmeldungen).

```bash
export FIREFLY_ADSB021_ENABLED=true
export FIREFLY_ADSB021_SAC=25
export FIREFLY_ADSB021_SIC=10
# Listen-Endpoint: Multicast-Gruppe (wird beigetreten) oder 0.0.0.0 (Unicast).
export FIREFLY_ADSB021_GROUP=239.255.0.21
export FIREFLY_ADSB021_PORT=8021
```

Weitere Variable mit Default: `FIREFLY_ADSB021_SENSOR_ID` (`230`). Ist die
Bodenstation die **einzige** Quelle, zusätzlich `FIREFLY_SYSTEM_REF_*` setzen
(Schritt 5) — die Quelle trägt keine BBox zum Referenzpunkt bei.

> **Orchestrierter Betrieb:** als `adsb_asterix`-Eintrag in `FIREFLY_SOURCES`
> (Vertrag `docs/source-input-contract.md` v1.6.0, Felder
> `listen`?/`sac`?/`sic`?/`sensor_id`?); `FIREFLY_SOURCES` hat dann **Vorrang**
> vor `FIREFLY_ADSB021_*`.

### Schritt 4e (optional): WAM/MLAT-System (CAT020/019 über UDP)

Ein **Multilaterations-System** (WAM) kann seine Zielmeldungen als **ASTERIX
CAT020** und seinen Systemstatus als **CAT019** über UDP senden (FEP.5) —
unabhängige Überwachung neben Radar und ADS-B. Der Adapter ist per
`FIREFLY_MLAT_ENABLED=true` zuschaltbar; die Messunsicherheit kommt je
Meldung aus **I020/500** (Standardabweichung der Positionslösung).
Feldmonitor-, Simulations-/Test- und Bodenziele werden verworfen. Kein
Standort nötig (CAT020-Positionen sind geodätisch).

```bash
export FIREFLY_MLAT_ENABLED=true
export FIREFLY_MLAT_SAC=25
export FIREFLY_MLAT_SIC=40
# Listen-Endpoint: Multicast-Gruppe (wird beigetreten) oder 0.0.0.0 (Unicast).
export FIREFLY_MLAT_GROUP=239.255.0.20
export FIREFLY_MLAT_PORT=8020
```

Weitere Variable mit Default: `FIREFLY_MLAT_SENSOR_ID` (`240`). Als
**einzige** Quelle zusätzlich `FIREFLY_SYSTEM_REF_*` setzen (Schritt 5).

> **Orchestrierter Betrieb:** als `mlat_asterix`-Eintrag in `FIREFLY_SOURCES`
> (Vertrag v1.7.0, Felder `listen`?/`sac`?/`sic`?/`sensor_id`?);
> `FIREFLY_SOURCES` hat dann **Vorrang** vor `FIREFLY_MLAT_*`.

### Schritt 4f (optional): QNH-Regionen (Meteo-Dienst, VERT.1)

Für die **Vertikal-Kette** (QNH-korrigierte Höhen unterhalb der Transition
Altitude, Verwertung ab VERT.2) kann Firefly **regionale QNH-Werte**
mitgegeben werden. Ohne Konfiguration gilt überall die Standardatmosphäre
(1013,25 hPa) — ehrlich gekennzeichnet, aber unterhalb der Transition
Altitude entsprechend ungenau (~27–30 ft je hPa Abweichung).

```bash
export FIREFLY_METEO_QNH='[
  {"name":"EDDF","lat":50.03,"lon":8.57,"radius_nm":60,"qnh_hpa":1008},
  {"name":"EDDK","lat":50.87,"lon":7.14,"radius_nm":60,"qnh_hpa":1011}
]'
```

`radius_nm` ist optional (fehlt = unbegrenzt); implausible Werte
(`qnh_hpa` außerhalb [870, 1085]) oder malformes JSON brechen den Start ab.
Die Werte werden vom Betreiber extern im Wetter-Zyklus aktualisiert; ein
automatischer METAR-Abruf ist ein dokumentiertes Folge-Häppchen.

### Schritt 4g (optional): Flugpläne (FPL.1)

Für die **Flugplan-Korrelation** (Track ↔ Flugplan, ADR 0038) kann Firefly
eine Liste gefileter Flugpläne mitgegeben werden. Ohne Konfiguration läuft
der Tracker unverändert — kein Track trägt dann ein Flugplan-Label.

```bash
export FIREFLY_FLIGHT_PLANS='[
  {"callsign":"DLH123","squawk":1234,"departure":"EDDF",
   "destination":"EDDM","expected_time":1752580800},
  {"callsign":"BAW22","squawk":"7500"}
]'
```

Nur `callsign` ist Pflicht (Primärschlüssel, doppelte Callsigns brechen den
Start ab). `squawk` wird **oktal wie geschrieben** gelesen — `1234` und
`"1234"` bedeuten beide Oktal 1234; eine Ziffer 8/9 bricht den Start ab
(nie stille Dezimal-Uminterpretation). `expected_time` ist Unix-Epoche in
Sekunden (Mitte des ±45-min-Plausibilitätsfensters; fehlt = zeitlich immer
plausibel). Malformes JSON oder implausible Werte brechen den Start ab;
unset heißt schlicht „keine Flugpläne" (INFO im Log). Der Feldsatz wächst
additiv (EFS-Bedarf, Wayfinder #244); eine Live-FDPS-Anbindung ist ein
dokumentiertes Folge-Häppchen.

### Schritt 5 (optional): System-Referenzpunkt setzen

Der **System-Referenzpunkt** (ADR 0021) ist der gemeinsame Ursprung für den
Tracking-Frame und die CAT062-I062/100-Projektion. Im Live-Modus ist er
standardmäßig die Mitte der OpenSky-Bounding-Box; bei Bedarf explizit setzen:

```bash
export FIREFLY_SYSTEM_REF_LAT=50.0379
export FIREFLY_SYSTEM_REF_LON=8.5622
```

> **Hinweis:** Das ändert **nicht** die Darstellung im ASD — Wayfinder rendert
> aus der absoluten WGS84-Position (I062/105). Der Referenzpunkt betrifft nur die
> optionale System-Stereografisch-Ebene (I062/100). Im **Replay-Modus** ist der
> Referenzpunkt fest der Szenen-Ursprung und wird ignoriert.

---

## 8. CAT062-Multicast-Feed für Wayfinder aktivieren

Damit Wayfinder die Tracks empfangen kann, müssen CAT062 (Tracks) und CAT065
(Heartbeat) aktiviert sein. CAT063 (Sensor-Status) läuft automatisch mit, sobald
beide aktiv sind:

```bash
export FIREFLY_CAT062_ENABLED=true
export FIREFLY_CAT062_GROUP=239.255.0.62   # Default
export FIREFLY_CAT062_PORT=8600             # Default
export FIREFLY_CAT065_ENABLED=true          # Default: an (wenn CAT062 an)
export FIREFLY_CAT063_PERIOD=5.0            # Default: 5 s; Sensor-Status-Takt
```

Der Strom trägt damit drei ASTERIX-Kategorien auf derselben Gruppe/Port:
**CAT062** (Tracks, `0x3E`), **CAT065** (SDPS-Heartbeat, `0x41`) und **CAT063**
(Per-Sensor-Status, `0x3F`). CAT063 erlaubt Wayfinder, einen ausgefallenen
Sensor von einem leeren Himmel zu unterscheiden.

Wayfinder muss derselben Multicast-Gruppe beitreten:

```bash
# Wayfinder-Seite (Beispiel):
export WAYFINDER_MULTICAST_GROUP=239.255.0.62
export WAYFINDER_MULTICAST_PORT=8600
```

---

## 9. WebSocket-Zugangskontrolle absichern (NFR-SEC-001, ADR 0017)

Standardmäßig ist der `/ws`-Endpunkt ohne Authentifizierung erreichbar — geeignet
für lokale Entwicklung. Im Produktionsbetrieb empfiehlt sich mindestens ein
Bearer-Token:

```bash
export FIREFLY_WS_TOKEN=mein-geheimes-token
# optional: erlaubte Browser-Origin einschränken
export FIREFLY_WS_ALLOWED_ORIGIN=https://mein-asd.example.com
```

- **Token-Prüfung:** Clients müssen `Authorization: Bearer <token>` senden.  
  Da der Browser-`WebSocket`-API keine Custom-Header unterstützt, akzeptiert
  Firefly alternativ `?token=<wert>` als Query-Parameter.  
  Fehlend oder falsch → **401 Unauthorized**.
- **Origin-Prüfung:** Der `Origin`-Header muss exakt mit `FIREFLY_WS_ALLOWED_ORIGIN`
  übereinstimmen. Fehlt oder stimmt nicht → **403 Forbidden**.
- Beide Variablen sind unabhängig — einer, beide oder keiner kann gesetzt werden.
- Im Produktionsbetrieb Token via Kubernetes Secret injizieren, nicht in Skripten
  hartkodieren.

---

## 10. Health-Check und Readiness verifizieren

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

## 11. Vollständiges Beispiel: ADS-B-Quelle + Wayfinder

```bash
# Alle Optionen kombiniert (produktionsnah, quellen-getrieben):
FIREFLY_PORT=8080 \
FIREFLY_CAT062_ENABLED=true \
FIREFLY_CAT062_GROUP=239.255.0.62 \
FIREFLY_CAT062_PORT=8600 \
FIREFLY_CAT065_ENABLED=true \
FIREFLY_OPENSKY_ENABLED=true \
FIREFLY_OPENSKY_CLIENT_ID=mein_client_id \
FIREFLY_OPENSKY_CLIENT_SECRET=mein_client_secret \
FIREFLY_OPENSKY_LAT_MIN=49.0 \
FIREFLY_OPENSKY_LAT_MAX=51.0 \
FIREFLY_OPENSKY_LON_MIN=7.0 \
FIREFLY_OPENSKY_LON_MAX=10.0 \
RUST_LOG=info \
./target/release/firefly-server
```

---

## 12. Fehlerdiagnose beim Start

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
