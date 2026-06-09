# M2 — Der Tracker: aus Plots werden Tracks

> Verständliche Erklärung des zweiten Meilensteins. Dieses Dokument wächst mit
> jedem umgesetzten Häppchen. Begriffe stehen ausführlicher im
> [Glossar](../glossary.md).

Der Tracker ist das Herzstück: Er nimmt den verrauschten, lückenhaften
Plot-Strom (aus M1) und macht daraus saubere, durchgehende **Tracks**. Wir bauen
ihn in kleinen Schritten in der Crate `firefly-track`.

---

## Häppchen 2.1 — Vom Plot zur kartesischen Messung *mit Unsicherheit*

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-002`

### Das Problem (fachlich)

Ein Radar misst **polar** — Entfernung und Winkel. Der Tracker will Bewegung
schätzen und tut das am liebsten in einem **flachen kartesischen Gitter**
(x = Ost, y = Nord), wo geradeaus-Fliegen eine gerade Linie ist. Also rechnen
wir jede Messung von polar nach kartesisch um.

Die *Position* umzurechnen ist simple Trigonometrie. Der wichtige Teil ist die
*Unsicherheit*: Ein Radar ist in der **Entfernung präzise**, im **Winkel grob** —
und ein kleiner Winkelfehler wird mit der Entfernung zu einem großen seitlichen
Versatz. Die Unsicherheit ist deshalb eine **Zigarre**: schmal längs der
Sichtlinie, breit quer dazu, und gekippt je nach Himmelsrichtung des Ziels.

Warum das zählt: Der Kalman-Filter (Häppchen 2.2) wägt Messung gegen Vorhersage
ab. Das geht nur richtig, wenn er die *richtungsabhängige* Unsicherheit kennt.

### Die Lösung (technisch)

Wir liefern pro Plot eine `CartesianMeasurement`:
- **Position** `z = [Ost, Nord]` (m), aus Bodenentfernung `ρ = Range·cos(Elevation)`
  und Azimut `θ`: `Ost = ρ·sinθ`, `Nord = ρ·cosθ`.
- **Mess-Kovarianz** `R` (2×2-Matrix), die die Zigarren-Ellipse beschreibt.

Der Kniff: Im Polarsystem ist die Unsicherheit einfach (Entfernung und Winkel
unabhängig: `R_polar = diag(σ_range², σ_azimut²)`). Wir „transportieren" sie über
die **Jacobi-Matrix** `J` der Umrechnung ins kartesische System:
`R = J · R_polar · Jᵀ`. Das ergibt genau die gekippte Ellipse.

Ergebnis in Zahlen: Längs der Sichtlinie ist die Varianz ≈ `σ_range²`, quer dazu
≈ `(ρ·σ_azimut)²` — die Quer-Unsicherheit **wächst also mit der Entfernung**
(quadratisch in der Varianz). Genau das prüfen unsere Tests nach.

### Eine bewusste Trennung: Modell ≠ Wahrheit

Der Simulator *kennt* die wahren Rausch-σ. Der Tracker bekommt ein eigenes,
**angenommenes** `SensorErrorModel` (σ aus „Datenblatt/Konfiguration"). Beide
sind bewusst getrennt: Ein echter Tracker kennt die Wahrheit nie, er *glaubt*
ein Modell. Später können wir gezielt untersuchen, was passiert, wenn Modell und
Realität auseinanderlaufen.

### Was noch fehlt (→ 2.2)

Wir haben jetzt eine saubere, kartesische Messung mit korrekter Unsicherheit —
aber noch keinen Filter, der über die Zeit glättet. Das ist Häppchen 2.2.

---

## Häppchen 2.2 — Der Kalman-Filter (Constant-Velocity)

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-003`

### Die Idee (fachlich)

Eine Einzelmessung ist verrauscht und kennt keine Geschwindigkeit. Der
**Kalman-Filter** macht aus der Folge von Messungen eine geglättete, durchgehende
Schätzung von **Position *und* Geschwindigkeit** — indem er in jedem Schritt
**Vorhersage und Messung nach ihrer jeweiligen Unsicherheit gewichtet** verrechnet.

Zwei Schritte im Wechsel:
- **Prädiktion:** „Wenn das Ziel so weiterfliegt — wo ist es beim nächsten Scan?"
  Die Unsicherheit *wächst* dabei.
- **Update:** Neuer Plot kommt rein, wird mit der Vorhersage verschmolzen; die
  Unsicherheit *schrumpft*.

### Die Bausteine (technisch)

- **Zustand** `x = [Ost, Nord, v_Ost, v_Nord]`, **Zustands-Kovarianz** `P` (4×4).
- **Constant-Velocity-Modell** über die Übergangsmatrix `F`: neue Position = alte
  Position + Geschwindigkeit · Δt.
