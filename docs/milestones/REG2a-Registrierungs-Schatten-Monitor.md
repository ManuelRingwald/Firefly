# REG.2a — Sensor-Registrierung: Online-Schatten-Monitor

> **Anforderung:** FR-TRK-038 · **ADR:** 0034 ·
> **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich: Warum ein Schattenmodus?

REG.1 hat bewiesen, dass Fireflys Bias-Schätzer aus Korrespondenzen die
injizierten Radar-Fehler zurückgewinnt — **offline, auf konstruierten Daten**.
Bevor die Schätzung in die Fusion zurückfließen darf (REG.2b), muss sie sich
auf **echten Live-Daten** beweisen: Konvergiert sie? Ist sie stabil über
Stunden, oder springt sie mit jedem Verkehrsmix? Wie oft ist die Geometrie
beobachtbar? Genau dafür ist der Schattenmodus da: Der Monitor läuft im
Live-Server mit, schätzt laufend — und **wendet nichts an**. Seine Ausgabe
sind Logs und Metriken, an denen der Operator die Vertrauensfrage beantwortet,
bevor ein Regelkreis in den sicherheitsrelevanten Fusionspfad geschlossen
wird. (ARTAS betreibt seine Registrierung genauso als eigenständige,
überwachbare Funktion.)

## Technik

**`RegistrationMonitor`** (`firefly-track::registration`, rein/testbar):

- **Gleitendes Datenzeit-Fenster** (Default 120 s): identitäts-tragende,
  registrierungs-nutzbare Plots — Radar-Polarmessungen nur von den
  konfigurierten Radaren, geodätische Selbstreports (ADS-B/FLARM) als
  bias-freie Referenz. Alles andere wird gar nicht erst gepuffert.
- **Schätz-Kadenz** (Default 10 s Datenzeit): `correspondences_by_identity`
  über das Fenster, dann `estimate_biases` (REG.1, SVD). Läufe mit weniger
  als `min_correspondences` (Default 20) werden **abgelehnt** — lieber keine
  Schätzung als eine verrauschte.
- **Datenzeit-getrieben** (ADR 0003): kein Wanduhr-Zugriff; Replay derselben
  `.ffplots`-Datei reproduziert dieselben Schätzungen.

**Server-Verdrahtung** (`firefly-server`):

- `LiveTracker::with_registration` hängt den Monitor an; `ingest` ruft
  `observe` **nach** der Tracker-Verarbeitung auf — der Monitor kann das
  Lagebild dieser Batch prinzipbedingt nicht beeinflussen (im Test belegt:
  identische Snapshots mit/ohne Monitor).
- **Opt-in:** `FIREFLY_REGISTRATION_ENABLED` (`1`/`true`/`yes`); ohne
  Radar-Quelle ein dokumentierter No-op mit Warn-Log.
- **Observability:** je frischer Schätzung ein `info`-Log (Paare, RMS
  vor/nach, Beobachtbarkeit, Biases je Sensor); pro Ausgabe-Tick werden
  `firefly_registration_estimates_total`, `firefly_registration_correspondences`,
  `firefly_registration_observable` und die gelabelten Gauges
  `firefly_registration_bias_range_m{sensor=…}` /
  `firefly_registration_bias_azimuth_deg{sensor=…}` aktualisiert (letztere
  erscheinen erst nach der ersten Schätzung — keine irreführenden 0-Biases).

## Ehrliche Grenzen (REG.2a)

- **Keine Korrektur** — die Fusion sieht weiterhin die rohen Messungen. Die
  Anwendungs-Politik (wann ist eine Schätzung gut genug? geglättete
  Übergänge?) ist REG.2b.
- **Feste Parameter** — Fenster/Kadenz/Mindest-Paare sind Konstanten der
  `RegistrationConfig`-Defaults; env-Tuning erst, wenn der Betrieb zeigt,
  dass es nötig ist.
- Die REG.1-Grenzen (kein Zeit-Offset, ungewichtet) gelten unverändert.

## Tests

`firefly-track`: Monitor gewinnt injizierte 150 m/0,3° aus dem laufenden
Strom zurück; Kadenz wird eingehalten; stale Plots werden verdrängt und
dünne Evidenz abgelehnt. `firefly-server`: Schattenmodus verändert das
Lagebild nicht (Snapshot-Gleichheit); Flag-Parsing; Metrik-Rendering inkl.
Abwesenheit der Bias-Gauges ohne Schätzung. Gates: `cargo test --workspace`,
`clippy`, `fmt` grün. Kein Wire-/ICD-Bezug — Wayfinder unberührt.
