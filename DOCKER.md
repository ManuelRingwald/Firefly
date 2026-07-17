# Docker-Setup für Firefly (M6.4)

Firefly läuft jetzt auch in Containern — ideal für Cloud-Deployment, Entwicklung ohne lokale Rust-Installation, und reproduzierbare Umgebungen.

## Schnellstart

```bash
# Ohne Quellen: leerer Himmel + Heartbeat (ADR 0030) — Server, Karte, Probes laufen.
docker-compose up

# Mit OpenSky-ADS-B-Quelle (Opt-in; Konto siehe docs/INSTALLATION.md §7):
FIREFLY_OPENSKY_ENABLED=true \
FIREFLY_OPENSKY_CLIENT_ID=client_id FIREFLY_OPENSKY_CLIENT_SECRET=client_secret \
docker-compose up
```

Dann im Browser: **http://localhost:8080**

## Details

### Dockerfile

**Multi-stage build:**
1. **Builder-Stage** (`rust:1.82-bookworm`): Kompiliert den ganzen Workspace im Release-Modus.
2. **Runtime-Stage** (`debian:bookworm-slim`): Minimal-Image mit nur dem Binary und statischen Assets (~50 MB).

**Healthcheck:** Der Container prüft, ob der Server auf `/health` antwortet —
über den **eingebauten Selbsttest** `firefly-server --healthcheck` (#99,
FR-OPS-010): ein lokaler `GET /health`, Exit-Code 0 = gesund, 1 = nicht.
Bewusst **kein** `curl` im Image (das Slim-Image enthält keins; der frühere
curl-Aufruf scheiterte deshalb immer und meldete jeden Container dauerhaft
`unhealthy`). Der Selbsttest respektiert `FIREFLY_PORT`.

Verifizieren (Akzeptanzkriterien aus #99):

```bash
docker ps                       # Status: (healthy)
docker inspect --format '{{json .State.Health.Log}}' <container> | jq .
# Negativtest — der Check misst wirklich: Server-Port im Container blockieren
# bzw. FIREFLY_PORT im Check-Kontext verstellen ⇒ Status kippt auf unhealthy.
```

### docker-compose.yml

**Service `firefly-server`:**
- Port: `8080` (HTTP, WebSocket)
- Umgebungsvariablen:
  - Quellen: `FIREFLY_SOURCES` (orchestriert, ADR 0023) oder die
    Opt-in-Adapter-Envs `FIREFLY_OPENSKY_*`/`FIREFLY_FLARM_*`/`FIREFLY_RADAR_*`
  - `RUST_LOG`: `info` (default) — setze auf `debug` für ausführliches Logging
- Healthcheck: prüft alle 10 Sekunden
- Restart-Policy: `unless-stopped`

**Beispiel mit ausführlichem Logging:**
```bash
FIREFLY_OPENSKY_ENABLED=true RUST_LOG=debug docker-compose up
```

## Lokaler Build (ohne docker-compose)

```bash
docker build -t firefly-server:latest .
docker run -p 8080:8080 firefly-server:latest
```

## Cloud-Deployment

Das Docker-Image eignet sich für:
- **Kubernetes**: Manifest mit `firefly-server:latest` Image und Port 8080 expose
- **Docker Swarm**: Einfach `docker-compose up -d` auf dem Manager-Node
- **Cloud Run / App Engine / ECS**: Standard OCI-Image, keine speziellen Dependencies

**12-Factor Config:**
- Alle Parameter via Env-Vars (`FIREFLY_SOURCES`/Adapter-Envs, `RUST_LOG`)
- Graceful Shutdown via SIGTERM (der Server horcht darauf)
- Stdout-Logging (strukturiert via `tracing-subscriber`)

## Troubleshooting

**Build schlägt fehl:**
- Stelle sicher, dass dein Docker-Daemon läuft (`docker ps`)
- Prüfe, dass genug Disk-Space für den Build vorhanden ist (~3 GB Intermediate-Images während Build)

**Server startet nicht:**
- Checke Logs: `docker-compose logs firefly-server`
- Prüfe, ob Port 8080 bereits belegt ist: `lsof -i :8080`

**Performance im Container:**
- Runtime-Image ist optimiert; Build ist ~2–3 Min auf moderner Hardware
- Multi-stage spart ~1 GB Speicher gegenüber einem Single-stage Build

## Mit Wayfinder (End-to-End-ASD)

Standardmäßig sendet der Container **keinen** CAT062-Multicast. Für den
End-to-End-Test mit Wayfinder den Feed aktivieren und eine Quelle konfigurieren
(empfohlen: gleich Wayfinders orchestrierten Stack nutzen, der je Feed eine
Firefly-Instanz spawnt — siehe Wayfinders `DOCKER.md`/`docs/CODESPACES.md`):

```bash
FIREFLY_OPENSKY_ENABLED=true FIREFLY_CAT062_ENABLED=true docker-compose up
```

Multicast (`239.255.0.62:8600`) traversiert Docker's Standard-Bridge-Netz
nicht — für den Container-basierten End-to-End-Test mit Wayfinder muss daher
`network_mode: host` verwendet werden (Linux). Details und der lokale Weg
(ohne Docker) stehen in Wayfinders `README.md`/`DOCKER.md`.

Unter **macOS/Windows (Docker Desktop)** funktioniert `network_mode: host`
nicht zuverlässig (Container sehen nur die Docker-VM, nicht den Host).
Wayfinders `DOCKER.md` beschreibt dafür eine Bridge-Netzwerk-Variante mit
gemeinsamem Master-Compose (beide Repos als Geschwister-Ordner).

## Zukunft

- Optional: **nginx Reverse Proxy** (in docker-compose.yml auskommentiert) für HTTPS/Load-Balancing
- Optional: **PostgreSQL Service** für State-Persistence (derzeit nur In-Memory)
- Optional: **Prometheus + Grafana** für Observability
