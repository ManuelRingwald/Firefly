# ADR 0013 — Asynchrone Pro-Plot-Verarbeitung + Periodischer Ausgabetakt

- **Status:** in Entscheidung
- **Datum:** 2026-06-12

## Kontext

Firefly wurde bislang mit einem **gekoppelten Modell** gebaut:
- **Eingang:** Radare senden Plots mit `scan_offset`-Versätzen (z.B. 0, 1.3, 2.6 s) → Simulator erzeugt Batch-Scans nach **Scan-Zeit** → `Tracker::process_scan(time, plots)` verarbeitet alle Plots einer Zeit gemeinsam.
- **Ausgang:** Ein `Frame` pro `process_scan`-Aufruf → WebSocket / CAT062 erbt die **unregelmäßige** Eingangs-Taktung (0, 1.3, 2.6, 4, 5.3 ...).

Diese Kopplung führt zu zwei Problemen:

1. **Unrealistischer Ausgabetakt:** Echte ATC-Systeme (EUROCONTROL ARTAS, ARBS) emittieren das System-Track-Bild mit **fester, regelmäßiger Periode** (typisch 4,8 s), unabhängig von Sensor-Asynchronie. Das Frontend sieht daher unregelmäßig gespacte Updates statt eines stabilen Herzschlags.

2. **Fragile Eingabeverarbeitung:** Der adaptive Lebenszyklus (ADR 0012) wurde als **Workaround** eingeführt, um Track-Churn durch die Scan-Batch-Struktur zu bekämpfen. Mit echter asynchroner Pro-Plot-Verarbeitung entfällt ein guter Teil dieser Komplexität.

### Ist-Zustand und Befund

- Der Simulator (`firefly_sim::run`) erzeugt alle Plots mit **gleicher Scan-Zeit pro Radar** — azimut-Abhängigkeit der Time-of-Day ist nicht modelliert.
- Der Tracker akzeptiert `process_scan(time, plots)` als **Batch**: alle Plots einer Zeit in einem Aufruf, gating/association gegen eine Scan-Start-Referenz.
- Der Server/Player (`firefly-player`, `firefly-server`) emittiert **einen Frame pro Tracker-Aufruf** → asynchroner Output.
- Tests und Frankfurt-Szene sind auf das Batch-Modell abgestimmt.

## Die abgewogenen Optionen

**Option A — Gekoppeltes Batch-Modell (Status quo).**
Plots nach Scan-Zeit gruppieren, ein `process_scan`-Aufruf pro Zeit. Vereinfacht den Tracker-Code, aber erbt die Eingangs-Unregelmäßigkeit nach außen.

**Option B — Entkoppeltes asynchrones Modell.**
Echte Pro-Plot-Verarbeitung (`process_plot` je Messung), periodische **Ausgabestufe** auf separatem Takt (z.B. alle 4,8 s). Entspricht echter ATC-Architektur, aber Tracker-Umstrukturierung nötig.

## Entscheidung

**Wir wählen Option B — vollständig asynchrone Pro-Plot-Verarbeitung mit entkoppeltem periodischem Ausgang.**

Das gliedert sich in drei konzeptionelle Änderungen:

### 1. Simulator: Azimut-abhängige Pro-Plot-Time-of-Day

Jeder Plot erhält einen **eigen** Zeitstempel, nicht die Scan-Zeit:

```
scan_start = 0 s, scan_period = 4 s, scan_azimut = 123°
plot_tod = scan_start + (azimut / 360°) × scan_period
        = 0 + (123 / 360) × 4 ≈ 1,367 s
```

Dadurch:
- Plots **innerhalb einer Antennendrehung** haben unterschiedliche Timestamps.
- Der Simulator emittiert realistischere Daten — echte asynchrone Messungen.
- Plots mehrerer **unabhängiger Radare** (0, 4, 8 ... für Radar 1; 0, 10, 20 ... für Radar 2) sind echt zeitlich versetzt.

### 2. Tracker: Pro-Plot-Verarbeitung statt Scan-Batches

Der Tracker erhält Plots **einzeln oder in beliebiger Gruppierung**, nicht zwangsweise nach Scan-Zeit:

```rust
// Alt (Batch):
process_scan(Timestamp(4.0), &[plot1, plot2, plot3])

// Neu (pro Plot oder beliebig):
process_plot(plot1)
process_plot(plot2)
process_plot(plot3)
// Oder intern gepuffert und regelmäßig verarbeitet.
```

**Interne Struktur:**
- Jeder `process_plot`-Aufruf prädiziert alle Tracks auf die **Plot-Zeit** (nicht Scan-Zeit).
- Gating, Association, Update gegen die aktuelle Schätzung (nicht gegen frozen Scan-Start-Referenz).
- Adaptive Lebenszyklus (ADR 0012) basiert auf **tatsächlichen Treffer-Zeiten**, nicht Miss-Budgets pro Scan — kann vereinfacht werden.

**Determinismus bleibt:** `process_plot(p)` ist weiterhin reine Funktion des Zustands, der Plot-Datenzeit und der Messwerte — keine Wanduhr.

### 3. Server: Entkoppelte Ausgabestufe mit fester Periode

Ein neuer Ausgabe-Scheduler, **unabhängig vom Eingang**:

