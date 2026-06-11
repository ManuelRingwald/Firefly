# M5 — Manöver sauber tracken: der IMM-Filter

> Verständliche Erklärung des fünften Meilensteins (Teil 1: IMM). Begriffe
> stehen ausführlicher im [Glossar](../glossary.md). JPDA (dichter Verkehr,
> überlappende Tore) ist der zweite Teil von M5 und steht noch aus.

Bis M4 schätzte jeder Track seine Bahn mit **einem** Kalman-Filter unter **einem**
Bewegungsmodell: *Constant Velocity* (CV) — geradeaus, gleichförmig. Das ist auf
den langen Reiseflug-Strecken genau richtig. Sobald ein Flugzeug aber **kurvt**,
zeigt sich das Dilemma eines einzelnen Modells:

- Stellt man das **Prozessrauschen `Q`** klein ein (Vertrauen in „geradeaus"),
  „hinkt" der Filter in der Kurve hinterher — er sagt stur die Tangente voraus,
  und die Schätzung wandert nach außen aus der Kurve.
- Stellt man `Q` groß ein (damit er Kurven folgt), „zappelt" er auf der Geraden
  dem Messrauschen hinterher und wird unnötig ungenau.

Ein einzelnes `Q` ist immer ein fauler Kompromiss. **M5 löst das mit dem IMM**
(*Interacting Multiple Model*): mehrere Bewegungsmodelle laufen **parallel**, und
die Messungen selbst entscheiden laufend, welches gerade passt.

---

## Häppchen M5.1 — Ein zweites Bewegungsmodell: Coordinated Turn

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-011`

### Das Problem (fachlich)

Eine Kurve ist kein „Rauschen um die Gerade", sondern eine **eigene
Bewegungsform**: gleichmäßiger Kurvenflug mit konstanter **Drehrate** `ω`
(Grad bzw. Radiant pro Sekunde). Wer dieses Verhalten *modelliert*, statt es als
Störung zu behandeln, folgt der Kurve sauber.

### Die Lösung (technisch)

Neues Modul `firefly-track::motion` mit dem Enum `MotionModel`:
- `ConstantVelocity` — das bisherige M2-Modell.
- `CoordinatedTurn { rate }` — eine Übergangsmatrix, die pro Zeitschritt den
  **Geschwindigkeitsvektor um `ω·dt` dreht** und den entstehenden Kreisbogen in
  die Position integriert. Der Betrag der Geschwindigkeit bleibt erhalten (eine
  Kurve ändert die Richtung, nicht die Schnelligkeit).

Der Clou: für `ω → 0` geht die CT-Matrix **exakt** in die CV-Matrix über —
Geradeausflug ist nur der Sonderfall „Drehrate null". Beide Modelle teilen
denselben 4-D-Zustand `[Ost, Nord, v_Ost, v_Nord]`, sodass derselbe Filter, dasselbe
Tor und dieselbe Assoziation mit beiden arbeiten. `LinearKalman::predict_with`
prädiziert unter einem beliebigen Modell; `predict` bleibt der CV-Standard.

---

## Häppchen M5.2 — Das IMM-Grundgerüst: Bank + Mischung

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-012`

### Die Idee (fachlich)

Statt sich auf ein Modell festzulegen, hält der IMM eine **Bank** von Filtern —
je einen pro Bewegungsmodell — und für jeden eine **Modellwahrscheinlichkeit**
`μ` („wie gut erklärt dieses Modell die Messungen gerade?"). Der **Wechsel**
zwischen den Modellen wird als **Markov-Kette** modelliert: `π_ij` ist die
Wahrscheinlichkeit, im nächsten Scan von Modell `i` nach `j` zu springen.

### Die Mischung (technisch)

Das Herzstück und der namensgebende Teil („*interacting*"): Bevor jedes Modell
für sich filtert, startet es **nicht** aus seiner eigenen letzten Schätzung,
sondern aus einer **Mischung** aller Modell-Schätzungen — gewichtet damit, wie
wahrscheinlich ein Ziel gerade *in dieses* Modell gewechselt ist
(`μ_{i|j} = π_ij·μ_i / c_j`). So erbt selbst ein eben noch unwahrscheinliches
Modell einen sinnvollen Startzustand, wenn das Ziel gerade dorthin manövriert.
Die gemischte Unsicherheit bekommt zusätzlich einen
**„Spread-of-the-Means"-Term**: Sind sich die Modelle über den Zustand uneins,
ist der gemischte Start ehrlich unsicherer. Implementiert in der Struktur `Imm`
(`predicted_model_probabilities`, `mixing_probabilities`,
`mixed_initial_conditions`).

---

## Häppchen M5.3 — Der vollständige Zyklus: Likelihood entscheidet

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-013`

Ein IMM-Zyklus hat vier Stufen:

1. **Mischung** (M5.2).
2. **Modellbedingtes Filtern:** Jedes Modell prädiziert + aktualisiert aus seiner
   gemischten Anfangsbedingung.
3. **Modellwahrscheinlichkeits-Update:** Jedes Modell bekommt eine
   **Likelihood** `Λ_j = N(y_j; 0, S_j)` — wie gut hat es den Plot vorhergesagt?
   Ein Plot, der landet, wo das Modell ihn erwartet hat, scort hoch. Die
   Wahrscheinlichkeiten werden neu gewichtet: `μ_j ∝ c_j·Λ_j`. **So „erkennt" der
   IMM ein Manöver — ganz ohne separaten Manöver-Detektor.**
4. **Kombination:** Die ausgegebene Schätzung ist die `μ`-gewichtete Mischung
   aller Modell-Schätzungen.

`Imm::step` führt einen ganzen Zyklus aus; intern getrennt in `predict` und
`update` (siehe M5.4). Nachgewiesen durch Konvergenz-Tests: auf gerader Bahn
gewinnt CV, in der Kurve das passende CT-Modell.

---

## Häppchen M5.4 — Der IMM im Tracker

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-014`

Jeder `Track` trägt jetzt statt eines einzelnen Kalman-Filters eine **IMM-Bank**.
Der Pro-Scan-Ablauf des Trackers bleibt unverändert — nur die drei Berührpunkte
mit dem Filter wechseln auf den IMM:

- **Prädiktion:** `Imm::predict` (Mischung + modellweise Prädiktion) statt
  `LinearKalman::predict`.
- **Gating/Assoziation:** über die **kombinierte Schätzung** (`Track::estimate`)
  — der IMM sieht nach außen aus wie *ein* Filter.
- **Update:** `Imm::update` (faltet den Plot in jedes Modell und gewichtet die
  Modelle nach Likelihood neu).

Ein neuer Track wird über `ImmConfig::seed` aus der ersten Messung geboren; die
Bank-Konfiguration (welche Modelle, Markov-Matrix, Start-Wahrscheinlichkeiten)
steht in `TrackerConfig`. Standard: CV + zwei Coordinated-Turns (±3°/s, eine
„Rate-One"-Kurve), mit einer **klebrigen** Markov-Matrix (Modelle wechseln
selten). Auf Geradeausflug bleibt das Verhalten praktisch unverändert
(RMSE- und Identitäts-Tests grün); ein kurvendes Ziel treibt nachweislich die
Wahrscheinlichkeit des passenden Turn-Modells nach oben
(`imm_favours_the_turn_model_on_a_turning_target`).

---

## Warum das zählt (Sicherheit & Lagebild)

Ein Tracker, der Kurven „verschmiert", liefert dem Lotsen in genau den Momenten
das schlechteste Bild, in denen es darauf ankommt — beim Eindrehen in den
Anflug, beim Ausweichen, in der Warteschleife. Der IMM hält die Schätzung auch
im Manöver eng an der Wahrheit und macht die Unsicherheit *ehrlich* sichtbar.
Das ist die Voraussetzung dafür, dass nachgelagerte Funktionen
(Konfliktwarnung, Trajektorienvorhersage) verlässlich arbeiten.

**Noch offen in M5:** JPDA (*Joint Probabilistic Data Association*) für dichten
Verkehr mit überlappenden Toren — die probabilistische Ergänzung zur heutigen
„harten" 1:1-Zuordnung (GNN).
