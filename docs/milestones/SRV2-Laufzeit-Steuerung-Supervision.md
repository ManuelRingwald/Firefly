# SRV.2 — Laufzeit-Steuerung (Sensor an/aus) + Supervision-Übersicht

> **Anforderung:** FR-OPS-008 · **ADR:** — (Ausführung der in ADR 0042
> festgeschriebenen Supervision-Linie) · **ICD:** unberührt ·
> **Einstufung:** S3 · umgesetzt auf Fable 5 (Roadmap-Empfehlung: Sonnet)

## Fachlich

Bisher konnte der Betreiber Firefly zur Laufzeit **beobachten** (Metriken,
Probes, CAT063), aber nicht **eingreifen**. Wenn eine Quelle Ärger machte —
ein Radar liefert nach einem Defekt systematisch falsche Positionen, eine
ADS-B-Quelle flutet das Bild mit Müll — blieb nur der Holzhammer: Instanz
mit geänderter Quell-Konfiguration neu starten, mit Lagebild-Unterbrechung
für alle. Beim Vorbild ARTAS ist genau das eine Kernfunktion der
Supervision (CMD): Der Operator nimmt einen Sensor per Kommando aus der
Fusion und holt ihn wieder herein, ohne dass das System stehen bleibt.

SRV.2 baut das nach: eine kleine, abgesicherte **Kommando-Schnittstelle**
(„Sensor X aus / wieder an") plus eine **Status-Übersicht auf einen
Blick** (`GET /status`) — der dokumentierte SNMP-/CMD-Ersatz aus der
Roadmap-Messlatte (Cloud-Observability + Steuerungs-API).

## Technik

### Sensor-Gate (`firefly-server/src/live.rs`)

- `SensorGate = Arc<Mutex<BTreeSet<SensorId>>>` — geteilt zwischen
  HTTP-Kommando-Kante (schreibt) und `LiveTracker::ingest` (filtert).
- Der Filter sitzt **ganz vorn** im Ingest: Plots deaktivierter Sensoren
  erreichen weder die `.ffplots`-Aufzeichnung noch den
  Registrierungs-Monitor noch den Tracker — der Sensor ist vollständig
  aus der Fusion, nicht bloß heruntergewichtet. Verworfene Plots werden
  gezählt (`plots_dropped_disabled`).

### Kommando-API (`firefly-server/src/app.rs`)

| Endpunkt | Wirkung |
|----------|---------|
| `GET /sensors` | Inventar: `[{sensor_id, kind, active, disabled}]` (Liveness aus dem CAT063-`SensorHealthMonitor`) |
| `POST /sensors/{id}/disable` | Sensor aus der Fusion (422 unbekannte ID, 409 auf Standby); idempotent, meldet `changed` |
| `POST /sensors/{id}/enable` | Sensor wieder herein (idempotent) |
| `GET /status` | Supervision-JSON: Rolle, Readiness, Restore, Failover-/Track-/Plot-Zähler, Sensoren (gesamt/aktiv/deaktiviert/verworfene Plots), Korrelation, Snapshot-Buchführung, JPDA-Kappen-Zähler |

Auth exakt wie die FPL.2-Korrelations-Kommandos: `FIREFLY_WS_TOKEN` als
`Authorization: Bearer`-Header **only** (kein Query-Fallback — Query-
Strings landen in Logs); Origin-Check nur bei mitgesendetem
`Origin`-Header. Auch `GET /status` ist token-gated (Betriebsdaten).

### Sichtbarkeit

- Gauge `firefly_sensors_disabled` (folgt jedem Kommando), Counter
  `firefly_sensor_disabled_plots_dropped_total` (via `on_tick`, 8.
  Parameter).
- Disable erzeugt ein **WARN**-Log (das Bild ist ab jetzt dünner), Enable
  ein INFO-Log.

## Bewusste Auslegung (und warum)

- **Flüchtig, fail-open:** Neustart/Failover startet mit allen Sensoren
  aktiv. Die Alternative — das Gate im HA.1-Snapshot zu persistieren —
  hätte den schlimmeren Fehlermodus: ein vor Monaten gesetztes,
  vergessenes Gate dünnt das Lagebild **still** aus. Ein nach Neustart
  wieder aktiver Störsensor fällt dagegen sofort auf (WARN + Metrik) und
  ist mit einem Kommando erneut deaktiviert.
- **Pro Instanz:** Der Standby übernimmt das Gate nicht (gleiche Logik).
- **CAT063 bleibt Quell-Wahrheit:** Der Draht meldet weiterhin, ob die
  Quelle *Daten liefert* — ein manuell deaktivierter Sensor sendet ja
  weiter und bleibt dort „active". Den Gate-Zustand in CAT063 zu spiegeln
  hieße, die ICD-Semantik („liefert die Quelle?") still umzudeuten;
  stattdessen ist er in `/sensors`, `/status` und den Metriken sichtbar.
- **Kappen nicht konfigurierbar, Kommandos nicht persistent** — beide
  Entscheidungen halten den sicherheitskritischen Pfad frei von
  schleichendem, unsichtbarem Zustand.

## Tests

- `live::sensor_gate_drops_and_resumes` — das Gate beißt: deaktivierter
  Sensor erzeugt keinen Track, `plots_ingested` bleibt 0, Drop-Zähler
  wächst; nach Enable läuft die Fusion ohne Neustart weiter.
- `app::sensor_commands_disable_list_and_enable` — voller Zyklus über
  HTTP inkl. Gauge und Idempotenz (`changed`).
- `app::sensor_commands_validate_inventory_and_configuration` — 422 bei
  unbekannter ID, 409 ohne Inventar (Standby), Reads nie ein Fehler.
- `app::sensor_commands_require_the_bearer_token` — 401 ohne Token, kein
  Query-Fallback, `/status` ebenfalls token-gated.
- `app::status_reports_role_sensors_and_gates` — Rolle/Readiness/
  Gate-Liste im Status-JSON, Standby meldet sich als solcher.
- `metrics::render_includes_all_metrics` — beide neuen Zeilen gerendert.

## Ehrliche Grenzen

- **Kein Config-Reload:** Das Gate schaltet konfigurierte Sensoren stumm,
  es kann keine **neuen** Quellen anlegen oder Endpunkte/Credentials
  ändern — das bleibt ein Neustart mit geänderter `FIREFLY_SOURCES`
  (bzw. Orchestrator-Sache, Wayfinder ADR 0012).
- **Polled-Quellen pollen weiter:** Ein deaktivierter OpenSky-/
  Aggregator-Sensor wird weiter gepollt (Rate-Limit-Budget läuft weiter);
  verworfen wird am Ingest. Das Abschalten des Adapters selbst wäre ein
  Folge-Häppchen, falls betrieblich nötig.
- **Kein Audit-Trail über Logs hinaus:** Wer wann geschaltet hat, steht
  im strukturierten Log (WARN/INFO), nicht in einer Historie-API.
