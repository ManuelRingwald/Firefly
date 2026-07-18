# Firefly Monitoring-Paket (MON.1, NFR-OBS-004)

**Bestands-Tooling, kein Eigenbau** (Betreiber-Entscheid 2026-07-16):
Firefly instrumentiert sich selbst (`/metrics`, `/status`, CAT065/063);
Sammeln, Alarmieren und Anzeigen übernehmen Prometheus, Alertmanager und
Grafana. Dieses Verzeichnis liefert die **Inhalte** dafür:

| Datei | Zweck |
|-------|-------|
| `prometheus/alerts.yaml` | **Kanonische** Alarmregeln (12 Alarme, 3 Schweregrade, jede mit Runbook-Link hierher) |
| `kubernetes/prometheusrule.yaml` | Dieselben Regeln als PrometheusRule-CRD (Prometheus-Operator/kube-prometheus-stack) — **generierte Verpackung**, Quelle bleibt `alerts.yaml` |
| `grafana/dashboard.json` | Betriebs-Dashboard in fünf Blöcken: Feed & Dienst · Quellen & Sensoren · Tracker · Hochverfügbarkeit · Korrelation |
| `validate.sh` | Syntax-Checks + Konsistenz Regeln ↔ CRD-Verpackung (+ `promtool`, wo vorhanden) |

**Einbindung:** Datei-Prometheus → `rule_files: ["…/alerts.yaml"]`;
Operator-Stack → `kubectl apply -f kubernetes/prometheusrule.yaml`
(Label `release: prometheus` an den eigenen `ruleSelector` anpassen);
Scrape → `prometheus.io/*`-Annotations der Pods (HA.3) oder der
ServiceMonitor im Helm-Chart (`monitoring.serviceMonitor.enabled=true`).
Dashboard → Grafana-Import von `grafana/dashboard.json`.

**Regeln ändern:** immer in `prometheus/alerts.yaml`, dann die
CRD-Verpackung neu erzeugen (Python-Einzeiler unten) und `./validate.sh`
laufen lassen — der Check schlägt bei Drift fehl.

```bash
python3 - <<'EOF'
import yaml
src = yaml.safe_load(open('prometheus/alerts.yaml'))
crd = yaml.safe_load(open('kubernetes/prometheusrule.yaml'))
crd['spec']['groups'] = src['groups']
open('kubernetes/prometheusrule.yaml','w').write(
    open('kubernetes/prometheusrule.yaml').read().split('apiVersion:')[0]
    + yaml.safe_dump(crd, allow_unicode=True, sort_keys=False, width=100))
EOF
```

---

## Runbooks — Alarm → Bedeutung → Handgriff

Alarm ohne Handgriff ist nur Lärm. Für jeden Alarm: was er **bedeutet**,
was **zuerst** zu tun ist, und wo es weitergeht. `$HOST` = die
Firefly-Instanz, Token nur nötig bei gesetztem `FIREFLY_WS_TOKEN`.

### FireflyAbsent

- **Bedeutung (critical):** Prometheus sieht **keine** Firefly-Metriken
  mehr — Instanz weg, Scrape-Pfad tot oder Netz-Problem. Der Feed-Zustand
  ist unbekannt = operativ wie „tot" behandeln.
- **Handgriff:** ① `kubectl get pods -l app.kubernetes.io/name=firefly`
  (läuft überhaupt etwas? Restart-Loop?) ② `curl $HOST:8080/health` direkt
  gegen den Pod ③ Scrape-Konfig/ServiceMonitor prüfen. Parallel bei
  Wayfinder gegenprüfen: zeigt das ASD ein Feed-Banner? Wenn Wayfinder den
  Feed **noch sieht**, ist nur die Monitoring-Kette kaputt — Priorität
  bleibt hoch (Blindflug), aber das Lagebild steht.

### FireflyHeartbeatSilent

- **Bedeutung (critical):** Die **aktive** Instanz sendet keine
  CAT065-Heartbeats — alle Konsumenten sehen einen toten Feed; ein
  wachender Standby wird nach dem Failover-Timeout übernehmen (HA.2).
