# HA.3 — Kubernetes-Deployment (Helm-Chart + Manifeste)

> **Anforderung:** NFR-OPS-002 · **ADR:** — (Umsetzung von ADR 0040/0041;
> keine neue Weichenstellung) · **ICD:** unberührt · **Einstufung:** S3 ·
> umgesetzt auf Fable 5 (Roadmap-Empfehlung: Sonnet)

## Fachlich

Snapshot-Volume, Main/Standby-Paar, Restart-Policy, Probes — alle
HA-Fähigkeiten existierten bisher nur als Env-Variablen und Doku-Prosa.
Wer Firefly betreiben wollte, baute sich das Deployment selbst und konnte
dabei genau die Fehler machen, vor denen die Doku warnt (kein gemeinsames
Volume, keine Restart-Policy, abweichende Konfiguration auf dem Standby).
Jetzt gibt es das **fertige, geprüfte Rezept**: `helm install firefly
deploy/helm/firefly` (oder `kubectl apply -f deploy/kubernetes/firefly.yaml`)
bringt ein korrekt verdrahtetes Main/Standby-Paar hoch. Manifeste im
Repo sind zugleich Konfigurationsmanagement im Sinne von ADR 0004.

## Technik — was das Rezept strukturell erzwingt

| Baustein | Warum so |
|----------|----------|
| Eine ConfigMap für **beide** Instanzen | Identische Konfiguration ist ADR-0041-Voraussetzung (HA.1-Fingerprint); geteilte Map = Drift unmöglich |
| Gemeinsames PVC (`ReadWriteMany`) | Der Standby übernimmt mit dem letzten Snapshot — ohne Volume mit leerem Bild |
| Deployments, `strategy: Recreate` | Restart by design (HA.2b-Demotion endet mit Exit-Code 3); Rolling Update zweier Mains wäre selbstgebautes Split Brain |
| **Ein** Service über beide Deployments | Routing per Readiness: der Standby antwortet 503 auf `/ready` → Traffic erreicht immer genau die aktive Instanz, auch durch einen Failover hindurch — ohne Eingriff |
| `hostNetwork` + Pflicht-Anti-Affinity | UDP-Multicast ist in Standard-CNI-Netzen nicht selbstverständlich (ehrlicher Default per ADR 0017); gleicher Host-Port ⇒ das Paar darf nie einen Knoten teilen |
| non-root 65532, read-only rootfs, Caps gedroppt | Der Prozess schreibt nur aufs Snapshot-Volume (fsGroup) und nach stdout |

Dazu: Secret-Muster (`existingSecret` bevorzugt, Inline-Secrets nur fürs
Labor), Liveness `/health` / Readiness `/ready`, Graceful-Shutdown 20 s,
Ressourcen-Requests/-Limits, `NOTES.txt` mit Prüf-Kommandos.
`deploy/validate.sh` prüft die YAML-Syntax überall und führt — wo Helm
existiert — `helm lint` plus Voll-Render (Default- und Degraded-Shapes:
Standby/Snapshot deaktiviert) aus.

## Ehrliche Grenzen

- **Kein Helm in der Entwicklungs-Sandbox:** `helm lint`/`template` konnte
  in dieser Umgebung nicht laufen (Netz-Policy); die YAML-Syntax der
  statischen Manifeste ist geprüft, der Helm-Teil von `validate.sh` ist
  für CI/Betreiber-Maschine gedacht und dort nachzuholen.
- **Kein Cluster-Smoke-Test im Repo:** Multicast-Reichweite,
  RWX-Storage-Klasse und Registry-Zugriff sind umgebungsspezifisch — der
  erste echte `helm install` gehört in die Abnahme des Zielclusters.
- Monitoring (Prometheus-Scrape auf `/metrics`, Grafana) bleibt bewusst
  außerhalb des Charts (`monitoring/`).
- Koppelt an Wayfinders ORCH-6 (deren Orchestrator-Deployment) — dort
  nicht Teil dieses Häppchens.
