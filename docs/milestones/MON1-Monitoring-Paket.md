# MON.1 — Monitoring-Paket: Alarmregeln, Runbooks, Dashboard-Neubau

> **Anforderung:** NFR-OBS-004 · **ADR:** — (Bestands-Tooling-Entscheid des
> Betreibers 2026-07-16, im Chat dokumentiert; kein Architektur-Sprung) ·
> **ICD:** unberührt · **Einstufung:** S2–S3 · umgesetzt auf Fable 5
> (Roadmap-Empfehlung: Sonnet) · **Schließt FHA-Lücke L4.**

## Fachlich

Firefly konnte sich bislang **messen, aber nicht melden**: ~50 Metriken,
Heartbeat, Sensor-Status — aber fällt nachts um drei ein Radar aus, schaut
niemand auf `firefly_sensors_active`. Die FHA hielt das als Lücke L4 fest
(„Detektions-Barrieren alarmieren niemanden aktiv"). MON.1 schließt sie —
mit **Bestands-Tooling** (Prometheus/Alertmanager/Grafana/Loki), nicht mit
Eigenbau: Firefly liefert die *Inhalte* (Regeln, Handgriffe, Dashboard),
die Werkzeuge liefert der Stack.

## Technik (alles unter `monitoring/`)

- **12 Alarmregeln** (`prometheus/alerts.yaml`, kanonisch), drei
  Schweregrade: **critical** (FireflyAbsent · HeartbeatSilent — mit
  `firefly_role`-Guard, damit der bewusst stumme Standby keinen Fehlalarm
  erzeugt · TrackerStalled = SAFE.4), **warning** (SensorOutage,
  SnapshotStale/WriteFailing, BackpressureLoss, Failover,
  Cat062SendErrors), **info** (JpdaCapEngaged, SensorGateInForce,
  SourceRateLimited). Jede Regel: begründete Schwelle + Runbook-Link.
- **Runbooks** (`README.md`): je Alarm *Bedeutung → erster Handgriff →
  Weiterführung*, mit konkreten Befehlen (inkl. der SRV.2-Kommandos).
- **Dashboard-Neubau** (`grafana/dashboard.json`): 25 Panels in fünf
  Blöcken (Feed & Dienst · Quellen & Sensoren · Tracker · HA ·
  Korrelation) — ersetzt die 5 SDPS-006-Panels. Statuskacheln mit
  Klartext-Mappings (OPERATIONELL/NOGO, AKTIV/STANDBY) und
  Ampel-Schwellen (z. B. Snapshot-Alter 30/60 s).
- **Verdrahtung:** `kubernetes/prometheusrule.yaml` (CRD-Verpackung,
  **generiert** aus der kanonischen Quelle), Opt-in-**ServiceMonitor** im
  Helm-Chart (`monitoring.serviceMonitor.enabled`, Default aus — Cluster
  ohne Operator-CRD dürfen nicht am Install scheitern), Loki/ELK-Rezept
  in TECHNICAL §2.5 (die Logs sind schon JSON).
- **`validate.sh`:** Syntax (YAML/JSON), **Gleichheit** CRD ↔ kanonische
  Regeln (Drift = Fehler), Schwere-/Runbook-Pflicht je Regel; `promtool`
  wo vorhanden (Sandbox: dokumentierter SKIP, HA.3-Muster).

## Ehrliche Grenzen

- **Aktivierung ist ein Betreiber-Schritt:** Regeln laden, Dashboard
  importieren, Alertmanager-Empfänger (Mail/Chat/Pager) konfigurieren —
  im Repo liegt der geprüfte Inhalt, nicht der laufende Alarm.
- **Schwellen sind Startwerte** aus Design-Konstanten (Heartbeat 1 s,
  Snapshot 10 s, Failover 3 s) und CAP-Messungen — nach den ersten
  Betriebswochen gegen echte Baselines kalibrieren.
- **Sandbox-Validierung = Syntax + Konsistenz**; `promtool check rules`
  und der Grafana-Sichtlauf gehören in CI/Betreiber-Umgebung.
- **Kein externer Watchdog:** Konsumenten-Sicht-Monitoring leistet heute
  Wayfinder (`wayfinder_feed_stale`); ein eigener Feed-Watchdog bliebe
  eine offene Design-Frage und bekäme erst ein Design-Konzept + Freigabe
  (Betreiber-Vorgabe 2026-07-16).
