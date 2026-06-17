# Docker-Setup für Firefly (M6.4)

Firefly läuft jetzt auch in Containern — ideal für Cloud-Deployment, Entwicklung ohne lokale Rust-Installation, und reproduzierbare Umgebungen.

## Schnellstart

```bash
# Demo-Szenario (zwei Flugzeuge): ~5 Sekunden, dann fertig.
docker-compose up

# Oder: Frankfurt-Showcase (40 Minuten, drei Radare, acht Flugzeuge).
FIREFLY_SCENE=frankfurt docker-compose up
```

Dann im Browser: **http://localhost:8080**

## Details

### Dockerfile

**Multi-stage build:**
1. **Builder-Stage** (`rust:1.82-bookworm`): Kompiliert den ganzen Workspace im Release-Modus.
2. **Runtime-Stage** (`debian:bookworm-slim`): Minimal-Image mit nur dem Binary und statischen Assets (~50 MB).

**Healthcheck:** Der Container prüft, ob der Server auf `/health` antwortet.

### docker-compose.yml

**Service `firefly-server`:**
- Port: `8080` (HTTP, WebSocket)
- Umgebungsvariablen:
  - `FIREFLY_SCENE`: `demo` (default) oder `frankfurt`
  - `RUST_LOG`: `info` (default) — setze auf `debug` für ausführliches Logging
- Healthcheck: prüft alle 10 Sekunden
- Restart-Policy: `unless-stopped`

**Beispiel mit Custom-Szenario:**
```bash
FIREFLY_SCENE=frankfurt RUST_LOG=debug docker-compose up
```

## Lokaler Build (ohne docker-compose)

```bash
docker build -t firefly-server:latest .
docker run -p 8080:8080 -e FIREFLY_SCENE=demo firefly-server:latest
```

## Cloud-Deployment

Das Docker-Image eignet sich für:
- **Kubernetes**: Manifest mit `firefly-server:latest` Image und Port 8080 expose
- **Docker Swarm**: Einfach `docker-compose up -d` auf dem Manager-Node
- **Cloud Run / App Engine / ECS**: Standard OCI-Image, keine speziellen Dependencies

**12-Factor Config:**
- Alle Parameter via Env-Vars (`FIREFLY_SCENE`, `RUST_LOG`)
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
End-to-End-Test mit Wayfinder empfiehlt sich das Frankfurt-Szenario:

```bash
FIREFLY_SCENE=frankfurt FIREFLY_CAT062_ENABLED=true docker-compose up
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
