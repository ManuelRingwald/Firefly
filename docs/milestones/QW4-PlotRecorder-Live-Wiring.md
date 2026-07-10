# QW.4 — PlotRecorder im Live-Pfad verdrahtet

> **Anforderung:** FR-OPS-006 (Live-Verdrahtung abgeschlossen) ·
> **ARTAS-Roadmap:** QW.4 (letztes Quick-Win-Häppchen, Vorstufe SDPS-002/HA) ·
> **Einstufung:** S2 · Umsetzung auf Opus 4.8

## Fachlich: Warum?

Der `.ffplots`-Eingangs-Recorder (ADR 0020) zeichnet den **Plot-Strom auf, der
in den Tracker eintritt** — quellenagnostisch (ADS-B/Radar/FLARM über denselben
`Plot`-Typ). Ihn wiederzugeben reproduziert einen Produktions-Lauf exakt: die
Grundlage, um einen Tracker-Fehler aus dem Betrieb nachzustellen **und** — der
eigentliche ARTAS-Bezug — der Wiederanlauf-Weg nach einem Neustart (Vorstufe zu
SDPS-002/HA, dem größten offenen Betriebs-Gap der Gap-Analyse).

Der Recorder-Code und seine Verdrahtung in `LiveTracker::ingest` existierten
bereits und waren unit-getestet — aber der **Live-Server** übergab
`LiveTracker::new(tracker, None)` (`main.rs`), zeichnete also im echten Betrieb
**nichts** auf. Der Kommentar „recorder wired in AP9.4c-4" war veraltet
(stale). QW.4 schließt diese Lücke: aus „unit-getestete Fähigkeit" wird
„im Betrieb nutzbar".

## Technisch

- **Neue opt-in-Env `FIREFLY_PLOT_RECORD_PATH`.** `main.rs` liest sie und ruft
  `resolve_plot_recorder(path)`:
  - unset / leer / nur Whitespace → `None` (kein Recording, der Default —
    kein Überraschungs-Schreiben ins Dateisystem).
  - gesetzter, öffenbarer Pfad → `PlotRecorder::create` (schreibt den
    `.ffplots`-Header sofort), `Some(recorder)` an `LiveTracker::new`.
  - gesetzter, **nicht öffenbarer** Pfad (fehlendes Verzeichnis, keine Rechte)
    → **nicht-fatal**: Warn-Log, `None` — der Server startet und trackt weiter.
    Dieselbe „Verfügbarkeit vor Aufzeichnung"-Politik, die `LiveTracker::ingest`
    schon auf **Schreib**-Fehler zur Laufzeit anwendet (Recorder wird dann
    verworfen, Tracking läuft weiter).
- **Reiner Resolver** `resolve_plot_recorder(Option<&str>) -> Option<PlotRecorder>`
  in `live.rs` — env-Lookup bleibt in `main.rs`, die Logik ist ohne
  Prozess-Env testbar.
- Der bestehende Metrik-Zähler `firefly_plot_records_written_total` bekommt
  damit im Live-Betrieb erstmals von 0 verschiedene Werte.
- **Kein CAT062-/Wire-Bezug, kein neuer Betriebszwang** — reine Betriebs-/
  Observability-Härtung.

## Verifikation

- **Unit:** `live::plot_recorder_resolves_opt_in_path` (unset/leer/Whitespace →
  None; realer Pfad → Recorder, `.ffplots`-Header auf Platte),
  `live::plot_recorder_unwritable_path_is_non_fatal` (unöffenbarer Pfad → None
  statt Panik), bestehender `live::recorder_captures_every_ingested_plot`.
- **End-to-end (echter Server-Start):** `firefly-server` mit gesetztem
  `FIREFLY_PLOT_RECORD_PATH` und leerem Himmel (keine Quellen) gestartet →
  Log „recording ingested plots to .ffplots (ADR 0020)", Datei mit
  `FFPLOTS\0`-Header angelegt, sauber beendet.
- Gates: `cargo test --workspace` (47 Suiten grün), `cargo clippy`,
  `cargo fmt`. TECHNICAL.md §6.2 + INSTALLATION.md §7 auf die opt-in-Env
  gezogen.

## Damit ist der Quick-Win-Block (AP-QW) abgeschlossen

QW.1 (Track-Nummern-Pool) · QW.2 (Fuzzing + FSPEC-Fix) · QW.3 (I062/080
MON/SPI) · QW.4 (Recorder-Verdrahtung). Roadmap-Stand **33,5 %**. Nächstes
Paket: **AP-REG** (Sensor-Registrierung/Bias-Schätzung, S5) — der
anspruchsvollste offene Punkt und die Voraussetzung, echte Radare ohne
Doppelbilder zu fusionieren.
