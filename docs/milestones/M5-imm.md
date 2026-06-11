# M5 — Manöver und dichter Verkehr: IMM + JPDA

> Verständliche Erklärung des fünften Meilensteins. Teil 1 (M5.1–M5.4) ist der
> **IMM** (Manöver), Teil 2 (M5.5–M5.9) ist **JPDA** (dichter Verkehr,
> überlappende Tore). Begriffe stehen ausführlicher im
> [Glossar](../glossary.md).

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

---

# Teil 2 — JPDA: dichter Verkehr, überlappende Tore

Bis hierhin ordnet der Tracker jedem Track **höchstens einen** Plot pro Scan zu
(GNN, FR-TRK-005) — eine **harte** 0/1-Entscheidung. Das funktioniert gut,
solange die Tore (Gates) der Tracks sich nicht überschneiden. Fliegen zwei
Ziele aber nah beieinander (Formationsflug, Anflug-Staffelung), können
**mehrere Plots in mehreren Toren gleichzeitig** liegen — und eine falsche
harte Entscheidung kann die Tracks vertauschen oder einen davon „entführen".

**JPDA** (*Joint Probabilistic Data Association*) ersetzt die harte Entscheidung
durch eine **weiche**: Jeder Track faltet *alle* seine gegateten Plots auf
einmal ein, gewichtet danach, wie wahrscheinlich jeder einzelne die wahre
Rückmeldung ist — und dabei „wissen" die Tracks *gemeinsam*, dass ein Plot nicht
zu zweien gleichzeitig gehören kann.

## Häppchen M5.5 — PDA: Assoziationswahrscheinlichkeiten `β`

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-015`

### Die Idee (fachlich)

Statt „dieser eine Plot gehört zum Track" oder „nicht", berechnet **PDA**
(*Probabilistic Data Association*) für *jeden* gegateten Plot eine
**Wahrscheinlichkeit** `β_j`, dazu `β_0` für „keiner der Plots gehört zum Track
(Fehldetektion oder Clutter)". Alle `β` zusammen summieren sich zu 1.

### Die Formel (technisch)

Für `m` gegatete Plots mit Likelihoods `Λ_j = N(y_j; 0, S)` (wie gut passt Plot
`j` zur Vorhersage, FR-TRK-013) und einem **Clutter-Term** `b`:

```
β_j = Λ_j / (b + Σ Λ_i)        (j = 1..m)
β_0 = b     / (b + Σ Λ_i)
```

`b = λ·(1 − P_D·P_G) / P_D` fasst zusammen, wie plausibel „Clutter" oder „nicht
gesehen" gegenüber „echter Treffer" ist: `λ` ist die **Clutter-Dichte**
(Falschplots pro Fläche), `P_D` die Erfassungswahrscheinlichkeit, `P_G` die
Gate-Wahrscheinlichkeit (aus der χ²-Schwelle zurückgerechnet, `P_G=1−e^{−γ/2}`).
Ein Plot, der gut passt und in einer ruhigen Umgebung liegt, bekommt ein hohes
`β_j`; viel Clutter macht `β_0` relativ größer — auch für einen ansonsten
perfekten Plot. Implementiert in `pda::association_probabilities`.

---

## Häppchen M5.6 — PDA-gewichtetes Kalman-Update

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-016`

### Die Idee (fachlich)

Die `β` aus M5.5 müssen irgendwie in die Schätzung einfließen. Anstatt einen
Plot „auszuwählen", behandelt `LinearKalman::update_pda` **jede Möglichkeit**
als eigene Hypothese:

