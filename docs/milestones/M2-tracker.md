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
Zielen/Plots braucht es ein **Gating** (welche Plots sind überhaupt plausibel?) —
Häppchen 2.3, über die Mahalanobis-Distanz.