- **Prozessrauschen `Q`** (das „Manöver-Budget"): erlaubt Abweichungen vom
  geraden Flug. Hier als *kontinuierliches Weißes-Beschleunigungs-Rauschen*,
  parametriert über eine Beschleunigungs-Intensität.
- **Update** mit der Messung aus 2.1: Innovation `y = z − H·x` (die „Überraschung"),
  Kalman-Gain `K` (der „Vertrauens-Hebel" zwischen Messung und Vorhersage),
  dann `x ← x + K·y` und ein schrumpfendes `P`.
- **Joseph-Form** für das `P`-Update: numerisch stabil, hält `P` gültig
  (symmetrisch & positiv definit) — Sorgfalt im Sinne der Assurance (ADR 0004).

### Der Beweis, dass es wirklich glättet

Der End-to-End-Test füttert den Filter mit den verrauschten Plots eines
gleichförmig fliegenden Ziels (aus dem M1-Simulator) und prüft: der
**Positionsfehler des Filters ist kleiner als der der Rohmessungen**, und die
**geschätzte Geschwindigkeit konvergiert** nahe an die Wahrheit (≈ 150 m/s nach
Norden) — obwohl das Radar Geschwindigkeit nie misst.

### Was noch fehlt (→ 2.3)

Bisher nehmen wir an, dass jeder Plot zu *diesem einen* Track gehört. Bei mehreren
Zielen/Plots braucht es ein **Gating** — Häppchen 2.3.

---

## Häppchen 2.3 — Gating: das Plausibilitäts-Tor

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-004`

### Die Idee (fachlich)

Pro Scan kommen viele Plots (mehrere Flugzeuge) und Falschalarme. Bevor wir
zuordnen (2.4), schließen wir billig das Unsinnige aus: Für jeden Track zählen nur
Plots in einem **Plausibilitäts-Fenster** (Gate) um seine Vorhersage. Das spart
Rechenzeit, verhindert absurde Zuordnungen und ist das Fundament der Assoziation.

Wichtig: Das Tor ist **nicht rund**. Es berücksichtigt die Unsicherheit (Track
*und* Messung) und hat damit dieselbe **zigarrenförmige** Gestalt wie die
Innovations-Kovarianz `S`.

### Die Umsetzung (technisch)

Die Zutaten liefert der Filter aus 2.2: Innovation `y = z − H·x` und ihre
Kovarianz `S`. Daraus die **quadrierte Mahalanobis-Distanz** `d² = yᵀ·S⁻¹·y` —
eine Zahl, „wie viele Sigma" der Plot entfernt ist. Gate-Regel: `d² ≤ γ`.

Die Schwelle `γ` kommt aus der **χ²-Verteilung** mit 2 Freiheitsgraden (Ost/Nord).
Für genau 2 Freiheitsgrade gibt es die geschlossene Formel `γ = −2·ln(1 − P_G)`
mit der Gate-Wahrscheinlichkeit `P_G` (Default 99 % → γ ≈ 9,21). Kein
Statistik-Paket nötig.

Die Berechnung von `y`/`S` haben wir in eine **gemeinsame Filter-Methode**
gezogen — Gating und Update teilen sie (eine Quelle der Wahrheit).

### Der Kern-Nachweis

Ein Test zeigt das Entscheidende: **derselbe Abstand** wird *entlang* der
unsicheren (Quer-)Achse akzeptiert, aber *quer* zur sicheren (Entfernungs-)Achse
abgelehnt — Mahalanobis ≠ Euklidisch. Genau dafür ist das Tor zigarrenförmig.

### Was noch fehlt (→ 2.4)

Das Gate sagt, welche Plots *möglich* sind. Wenn mehrere Tracks und mehrere Plots
sich überschneiden, müssen wir die **beste Gesamtzuordnung** finden — Häppchen 2.4.

---

## Häppchen 2.4 — Datenassoziation (Global Nearest Neighbor)

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-005`

### Die Idee (fachlich)

Nach dem Gating kann ein Plot für mehrere Tracks plausibel sein. Es braucht eine
eindeutige **1:1-Zuordnung**. „Jeder Track nimmt seinen nächsten Plot" (gierig)
ist global oft falsch — bei kreuzenden Flugzeugen vertauscht es die Identitäten.
**GNN** minimiert stattdessen die **Gesamtkosten über alle Paare gleichzeitig**.

### Die Umsetzung (technisch)

- **Kostenmatrix:** Zeilen = Tracks, Spalten = Plots, Eintrag = gegatete
  Mahalanobis-Distanz `d²` (außerhalb des Gates: verboten).
- **Ungarische Methode** (Kuhn–Munkres, `O(n³)`) findet die exakt
  kostenminimale Zuordnung — selbst implementiert, ohne neue Abhängigkeit.
- **Reste sauber abgebildet:** über Dummy-Optionen (Track/Plot „unzugeordnet" zu
  Kosten γ). Ergebnis: `pairs`, `unassigned_tracks`, `unassigned_measurements`.

### Der Kern-Nachweis

`hungarian_beats_greedy` zeigt einen Fall, in dem die gierige Wahl
(Gesamtkosten 10) verliert und die Methode die „gekreuzte", global beste Lösung
(Gesamtkosten 4) findet. Dazu: korrekte Zuordnung gegateter Plots, ungegatete
Plots und „ausgehungerte" Tracks bleiben übrig, ungleiche Anzahlen funktionieren.

### Was noch fehlt (→ 2.5)

Jetzt haben wir alle Bausteine — Messung, Filter, Gate, Zuordnung. Häppchen 2.5
fügt sie zum **Track-Lebenszyklus** zusammen.

---

## Häppchen 2.5 — Track-Lebenszyklus & Pro-Scan-Orchestrierung

**Status:** ✅ umgesetzt · Anforderungen `FR-TRK-001`, `FR-TRK-006`

### Die Idee (fachlich)

Erst hier wird aus den Einzelteilen ein *laufender Tracker*. Tracks haben einen
Lebenszyklus: **Geburt** (aus einem Plot ohne Track → zunächst *tentativ*, könnte
Clutter sein), **Bestätigung** (nach Bewährung über mehrere Scans → *confirmed*
und der Luftlage gemeldet), **Coasting** (bei Fehldetektion „segelt" der Track auf
der Vorhersage weiter), **Löschung** (bleibt er zu lange aus). Die Bestätigungs-
Regel ist **M-aus-N** (Default 3 aus 5); gelöscht wird nach aufeinanderfolgenden
Fehltreffern (tentativ schneller als bestätigt).

### Die Umsetzung (technisch)

Ein `Tracker` mit `process_scan(time, plots)` führt pro Scan eine feste Abfolge
aus: **prädizieren → Messungen bilden → zuordnen (Gate+GNN) → Treffer updaten /
Fehltreffer coasten → bestätigen → löschen → neue Tracks gebären**.

Wichtig: `process_scan` ist eine **reine, datenzeit-getriebene Zustandsänderung**
(ADR 0003) — keine Wanduhr, kein I/O. Der Track-Zustand (Filter `x`/`P`, Zähler,
Status) ist einfache, später serialisierbare Daten → Wiederherstellbarkeit
(NFR-CLOUD-001/002/003).

### Der Höhepunkt: zwei kreuzende Ziele

Der Integrationstest `two_crossing_targets_keep_their_identities` lässt zwei
Flugzeuge sich kreuzen und prüft: durchgehend **genau zwei** bestätigte Tracks
mit **entgegengesetzten** Geschwindigkeiten — keine Identitätsvertauschung, keine
verlorenen oder erfundenen Tracks. Das ist der Beweis, dass M2 als Ganzes trägt.

### Damit ist M2 inhaltlich rund

Der Single-Radar-Tracker steht: Messung → Filter → Gate → Zuordnung →
Lebenszyklus. Es folgen noch „Veredelungs"-Häppchen (Cloud-Härtung, ASD-Ausgabe,
Güte-Metriken).

---

## Häppchen 2.6 — Serialisierbarer Zustand: Snapshot & Replay

**Status:** ✅ umgesetzt · Anforderungen `NFR-CLOUD-001/002/003`

### Die Idee (fachlich)

In der Cloud ist der Ausfall einer Instanz normal. Ein zustandsbehafteter
Tracker muss sein „Gedächtnis" **wiederherstellbar** machen: per **Snapshot**
(gespeicherter Stand) und **Replay** (erneutes Abspielen ab dem Snapshot).
Weil die Scan-Funktion deterministisch ist, kommt dabei derselbe Zustand wieder
heraus — gut für Ausfallsicherheit *und* für die Audit-Rekonstruktion.

### Die Umsetzung (technisch)

Der ganze Zustand wird über **`serde`** serialisierbar (Ableitungen auf
`Tracker`, `Track`, `LinearKalman`, …; nalgebra mit `serde`-Feature). Der Kern
bleibt **format-neutral**; fürs Testen dient `serde_json` als Dev-Abhängigkeit
(ADR 0007).

### Eine ehrliche Erkenntnis

JSON ist ein Text-Format — der `f64`-Round-Trip ist nicht bit-genau (1 ULP
Abweichung beobachtet). Deshalb trennen wir die Aussagen sauber:
- **Determinismus** beweisen wir *ohne* Serialisierung: zwei identische Läufe →
  bit-genau gleicher Zustand.
- **Wiederherstellbarkeit** prüfen wir mit enger Toleranz: Restore stellt den
  Zustand auf volle Zahlengenauigkeit her und läuft äquivalent weiter.
- Für byte-genaue Produktions-Snapshots wäre ein binäres Codec die richtige Wahl
  (reine Rand-Entscheidung, später).

### Was noch fehlt (→ 2.7)

Der **neutrale `SystemTrack`-Output in WGS84** (die ASD-Andock-Schnittstelle
Richtung CAT062) — Häppchen 2.7. Danach die Güte-Metriken (2.8).