- Hypothese 0 („kein Treffer"): der Zustand bleibt die Prädiktion.
- Hypothese `j` („Plot `j` gehört dazu"): ein normales `update(j)`.

### Die Mischung (technisch)

Das Ergebnis ist die `β`-gewichtete Mischung aller Hypothesen — exakt dasselbe
„Spread-of-the-Means"-Muster wie beim IMM-Mixing (M5.2, FR-TRK-012):

```
x = Σ β_k · x_k
P = Σ β_k · (P_k + d_k·d_kᵀ)     mit d_k = x_k − x
```

Sind sich die Hypothesen einig (z. B. nur ein sehr wahrscheinlicher Plot), bleibt
`P` klein. Streiten sich mehrere plausible Plots um den Track, bläht der
`d_k·d_kᵀ`-Term `P` ehrlich auf — der Filter „weiß", dass er gerade zwischen
Möglichkeiten unentschieden ist. Zwei Grenzfälle beweisen die Korrektheit:
`betas=[1.0]` (keine Messung) ist ein Noop, `betas=[0,1]` (sicherer Treffer)
ist identisch zum bisherigen `update`.

---

## Häppchen M5.7 — PDA-gewichtetes IMM-Update

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-017`

Da jeder Track inzwischen eine **IMM-Bank** statt eines einzelnen Filters trägt
(M5.4), muss M5.6 auf die ganze Bank übertragen werden. `Imm::update_pda`:

- **Zweig 0** („kein Treffer"): die Bank bleibt exakt so, wie `predict` sie
  hinterlassen hat (Markov-prädizierte Modellwahrscheinlichkeiten, prädizierte
  Filter).
- **Zweig `1+j`** („Plot `j`"): ein vollständiger `Imm::update(measurements[j])`
  auf einer Kopie der Bank — inklusive eigener
  Modellwahrscheinlichkeits-Neubewertung (FR-TRK-013).

Die neue Bank entsteht durch `β`-gewichtetes Mischen **je Modell** (wie M5.6)
und durch `β`-gewichtetes Mischen+Renormieren der **Modellwahrscheinlichkeiten**
über alle Zweige. Auch hier gelten dieselben Grenzfall-Beweise wie in M5.6.

---

## Häppchen M5.8 — JPDA: gemeinsame Exklusivität

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-018`

### Das Problem (fachlich)

PDA (M5.5) behandelt jeden Track für sich. Liegt derselbe Plot im Tor von
**zwei** Tracks, rechnet jeder Track unabhängig „könnte meiner sein" — beide
können demselben Plot ein hohes `β` geben, *als gäbe es den anderen Track
nicht*. In Wirklichkeit kann der Plot aber nur von **einem** der beiden
stammen.

### Die Lösung (technisch)

`joint_association_probabilities` löst das in drei Schritten:

1. **Clustern:** Tracks und Plots, die über gemeinsame Tore verbunden sind,
   bilden ein **Cluster** (Union-Find). Unabhängige Tracks/Plots werden wie
   bisher (PDA bzw. „sicher kein Treffer") behandelt — der Mehraufwand bleibt
   auf die wenigen Stellen begrenzt, wo es wirklich eng wird.
2. **Gemeinsame Ereignisse aufzählen:** Für jedes Cluster zählt
   `ClusterEnumerator` per Backtracking alle **zulässigen** Zuordnungen auf —
   jede Kombination, in der jeder Plot höchstens einem Track zugeordnet ist
   (oder gar keinem). Jedes Ereignis bekommt ein Gewicht
   `∏ (Λ_ij wenn zugeordnet, sonst b)`.
3. **Marginalisieren:** `β_ij` = Summe der Ereignisgewichte, in denen Track `i`
   mit Plot `j` zusammen vorkommt, geteilt durch die Summe aller
   Ereignisgewichte des Clusters.

Für `t=1` (ein einzelner Track) fällt diese Formel exakt auf die PDA-Formel aus
M5.5 zurück — ein direkter Korrektheitsbeweis (`single_track_matches_per_track_pda`).
Für zwei Tracks, die sich einen Plot teilen, ist das gemeinsame `β` für diesen
Plot **strikt kleiner** als das (zu optimistische) PDA-`β` jedes Tracks für
sich (`overlapping_tracks_split_a_shared_plot`) — die Exklusivität „dämpft"
beide Tracks gegenseitig.

In realer Luftlage sind Cluster klein (eine Handvoll Tracks/Plots), sodass die
im schlimmsten Fall exponentielle Aufzählung in der Praxis nur eine Handvoll
Ereignisse umfasst — ein bewusster, dokumentierter Komplexitäts-Kompromiss.

---

## Häppchen M5.9 — JPDA im Tracker

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-019`

Der letzte Schritt verdrahtet M5.5–M5.8 in den Pro-Scan-Ablauf: Die harte
GNN-Zuordnung (`associate`, FR-TRK-005) ist je Sensor durch
`joint_association_probabilities` ersetzt. Für jeden Track:

- Sind alle gegateten `β` praktisch 0 außer `β_0` (`β_0 ≥ 1−ε`), liegt **kein**
  Plot im Tor — der Track coastet wie bisher.
- Sonst faltet `Imm::update_pda` (M5.7) **alle** gegateten Plots mit ihren
  gemeinsamen `β`-Gewichten ein; der Track gilt als getroffen
  (`last_hit_time`, `record_hit_from`). Der Plot mit dem größten `β` liefert
  die SSR-Identität (FR-TRK-009) — Identität ist eine diskrete
  „Ja/Nein/Welcher"-Information, die sich nicht sinnvoll kinematisch mischen
  lässt.
- Plots, die in **keinem** Tor landen, gründen wie bisher einen neuen Track.

`TrackerConfig` bekommt dafür ein `ClutterModel` (Default: ~1 Falschplot pro
10 km², `P_D=0,95`) — eine sparsame Clutter-Umgebung, die für isolierte Tracks
kaum etwas ändert, aber bei überlappenden Toren die Exklusivitäts-Überlegung
mit Substanz füllt.

### Trade-off: Clutter-Dichte und „Verschmieren" sauberer Tracks

Selbst eine winzige Clutter-Dichte ergibt pro Scan ein kleines `β_0 > 0` —
auch für einen perfekt isolierten, einzelnen Track. Der
Spread-of-the-Means-Term `β_0·d·d^T` (M5.6) addiert dann etwas Unsicherheit,
selbst wenn nichts „strittig" ist. Bei `λ=10⁻⁶` (≈1 Falschplot/km²) reichte das,
um den Positions-RMSE eines sauberen Geradeausflugs leicht über die
40-m-Schwelle (FR-TRK-007) zu heben. Mit `λ=10⁻⁷` (≈1 Falschplot/10 km²) bleibt
der Effekt auf saubere Einzeltracks vernachlässigbar, während er bei
überlappenden Toren weiterhin wirkt — ein bewusst getunter, dokumentierter
Kompromiss.

### Bekanntes Merkmal: Track-Koaleszenz

Der End-to-End-Test `jpda_keeps_two_close_parallel_tracks_distinct` lässt zwei
Ziele 100 m auseinander parallel fliegen, mit jeden Scan überlappenden Toren.
Ergebnis: Beide bleiben **zwei** unterscheidbare, bestätigte Tracks — aber ihre
Schätzungen rücken etwas **näher zusammen** als die wahren 100 m
(„**Track-Koaleszenz**", siehe Glossar). Das ist eine **bekannte, dokumentierte
Eigenschaft** von JPDA: weil jeder Plot weich auf beide Tracks verteilt wird,
zieht jeder Track auch den Plot des Nachbarn ein Stück mit ein. Eine vollständige
Trennung erfordert fortgeschrittenere Verfahren (z. B. zusätzliche
Merkmale/Identität in der Assoziation) und ist bewusst **nicht** Teil dieses
Häppchens.

---

## Warum das zählt (Sicherheit & Lagebild), Teil 2

Gerade in den Situationen, in denen es am meisten zählt — Staffelung im Anflug,
Formationsflug, dichte Warteschleifen — ist eine **falsche harte Zuordnung**
gefährlicher als eine etwas unschärfere, aber ehrliche weiche Schätzung. JPDA
tauscht „vielleicht falsch, aber präzise" gegen „garantiert plausibel, mit
ehrlich ausgewiesener Unsicherheit" — und behält dabei für *jedes* Ziel eine
durchgehende Identität, was für nachgelagerte Funktionen (Konfliktwarnung,
EFS-Korrelation) entscheidend ist.

**Damit ist M5 (IMM + JPDA) vollständig abgeschlossen.**
