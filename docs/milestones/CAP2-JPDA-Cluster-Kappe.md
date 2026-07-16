# CAP.2 — JPDA-Cluster-Kappe + dokumentierte Auslegungsgrenzen

> **Anforderung:** FR-TRK-052 · **ADR:** — · **ICD:** unberührt ·
> **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich

Der Tracker war schnell — außer in genau dem Moment, in dem es darauf
ankommt. Die JPDA-Zuordnung rechnet in einem **Konflikt-Cluster** (mehrere
Tracks streiten sich um dieselben Plots) alle zulässigen
Plot-zu-Track-Kombinationen durch. In normalem Verkehr sind diese Cluster
winzig; in einem **dichten Pulk** (Warteschleifen-Stapel, Formation,
Parade) kettet sich aber alles zu *einem* Cluster zusammen, und die Zahl
der Kombinationen explodiert exponentiell. Gemessen: Eine Kolonne aus
10 Flugzeugen im 120-m-Abstand kostete **27,8 Sekunden** Rechenzeit für
ein 60-Sekunden-Szenario — der Tracker wäre live hinter die Echtzeit
gefallen, das Lagebild eingefroren. Genau dann, wenn der Himmel am
dichtesten ist.

CAP.2 zieht eine **Kappe** ein: Übersteigt ein Cluster eine gemessene,
begründete Größe, rechnet der Tracker diesen einen Cluster mit einem
einfacheren, garantiert schnellen Verfahren weiter — und meldet das
sichtbar (Metrik + WARN-Log). Der Rest des Lagebilds rechnet unverändert
exakt. Ergebnis: Die 10er-Kolonne fällt von 27,8 s auf **0,75 ms**, und
das Lagebild bleibt in jedem Verkehr flüssig.

## Technik

### Die Kappe (`firefly-track/src/jpda.rs`)

- `MAX_CLUSTER_TRACKS = 8` / `MAX_CLUSTER_PLOTS = 10` — ein Cluster, der
  **eine** der beiden Grenzen reißt, wird nicht mehr exakt enumeriert.
- **Fallback = Pro-Track-PDA:** Jede Track-Zeile wird unabhängig
  normalisiert: `β(i,j) = λ(i,j) / (b(i) + Σ_j λ(i,j))`. Das ist die
  **exakte Einzeltrack-JPDA-Formel** — aufgegeben wird ausschließlich die
  Track-übergreifende Exklusivität („ein Plot gehört höchstens einem
  Track"). Deterministisch, O(Tracks × Plots).
- Der Koaleszenz-Schutz (SPEC.1) läuft **nach** der Assoziation und
  bleibt vollständig wirksam — verschmelzende Tracks werden weiterhin
  auseinandergehalten.
- Rückgabe als `(betas, cap_hits)` (`…_counted`-Variante); die alte
  Signatur delegiert, alle Bestandstests unverändert grün.

### Sichtbarkeit

- Tracker-Zähler `jpda_cap_hits` (im Snapshot `#[serde(default)]` —
  HA.1-Snapshots älterer Stände bleiben wiederherstellbar).
- WARN-Log beim ersten Treffer und jedem 100. (kein Log-Sturm im Pulk).
- Prometheus: `firefly_jpda_cluster_cap_hits_total` (via `on_tick`).
  Im normalen Betrieb — auch `load_grid` mit 100 Zielen — bleibt der
  Zähler **0**.

### Messwerte (2026-07-16, Release-Build, Sandbox-Host)

Dichte Kolonne (`scenarios::dense_column`: 120-m-Abstand, ein Radar,
ein Union-Find-Cluster), Rechenzeit je 60-s-Szenario:

| Kolonne | ohne Kappe | mit Kappe | bestätigte Tracks |
|---------|-----------|-----------|-------------------|
| 8 Ziele | 149 ms | 149 ms (exakt) | 2 |
| 10 Ziele | **27,8 s** | 0,75 ms | 2 (vorher wie nachher) |
| 12 Ziele | Stunden (extrapoliert) | 0,57 ms | stabil |

Bench-Gruppe `dense_cluster` (criterion): 4 Ziele 0,24 ms · 6 Ziele
1,6 ms · 8 Ziele ≈ 160 ms (der teuerste **exakte** Fall liegt jetzt an
der Kappe) · 12 Ziele 0,57 ms (gekappt). Baseline-Durchsatz aus CAP.1
unverändert.

### Warum 8/10?

Aus der Messung, nicht aus dem Bauch: n=8 exakt ≈ 150 ms je 60-s-Szenario
(≈ 2,5 ms je Scan — verkraftbar), n=10 exakt = 27,8 s (Echtzeitbruch).
Die Kappe sitzt an der letzten Größe, die exakt bezahlbar ist; die
Plot-Grenze 10 fängt den dualen Fall (wenige Tracks, Plot-Schwemme durch
Clutter) ab.

## Tests

- `jpda::oversized_cluster_degrades_to_counted_bounded_pda` — 12×12
  voll-valider Cluster: terminiert < 1 s, `cap_hits == 1`, jede Zeile
  summiert auf 1 und ist **identisch** mit der Einzeltrack-PDA-Referenz
  (`association_probabilities`); ein 8×6-Cluster (an der Grenze) rechnet
  exakt, `cap_hits == 0`.
- `firefly-eval::dense_column_is_bounded_by_the_cluster_cap` — die
  12er-Kolonne durchläuft den **vollen** Produktions-Hot-Path in < 30 s,
  Zähler > 0, stabiles Bild.
- Bestand: `jpda_keeps_two_close_parallel_tracks_distinct` u. a.
  unverändert grün — kleine Cluster rechnen exakt wie zuvor.
- `metrics::render_includes_all_metrics` deckt die neue Zeile ab.

## Ehrliche Grenzen

- **Oberhalb der Kappe ist die Zuordnung gröber:** Ein Plot kann mehreren
  Tracks zugleich Gewicht geben. Im gemessenen Szenario ist das
  **unbeobachtbar** — die 120-m-Kolonne ist für einen 50-m/0,08°-Sensor
  physikalisch unauflösbar, der Tracker bestätigt vor wie nach der Kappe
  2 Tracks. Ein *auflösbarer* Pulk oberhalb der Kappe (z. B. weit
  auseinander, aber durch Clutter verkettet) würde dort mehr
  Fehlzuordnungen riskieren als exakte JPDA; der Zähler macht jeden
  solchen Fall sichtbar.
- **Der teuerste exakte Fall bleibt:** ≈ 160 ms je 60-s-Szenario an der
  Kappe (Sandbox-Host) — kein Echtzeitproblem, aber nicht null.
- **Host-abhängig:** Alle Absolutwerte auf Zielhardware wiederholen
  (`cargo bench -p firefly-eval`); die Größenordnungen und das
  Exponential-Verhalten sind übertragbar.
- Die Kappen-Konstanten sind bewusst **nicht konfigurierbar** — eine
  falsch gesetzte Env-Variable dürfte hier die Echtzeitfähigkeit nicht
  aushebeln können. Änderung = Code-Änderung mit Review.