- **Handgriff:** ① Abwarten prüfen: hat der Standby schon übernommen
  (`FireflyFailover` gefeuert, `firefly_role` gewechselt)? Dann → Runbook
  FireflyFailover. ② Wenn kein Standby existiert:
  `curl $HOST:8080/status -H 'Authorization: Bearer <token>'` und Logs
  (`kubectl logs …`) — Sende-Socket-Fehler? ③ Prozess-Neustart ist sicher
  (HA.1-Snapshot stellt das Bild wieder her; Startup-Arbitrierung
  verhindert Doppel-Sender).

### FireflyTrackerStalled

- **Bedeutung (critical):** Der SAFE.4-Watchdog hat angeschlagen — der
  Tracker-Task macht keine Output-Ticks mehr, der Heartbeat meldet
  ehrlich NOGO. Das Bild dahinter friert ein.
- **Handgriff:** ① Logs auf den ERROR `tracker output ticks stopped`
  und die letzten Meldungen davor prüfen (Deadlock? Panik in einem
  Task?). ② `curl $HOST:8080/status`: steigt `plots_ingested_total`
  noch? ③ Neustart (sicher, s. o.) — und den Vorfall mit
  `FIREFLY_PLOT_RECORD_PATH`-Aufzeichnung (falls aktiv) reproduzierbar
  melden: das ist ein Kern-Bug, `.ffplots` + Logs sichern!

### FireflySensorOutage

- **Bedeutung (warning):** Mindestens ein Sensor liefert seit
  > 2,5 × Scan-Periode keine Plots — das Bild wird still dünner
  (FHA H-F4-02).
- **Handgriff:** ① `curl $HOST:8080/sensors -H '…'` — **welcher** Sensor,
  und steht dort ein Grund? (CAT063 `SRC-REASON`: `unreachable` = Netz/
  Firewall, `auth` = Credentials, `rate_limited` = Drosselung.) ② Je nach
  Grund: Netzpfad/Firewall, Cred-Env der Quelle (`FIREFLY_SOURCES`), oder
  einfach warten (Backoff). ③ Liefert ein Sensor dauerhaft **Müll** statt
  nichts: `POST /sensors/{id}/disable` nimmt ihn aus der Fusion (SRV.2).

### FireflySnapshotStale

- **Bedeutung (warning):** Der letzte erfolgreiche Zustands-Snapshot ist
  > 60 s alt (Soll: alle 10 s) — das Verlustfenster bei Neustart/Failover
  wächst über das Ausgelegte hinaus.
- **Handgriff:** ① Meist zusammen mit `FireflySnapshotWriteFailing` →
  dorthin. ② Sonst: hängt der Output-Tick (→ FireflyTrackerStalled)?
  Snapshots werden im Tick geschrieben.

### FireflySnapshotWriteFailing

- **Bedeutung (warning):** Snapshot-Schreibfehler — das Lagebild läuft
  weiter (Verfügbarkeit vor Persistenz), aber die Wiederanlauf-Absicherung
  erodiert; es wird automatisch weiter versucht.
- **Handgriff:** ① Volume voll? `df` auf dem Snapshot-PVC/Pfad
  (`FIREFLY_SNAPSHOT_PATH`). ② PVC-Zustand (`kubectl get pvc`),
  RWX-Mount auf beiden Instanzen? ③ Nach Behebung verschwindet der Alarm
  selbst (Fehlerzähler stoppt, Age sinkt beim nächsten Write).

### FireflyBackpressureLoss

- **Bedeutung (warning):** Der Quell→Tracker-Kanal lief voll, Plot-Batches
  wurden verworfen — echter Datenverlust unter Überlast. Bei unseren
  gemessenen Reserven (> 1500× Echtzeit, TECHNICAL §11) ist das ein
  Anomalie-Signal, kein erwarteter Zustand.
- **Handgriff:** ① CPU-Drosselung prüfen (K8s-Limits, `kubectl top pod`).
  ② `FireflyJpdaCapEngaged` zeitgleich? Extrem dichter Verkehr → Kappe
  arbeitet, trotzdem Kapazität prüfen. ③ Quell-Rate plausibel (Amok
  laufende Quelle → ggf. Sensor-Gate)?

