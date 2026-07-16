# SAFE.4 — Tracker-Fortschritts-Watchdog (FHA-Lücke L1)

> **Anforderung:** FR-OPS-009 · **ADR:** — · **ICD:** 3.7.1 (dokumentarisch,
> kein Wire-Format-Bezug) · **Einstufung:** S2–S3 · umgesetzt auf Fable 5
> (Roadmap-Empfehlung: Sonnet) · **Auslöser:** FHA H-F1-02 (ASSUR.1),
> Betreiber-Entscheid „Finding direkt lösen" (2026-07-16)

## Fachlich

Die FHA fand genau eine echte Code-Lücke: Der CAT065-Heartbeat lief
**unabhängig vom Tracker**. Wäre der Tracker-Task hängen geblieben, hätte
Firefly weiter im Sekundentakt „ich lebe" gesendet — mit eingefrorenem
Lagebild dahinter. Für jeden Konsumenten (Wayfinder-Feed-Banner,
Standby-Heartbeat-Wache!) hätte der Dienst gesund ausgesehen: die Klasse
**„irreführend unerkannt"**, vor der die FHA warnt. Besonders tückisch:
Auch der **Standby hätte nicht übernommen**, denn er wacht genau über
diesen Heartbeat.

Jetzt prüft der Heartbeat **vor jedem Senden**, ob der Tracker noch
Output-Ticks macht. Bleiben sie länger als 3 Ausgabeperioden aus, meldet
der Heartbeat ehrlich **NOGO/degradiert** — Konsumenten sehen den
degradierten Dienst, und (Folgewirkung der bestehenden HA.2-Logik) ein
wachender Standby, der nur *verstummte* Heartbeats als Ausfall wertet,
bekommt zumindest das ehrliche Signal auf dem Draht.

## Technik

- **`firefly-multicast::run_heartbeat`**: neuer `operational()`-Callback,
  vor **jedem** Heartbeat gefragt; `false` ⇒ `encode_status(…, false)` ⇒
  NOGO `0x40` in I065/040 (Encoder konnte das seit ADR 0018, wurde nur
  nie angesteuert).
- **`firefly-server::tracker_progress_stalled(last_tick, threshold, now)`**
  (rein, unit-getestet): beißt nur, wenn Schwelle **und** erster Tick
  gesetzt sind und die Stille **strikt** über der Schwelle liegt;
  `saturating_sub` gegen Uhr-Rücksprung.
- **Verdrahtung:** `on_tick` stempelt `tracker_last_tick_unix_s`; die
  Live-Verdrahtung armiert `tracker_watchdog_threshold_s = max(3 ×
  Ausgabeperiode, 3 s)` (dieselbe „drei verpasste Takte"-Konvention wie
  der Failover-Timeout). Übergänge: ERROR-Log beim Anschlagen, INFO bei
  Erholung, Gauge `firefly_heartbeat_degraded`.
- **Keine Konfiguration** — die Schwelle folgt der Ausgabeperiode; ein
  falsch setzbarer Knopf würde die Ehrlichkeit des Status aushebeln.

## Tests

- `live::tracker_progress_watchdog_bites_only_after_progress_then_silence`
  — unarmed vor Schwelle/erstem Tick; exakt an der Schwelle gesund;
  strikt darüber degradiert; Uhr-Rücksprung ≠ degradiert.
- `heartbeat::degraded_answer_sets_nogo_on_the_wire` — ein `false` des
  Callbacks erreicht als NOGO das dekodierte Datagramm (Draht-Wirkung,
  nicht nur interner Zustand).
- Bestand `heartbeats_are_sent_and_decodable` unverändert grün
  (operationeller Normalfall); `metrics::render_includes_all_metrics`
  deckt die neue Gauge ab.

## Ehrliche Grenzen

- Der Watchdog erkennt **ausbleibende Output-Ticks** — die adressierte
  Gefährdung (hängender/blockierter Tracker-Task). Einen Task, der
  *tickt, aber Unsinn rechnet*, erkennt er nicht (dafür: HA.4-Messstand,
  Regression-Gates, COMPASS).
- In der **Startphase** (vor dem ersten Tick) bleibt der Heartbeat
  operationell — bewusst: „noch nicht gestartet" ist über `/ready`
  sichtbar und nicht die Gefährdung dieses Watchdogs.
- Der wachende **Standby** wertet heute nur *verstummte* Heartbeats als
  Übernahme-Signal; ein degradiert-sendender Main löst keine Promotion
  aus. Das ist die dokumentierte ADR-0041-Semantik (Timeout-Detektion,
  kein Konsens) — ob NOGO-Heartbeats eine Promotion auslösen sollen, wäre
  eine eigene, anzukündigende Verhaltensänderung.
