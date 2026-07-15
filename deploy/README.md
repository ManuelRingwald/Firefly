# Firefly-Deployment (HA.3)

Zwei Wege, ein Ziel: ein korrekt verdrahtetes **Main/Standby-Paar**
(ADR 0041) mit **gemeinsamem Snapshot-Volume** (ADR 0040) und dem
CAT062/065/063-Multicast-Feed.

| Artefakt | Zweck |
|----------|-------|
| `helm/firefly/` | Helm-Chart (der empfohlene Weg): `helm install firefly deploy/helm/firefly` |
| `kubernetes/firefly.yaml` | Statisches Äquivalent für `kubectl apply` (spiegelt die Chart-Defaults) |
| `validate.sh` | YAML-Syntax-Check überall; `helm lint` + Voll-Render, wo Helm existiert |

## Was das Rezept richtig macht (und warum)

- **Eine ConfigMap für beide Instanzen** — identische Konfiguration ist
  ADR-0041-Voraussetzung; der HA.1-Fingerprint lehnt eine Übernahme mit
  abweichender Quell-Konfiguration ab. Geteilte Map = Drift strukturell
  unmöglich.
- **Gemeinsames PVC** (`ReadWriteMany`) für `FIREFLY_SNAPSHOT_PATH` —
  ohne das übernimmt der Standby mit leerem Bild.
- **Deployments (Restart by design)** — die HA.2b-Demotion beendet die
  weichende Seite mit Exit-Code 3 und verlässt sich auf den Neustart.
- **`strategy: Recreate`** — ein Rolling Update mit zwei gleichzeitigen
  Mains wäre ein selbstgebautes Split Brain.
- **Ein Service über beide Deployments, Routing per Readiness** — der
  Standby antwortet auf `/ready` mit 503 und bekommt nie Traffic; nach
  einem Failover wandert der Traffic ohne Eingriff mit.
- **`hostNetwork: true` + Pflicht-Anti-Affinity** — UDP-Multicast ist in
  Standard-CNI-Pod-Netzen **nicht** selbstverständlich; der Default folgt
  ADR 0017 (abgeschottetes Betriebsnetz). Gleicher Host-Port + gleiche
  Multicast-Sockets heißt: das Paar darf nie auf demselben Knoten landen.
  Alternative: `hostNetwork: false` setzen und selbst ein
  multicast-fähiges Zweitnetz (z. B. Multus) bereitstellen — das Chart
  tut nicht so, als wäre das gelöst.
- **Non-root, read-only rootfs, keine Capabilities** — der Prozess
  schreibt nur aufs Snapshot-Volume (fsGroup) und nach stdout.

## Ehrliche Grenzen

- `validate.sh` prüft Syntax und (mit Helm) Lint/Render — **kein Ersatz**
  für einen Smoke-Test im echten Cluster (Multicast-Reichweite, RWX-
  Storage-Klasse, Registry-Zugriff sind umgebungsspezifisch).
- Das Image (siehe `../Dockerfile`, `../DOCKER.md`) muss in eine für den
  Cluster erreichbare Registry gebracht und in den Values gesetzt werden.
- Monitoring (Prometheus-Scrape auf `/metrics`, Grafana) ist bewusst
  nicht Teil des Charts — siehe `../monitoring/`.
