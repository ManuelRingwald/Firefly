# REG.2b — Sensor-Registrierung: Bias-Korrektur vor der Fusion

> **Anforderung:** FR-TRK-039 · **ADR:** 0034 ·
> **Einstufung:** S5 · umgesetzt auf Fable 5

## Fachlich: Warum?

Mit REG.2a *sieht* Firefly die systematischen Radar-Fehler live — aber die
Fusion rechnete weiter mit den rohen, verschobenen Messungen: dasselbe
Flugzeug erscheint zwei leicht versetzten Radaren an zwei Orten, die zentrale
Mess-Fusion (ADR 0010) baut Doppelbilder. REG.2b schließt den Kreis: Die
geschätzten Biases werden **vor der Fusion von den Messungen abgezogen** —
die eigentliche „Registrierung" im ARTAS-Sinn. Damit ist Firefly erstmals
fähig, echte Multi-Radar-Konstellationen ohne Doppelbilder zu fusionieren.

Weil das ein **Regelkreis in den sicherheitsrelevanten Fusionspfad** ist,
liegt der Kern nicht in der Subtraktion (trivial), sondern in der
**Anwendungs-Politik**: Wann ist eine Schätzung gut genug — und wie wird sie
angewandt, ohne das Lagebild springen zu lassen?

## Technik

**`ApplyPolicy` + `RegistrationApplier`** (`firefly-track::registration`,
rein/deterministisch):

- **Gate — alle Kriterien oder nichts:**
  - `observable` — rangdefiziente Geometrien liefern Minimum-Norm-Lösungen,
    deren Aufteilung zwischen den Sensoren willkürlich ist: nie anwenden.
  - **Residuen-Gewinn** — `rms_after ≤ 0,5 · rms_before`. Eine Schätzung,
    die die Residuen kaum schrumpft, fittet Rauschen statt Bias; bei
    tatsächlich bias-freien Sensoren hält genau dieses Kriterium die
    Korrektur korrekt bei Null.
  - **Plausibilität** — |Δr| ≤ 1000 m, |Δθ| ≤ 1°. Reale Kalibrierfehler sind
    zweistellige bis dreistellige Meter und Zehntelgrad; eine
    Kilometer-„Schätzung" ist ein Daten-/Geometriefehler.
- **Geglättete Übergänge:** angewandt = exponentieller Tiefpass (α = 0,3 je
  Schätzlauf) der akzeptierten Schätzungen — eine frische Schätzung
  verschiebt das Lagebild nie sprunghaft. Gate-Ausfälle werden 3 Läufe
  **gehalten** (ein dünnes Verkehrs-Fenster wickelt keine gute Kalibrierung
  ab), danach klingt die Korrektur zur Null ab; numerisch verschwundene
  Einträge werden entfernt (`active()` meldet ehrlich „keine Korrektur").

**Stabilität per Konstruktion (die zentrale Regelungs-Entscheidung):** Der
Monitor beobachtet weiterhin den **rohen** Plot-Strom. Seine Schätzung ist
damit stets der *volle* Bias — unabhängig davon, was gerade angewandt wird.
Die angewandte Korrektur ist ein reiner Tiefpass dieser Schätzung: **kein
Integrator im Kreis, nichts kann oszillieren.** (Die Alternative — den
korrigierten Strom schätzen und Residuen aufintegrieren — wäre ein echt
rückgekoppelter Kreis mit Schwingungsrisiko; bewusst vermieden.) Der
Konvergenz-Test belegt monotone Annäherung ohne Überschwingen.

**Server-Verdrahtung** (`firefly-server`):

- `LiveTracker::with_registration_apply`; Korrektur `r − Δr`, `θ − Δθ`
  (azimut-normalisiert auf [0, 2π)) **vor** `process_plots`, ausschließlich
  für Radare mit aktiver Korrektur — geodätische Plots und fremde Sensoren
  passieren unverändert.
- Der Applier rückt **genau einmal je Schätzlauf** vor (neuer Zähler
  `RegistrationMonitor::runs_total`); abgelehnte Läufe zählen mit und
  speisen Hold/Decay.
- Die `.ffplots`-Aufzeichnung enthält weiterhin die **rohen** Plots: ein
  Replay durchläuft dieselbe Korrektur-Logik, statt doppelt zu korrigieren
  (NFR-REPRO-001).
- **Doppeltes Opt-in:** `FIREFLY_REGISTRATION_APPLY` **zusätzlich** zu
  `FIREFLY_REGISTRATION_ENABLED` — ein Regelkreis in den Fusionspfad wird
  ausdrücklich geschaltet, nie impliziert. `_APPLY` ohne laufenden Monitor:
  Warn-Log, No-op.
- **Observability:** `firefly_registration_apply_active` (0/1) und die
  angewandten Bias-Gauges je Sensor (getrennt von den rohen Schätzwerten);
  `info`-Log bei Übernahme/Rücknahme der Korrektur.

## Ehrliche Grenzen (REG.2b)

- **Feste Politik-Parameter** — Gate-Schwellen, α und Hold sind
  `ApplyPolicy`-Defaults; env-Tuning erst, wenn der Betrieb es verlangt.
- Die REG.1-Grenzen gelten fort: kein Zeit-Offset-Term, ungewichtete
  Schätzung.
- Bias-Statistik auf den Draht (I063/070–092) ist REG.3.

## Tests (Ground-Truth-Nachweis)

`firefly-track` (5 neu): jedes Gate-Kriterium allein verwirft; erste
Glättungs-Stufe exakt α·Ziel, Konvergenz ohne Überschwingen; Hold über
3 Ausfälle, dann Abklingen bis inaktiv; `correct` trifft nur gelistete
Radar-Messungen; **geschlossene Kette** Monitor→Gate→Glättung→Korrektur über
den synthetischen Live-Strom (150 m/0,3° monoton zurückgewonnen, korrigierte
Messung < 10 m neben der Wahrheit). `firefly-server` (1 neu, End-to-End über
den echten Ingest-Pfad): identischer biased-Radar-Strom in zwei Tracker —
mit Korrektur sitzt der Track auf der Wahrheit, ohne trägt er die volle
800-m-Verschiebung. Gates: `cargo test --workspace`, `clippy`, `fmt` grün.
Kein Wire-/ICD-Bezug — Wayfinder unberührt.