### FireflyFailover

- **Bedeutung (warning):** Ein Standby hat übernommen — der Schutz hat
  **funktioniert**, aber der alte Main ist aus einem Grund verstummt, der
  geklärt gehört.
- **Handgriff:** ① Lage bestätigen: genau eine aktive Instanz
  (`firefly_role`), Bild da? ② Logs/Exit-Code des alten Main: OOM? Crash?
  Exit 3 = Split-Brain-Demotion (dann: was hat die Partition verursacht?).
  ③ Alten Pod als neuen Standby verifizieren (`/ready` = 503 „standby").
  Verlustfenster: ≤ Snapshot-Periode + Failover-Timeout (ADR 0041).

### FireflyCat062SendErrors

- **Bedeutung (warning):** Der Multicast-Sendepfad wirft Fehler —
  Konsumenten bekommen Scans lückenhaft oder gar nicht.
- **Handgriff:** ① Logs (`failed to send live CAT062 data block`):
  Fehlercode? ② Netz-Interface/hostNetwork des Pods, Multicast-Route
  (`ip route`, TTL). ③ Mit `tcpdump`/Wireshark auf der Gruppe gegenprüfen
  (Verfahren: `docs/verification/compass-gegen-check.md` §PCAP).

### FireflyJpdaCapEngaged

- **Bedeutung (info):** Sehr dichter Pulk — die CAP.2-Kappe hat Cluster
  > 8 Tracks/10 Plots auf Pro-Track-PDA degradiert: Echtzeit gehalten,
  Zuordnung dort gröber. **Erwartetes Verhalten.**
- **Handgriff:** Kenntnisnahme; bei Dauerfeuer Verkehrsbild ansehen
  (echter Pulk oder Clutter-Sturm? → ggf. Quelle prüfen/gaten).

### FireflySensorGateInForce

- **Bedeutung (info):** Ein Betreiber-Gate (SRV.2) nimmt seit > 1 h
  Sensoren aus der Fusion. Absichtlich flüchtig — dieser Alarm ist die
  Erinnerung, dass der Eingriff noch aktiv ist.
- **Handgriff:** `GET /sensors` — Grund noch gegeben? Sonst
  `POST /sensors/{id}/enable`.

### FireflySourceRateLimited

- **Bedeutung (info):** OpenSky/Community-Aggregator drosseln (HTTP 429);
  der Backoff dehnt die Poll-Intervalle — das Bild aktualisiert langsamer,
  Daten gehen nicht verloren.
- **Handgriff:** Bei Dauerzustand: OpenSky-Kontingent/Credentials prüfen
  (`FIREFLY_OPENSKY_CLIENT_ID`), Poll-Intervall der Quelle erhöhen
  (`poll_interval_secs` in `FIREFLY_SOURCES`).

---

## Ehrliche Grenzen

- **Validierung hier = Syntax + Konsistenz.** `promtool check rules` und
  der Grafana-Import selbst laufen in der Betreiber-/CI-Umgebung
  (`validate.sh` nutzt `promtool`, wo vorhanden — in der
  Entwicklungs-Sandbox ist es nicht installierbar, wie Helm in HA.3).
- **Schwellen sind Startwerte** aus Design-Konstanten (Heartbeat 1 s,
  Snapshot 10 s, Failover 3 s) und den CAP-Messungen — nach den ersten
  Betriebswochen gegen echte Baselines kalibrieren.
- **Kein externer Watchdog:** Monitoring aus Konsumenten-Sicht leistet
  heute Wayfinder (`wayfinder_feed_stale`). Ein eigener Feed-Watchdog
  (für Betrieb ohne Wayfinder) bleibt eine offene Design-Frage und
  bekäme **erst ein Design-Konzept, dann Code** (Betreiber-Vorgabe
  2026-07-16).
- **Log-Aggregation** ist Rezept, nicht Paket: die Logs sind strukturiertes
  JSON (TECHNICAL §2) — Loki/ELK-Anbindung siehe TECHNICAL §2.5.