```
Tracker.process_plot(p1 @ t=1.367 s)  → State update
Tracker.process_plot(p2 @ t=3.891 s)  → State update
...
[Periodischer Ausgabe-Tick @ t=4.8 s]  → snapshot_at(4.8) → Frame
[Periodischer Ausgabe-Tick @ t=9.6 s]  → snapshot_at(9.6) → Frame
```

Dabei:
- `snapshot_at(t)` prädiziert alle Tracks auf Zeit `t` (IMM + dead reckoning).
- Emittiert ein **System-Track-Bild** (CAT062, JSON) mit konsistenter Zeitmarke.
- Periode `T_out` ist **konfigurierbar** (12-Factor: `FIREFLY_OUTPUT_PERIOD`); Default = Minimum aller konfigurieren Sensorperioden (in Frankfurt: 4,8 s).
- Periodischer Ausgang ist **unabhängig** davon, wie viele Radare gerade online sind oder ihre Periode haben.

## Begründung

- **Realismus:** Echte SDPS emittieren ihren Air Picture mit eigenem, stabilem Takt — nicht getrieben durch Sensor-Asynchronie.
- **Robustheit:** Ausfälle oder Periodenwechsel von Sensoren können den Ausgabetakt nicht destabilisieren.
- **Determinismus bewahrt:** `process_plot` ist (wie `process_scan`) eine reine Funktion; der Ausgabetakt ist **absolute Datenzeit**, nicht Wanduhr → replay-fähig.
- **Operativität:** Das Frontend und andere Konsumenten erhalten einen **vorhersagbaren, stabilen Herzschlag** — nicht die asynchrone Unregelmäßigkeit von heute.
- **Vereinfachung:** Die komplexe adaptive-Lebenszyklus-Logik (ADR 0012) war ein Workaround für die Batch-Struktur und kann teilweise **abgelöst werden**, wenn Eingang und Ausgang entkoppelt sind.

## Konsequenzen

### Simulator (`firefly-sim`)
- **Radar:** `scan_times()` bleibt; neu: `scan_plots_at(time)` oder `plots_for_scan(scan)` mit azimut-abhängiger ToD pro Plot.
- **`run()`:** Liefert weiterhin einen zeitgeordneten `Vec<Plot>`, aber jeder Plot hat seinen **eigenen Zeitstempel** (azimutabhängig), nicht die Scan-Zeit.

### Tracker (`firefly-track`)
- **API-Bruch:** `process_scan(time, plots)` wird zu `process_plot(plot)` oder `process_plots(plots)` (ohne erzwungene Gruppierung nach Zeit).
- **Interne Arbeit:**
  - Vorhersage auf Plot-Zeit (nicht Scan-Zeit).
  - Keine frozen Scan-Start-Referenz mehr (Gating gegen Live-Schätzung).
  - ADR 0011 (Initierungs-Tor-Freezing) entfällt oder wird vereinfacht.
  - ADR 0012 (adaptive Lebenszyklus): Vereinfachung durch zeitliche Kontinuität statt Batch-Semantik.
- **Neuer Public-API:** `snapshot_at(t: Timestamp) -> Vec<SystemTrack>` — prädiziert alle Tracks auf Zeit `t` und reportiert sie.

### Player (`firefly-player`)
- **`frames()`:** Erzeugt nicht mehr „einen Frame pro Tracker-Aufruf", sondern erzeugt Frames periodisch (nach `T_out`).
- Intern: puffert Plots nach Datenzeit, arbeitet sie kontinuierlich in den Tracker, emittiert Frames nach festem Takt.

### Server (`firefly-server`)
- **`pacing::due_at`:** Bleibt unverändert — mappt Data-Zeit auf Wall-Clock.
- **Output-Scheduler:** Neuer Baustein, der periodisch `Tracker::snapshot_at(t)` aufruft und das Bild via WebSocket sendet.

### Tests und Frankfurt-Szene
- Tests an neuer API anpassen.
- Frankfurt: `scan_offset` **entfernen** — Radare laufen unabhängig (0, 4, 8, ... für R1; 0, 10, 20, ... für R2; 0, 12, 24, ... für R3).
- Neue Regressions-Tests für asynchrone Multi-Radar-Plots + periodischen Output.

## Nicht entschieden (Folgeschritte)

- **Live-Sensorperioden-Adaption:** Kann `T_out` sich an die aktuelle Sensorausstattung anpassen (z.B. wenn ein Radar mit 10 s Period hinzukommt)? → Eigner ADR später.
- **Puffer-Semantik:** Wie lang das Eingabepuffer für verspätete Plots? → Konfiguration, nicht Design.
- **CAT062 aus Snapshot:** Wird der CAT062-Encoder (heute: pro `scan`) auf den neuen `snapshot_at(t)` angepasst? → Ja, aber als Adapter-Anpassung, nicht hier spezifiziert.

## ADR-Links

- **ADR 0010:** Zentrale Mess-Fusion bleibt, ist aber nun **pro-Plot-Fusionierung**, nicht Scan-Fusion.
- **ADR 0011:** Scan-Start-Referenz entfällt → Gating gegen Live-Schätzung. Initiierungs-Tor-Logik vereinfacht sich oder wird obsolet.
- **ADR 0012:** Adaptive Lebenszyklus als Workaround für Batch-Semantik — kann mit echter Zeitkontinuität simplifiziert werden.
