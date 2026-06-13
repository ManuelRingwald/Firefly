# ADR 0013 — Asynchrone Pro-Plot-Verarbeitung + Periodischer Ausgabetakt

- **Status:** akzeptiert — **Umsetzung läuft** (13.1 `process_plot` umgesetzt, Ansatz B/additiv; 13.2–13.7 offen — siehe Abschnitt „Umsetzungsstand / Wiedereinstieg")
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

---

## Umsetzungsstand / Wiedereinstieg

> **Zweck dieses Abschnitts:** Diese Architektur-Entscheidung ist **angenommen**, aber
> noch **nicht implementiert**. Ein erster Foundation-Schritt wurde begonnen und
> bewusst wieder zurückgenommen, damit `main` grün und stabil bleibt. Dieser
> Abschnitt hält fest, *was* begonnen wurde, *warum* es zurückgenommen wurde und
> *wie* die volle Umsetzung wieder aufgenommen wird.

### Was wurde begonnen (Foundation-WIP)

Ein erster Schritt — **nur Teil 1 (Simulator)** der drei konzeptionellen Änderungen —
wurde umgesetzt und ist in der Git-Historie erhalten:

- **Commit `6a58a03`** *„WIP: ADR 0013 foundation — azimuth-dependent plot timestamps, sync radars"*

Inhalt dieses WIP (Dateien `crates/firefly-sim/src/radar.rs`, `crates/firefly-sim/src/run.rs`,
`crates/firefly-server/src/scene.rs`):

- **`scan_offset` entfernt** aus `RadarParams` — alle Radare starten bei `t = 0`,
  laufen aber mit ihren eigenen, unabhängigen Perioden (4 / 10 / 12 s in Frankfurt).
- **Azimut-abhängige Pro-Plot-Zeitstempel:** `Radar::try_detect` bekommt statt eines
  fertigen `Timestamp` den `scan_start` und berechnet `plot_time = scan_start +
  (azimut / 2π) · min(scan_period, 0,1 s)`. Jeder Plot trägt damit seinen eigenen,
  feiner aufgelösten Zeitstempel innerhalb der Antennendrehung.
- **`run()`** sortiert die Plots nach dieser neuen Pro-Plot-Zeit statt nach der Scan-Zeit.

### Warum es zurückgenommen wurde

Der WIP setzt **nur Teil 1** um. Die dafür *notwendigen* Teile 2 (Tracker:
`process_plot` statt `process_scan`) und 3 (Server: periodischer Ausgabetakt) fehlen
noch. Ohne sie kollidiert der Foundation-Schritt mit der bestehenden Batch-Semantik:

- `Player::scans` gruppiert Plots nach **exakt gleicher** Datenzeit. Sobald jeder Plot
  einen eigenen (azimut-abhängigen) Zeitstempel hat, fällt fast jeder Plot in seinen
  *eigenen* „Scan" → der Tracker bucht massenhaft Fehltreffer → **Track-Churn**.
- Sichtbarer Effekt: Der Regressionstest
  `scene::frankfurt_scene_keeps_one_identity_per_aircraft` lieferte **155 statt 8**
  Track-IDs.

Da die Foundation allein einen roten Test (Qualitäts-Gate, `CLAUDE.md` §5) hinterlässt
und die volle Umsetzung ein größerer, mehrstufiger Schritt ist (S4–S5, gehört laut
„goldener Regel" in erklärte, freigegebene Häppchen), wurde der WIP mit
**Commit `0959059`** (`git revert`) zurückgenommen. `main` ist damit wieder grün
(M6.5-Stand, 8 stabile Track-IDs, `scan_offset = 0 / 1,3 / 2,6 s`).

### Umsetzung in Häppchen (Plan für den Wiedereinstieg)

Reihenfolge so gewählt, dass **nach jedem Häppchen die Tests grün** bleiben:

- [x] **13.1 — Tracker `process_plot` (Kern-Umbau, S5). ✅ umgesetzt
  (Ansatz B — additiv).** Neue API `Tracker::process_plot(plot)` **neben** dem
  bestehenden `process_scan`. Jeder Aufruf prädiziert alle Tracks auf die
  **Plot-Zeit** und gatet/assoziiert gegen die **Live-Schätzung** (keine
  eingefrorene Scan-Start-Referenz; ADR 0011 entfällt im async-Pfad, weil
  zeitlich getrennte Plots durch die Prädiktions-Kovarianz wieder ins Tor
  wachsen). Update per PDA über die eine Messung (JPDA-Exklusivität über Tracks)
  bzw. Initiierung außerhalb jedes `init_gate`; danach zeit-skalierte
  Bestätigung/Löschung. **FR-TRK-022**, Tests `tracker::process_plot_*`.

  > **Abweichung von der ursprünglichen Planung (bewusst, freigegeben).** Der
  > erste Entwurf sah vor, `process_scan` *sofort* als dünne Schleife über
  > `process_plot` umzuschreiben. Bei der Umsetzung zeigte sich: die
  > Same-Time-Batch-Semantik (eingefrorene Fusions-Referenz + Joint-Association,
  > ADR 0011/FR-TRK-020) ist für die heutigen Tests **tragend**, solange Plots
  > dieselbe Datenzeit teilen (was der Simulator bis 13.5 noch tut) — eine
  > naive Pro-Plot-Schleife mit Live-Schätzung würde
  > `two_sensors_seeing_one_aircraft_make_one_track` (Geist nach sequenzieller
  > Tor-Verengung) und `jpda_keeps_two_close_parallel_tracks_distinct`
  > (verlorene Joint-Exklusivität) brechen. Daher **Ansatz B**: `process_plot`
  > wird additiv eingeführt, `process_scan` bleibt unverändert. Der Batch-Pfad
  > treibt den Player weiter, bis die asynchrone Ausgabe-Pipeline (13.4/13.5)
  > umschaltet; dort wird der Frankfurt-Batch-Test durch eine Async-Regression
  > ersetzt und `process_scan` zurückgebaut. Vorteil: jedes Qualitäts-Gate
  > bleibt in **jedem** Häppchen grün, kein Test-Churn vor der Zeit. Kosten:
  > temporäre Logik-Duplikation zwischen beiden Pfaden, in 13.4/13.5
  > zusammengeführt.
- [ ] **13.2 — Adaptiven Lebenszyklus auf Zeitkontinuität umstellen (S4).** Treffer/
  Fehltreffer nach **tatsächlichen Zeitlücken** statt nach Scan-Aufrufen (ADR 0012
  vereinfachen). Damit ist der Workaround-Anteil nicht mehr nötig.
- [ ] **13.3 — `snapshot_at(t)` (S4).** `Tracker::snapshot_at(t: Timestamp) ->
  Vec<SystemTrack>` prädiziert alle Tracks auf `t` (IMM + Dead-Reckoning) und
  reportiert sie — ohne den Zustand zu verändern (read-only Projektion).
- [ ] **13.4 — Periodischer Ausgabetakt im Player/Server (S4).** `Player::frames`
  puffert Plots nach Datenzeit, arbeitet sie kontinuierlich in den Tracker und
  emittiert Frames im festen Takt `T_out` (12-Factor: `FIREFLY_OUTPUT_PERIOD`,
  Default = kleinste Sensorperiode). `pacing::due_at` bleibt unverändert.
- [ ] **13.5 — Simulator-Foundation neu einspielen (S3).** Den WIP aus `6a58a03`
  wieder aufnehmen (`git cherry-pick 6a58a03` als Ausgangspunkt) — jetzt *deckt*
  ihn die Pro-Plot-Pipeline. `scan_offset` entfällt endgültig.
- [ ] **13.6 — Frankfurt-Regression auf asynchrone Perioden anpassen (S3).** Test auf
  8 stabile IDs bei echt asynchronen Radaren (0/4/8…, 0/10/20…, 0/12/24…). Neue
  Regressions-Tests für periodischen Output.
- [ ] **13.7 — CAT062-Adapter auf `snapshot_at` umstellen (S3).** Encoder speist sich
  aus dem periodischen Snapshot statt aus „pro Scan".

### Wiedereinstiegs-Anker (Kurzform)

- **Akzeptierte Entscheidung:** dieses ADR (Option B).
- **Foundation-Code:** `git show 6a58a03` (Simulator-Teil, Teil 1).
- **Zurücknahme:** `git show 0959059` (Revert, damit `main` grün ist).
- **Gebrochenes Symptom ohne Teile 2+3:** `frankfurt_scene_keeps_one_identity_per_aircraft`
  → 155 statt 8 IDs.
- **13.1 erledigt:** `Tracker::process_plot` additiv (Ansatz B), FR-TRK-022, Tests
  `tracker::process_plot_*`; `process_scan` unverändert, alle Gates grün.
- **Nächster Schritt:** Häppchen **13.2** (adaptiven Lebenszyklus auf
  Zeitkontinuität umstellen, S4), erklären → Go → bauen.
