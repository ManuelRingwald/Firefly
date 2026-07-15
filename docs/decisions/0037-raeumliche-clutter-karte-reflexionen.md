# ADR 0037: Räumliche Clutter-Karte + Multipath-Reflexions-Heuristik

**Status:** akzeptiert (2026-07-14, Betreiber-Go SPEC.2) ·
**Bezug:** ARTAS-Gap-Roadmap AP-SPEC (SPEC.2), FR-TRK-046, ADR 0036 (SPEC.1)

## Kontext

Fireflys Clutter-Modell war eine **globale Konstante** λ (Falschplots/m²)
für den gesamten Erfassungsbereich. Real konzentriert sich Clutter
räumlich (Windparks, Straßenverkehr, Vogelzug, Wetter): global heißt
in Hotspots unterschätzt (Geister-Geburten, verzerrte Assoziation) und
in sauberen Regionen überschätzt. Dazu **Mehrwege-Reflexionen**: ein
starkes Ziel nahe einer Reflektorfläche erzeugt ein Geisterecho —
klassisch auf ähnlichem Azimut, hinter dem echten Ziel, ohne SSR.

## Entscheidung

1. **Per-Sensor-Clutter-Karte** (`firefly-track::clutter_map`): polares
   Raster (5-km-Ringe × 64 Sektoren), je Zelle eine **exponentiell
   vergessene Ereignisrate** (`rate ← rate·e^(−Δt/τ) + 1/τ`, τ = 600 s) —
   ereignisgetrieben, O(1) pro Plot, datenzeit-deterministisch,
   snapshot-fähig (Tupel-Schlüssel als Paar-Liste serialisiert).
   **Lernsignal:** Plots, die in keinem Track-Gate liegen (die
   Initiierungs-Kandidaten); die Gründungs-Plots echter Ziele waschen
   über τ aus, persistente Hotspots akkumulieren.
2. **Floor = Default (wichtigste ehrliche Korrektur):** Der Schätzer
   zählt nur Ereignisse, nie Exposition („Scan verging ohne Falschplot").
   Abwesenheit von Ereignissen ist damit **kein** Beleg für eine
   saubere Zelle — die Karte darf λ nur **anheben** (Deckel 100×),
   nie unter den Default senken. Der erste Entwurf (Floor 0,1×) hat
   messbar reale Ziele benachteiligt: zwei Gründungs-Plots drückten das
   lokale λ und kippten eine knappe Zwei-Ziel-Assoziation
   (Regressions-Test `jpda_keeps_two_close_parallel_tracks_distinct`).
   „Saubere Regionen ehrlich niedriger" braucht Expositions-Buchführung
   — explizites Folge-Häppchen.
3. **Per-Track-λ in JPDA** (`joint_association_probabilities_local`):
   der Clutter-Term geht als Faktor **unassignierter Tracks** in die
   Joint-Event-Gewichte ein — die Zelle, durch die der Track fliegt,
   ist die ehrliche Granularität. Bestehende Signatur delegiert mit
   uniformen Dichten (identisches Verhalten ohne Karte).
4. **Reflexions-Heuristik bei Geburt:** Primary-only-Neugeburt auf
   ±2° Azimut eines bestätigten Tracks desselben Radars, ≥ 500 m
   dahinter ⇒ `reflection_suspect`. Wirkung: **Bestätigungs-Schwelle
   +2 Hits** — verzögert, nie exekutiert; eine SSR-Identität löscht
   den Verdacht (Heuristik modelliert nur PSR-Geister).

**Verworfen:** Verdachts-Tracks hart verwerfen (tötet echte Ziele in
Staffelung hintereinander); Karten-Lernen aus allen Plots (echte Ziele
würden ihre eigene Umgebung „verclutten"); zellgenaues λ pro Messung
statt pro Track (der Clutter-Faktor hängt im Joint-Event am Track).

## Konsequenzen

- Kein Wire-/ICD-Bezug, keine Env-Variablen; Karten sind Teil des
  Tracker-Snapshots (additiv, `serde(default)`).
- Metrik-Ausleitung (`firefly_clutter_cells` o. Ä.) bewusst in die
  Betriebs-Härtung verschoben; Test-/Debug-Hook `clutter_cells_total()`.
- Ein echtes Ziel, das PSR-only exakt hinter einem anderen einfliegt,
  bestätigt 2 Umläufe später — dokumentierter Trade.

## Nachtrag SPEC.2b (2026-07-14, Betreiber-Entscheidung „sauber abarbeiten")

Die Floor-Regel aus Punkt 2 ist durch **Expositions-Buchführung**
abgelöst: Die Karte akkumuliert je Sensor **kreditierte Beobachtungszeit**
(`mark_active` je verarbeitetem Batch; eine Aktivitätslücke kreditiert
höchstens 30 s — ein Feed-Ausfall ist keine Beobachtung und reift die
Karte nie). Erst ab **1200 s Reife** (2τ; je Zelle ab ihrem ersten
Ereignis, für ereignisfreie Zellen ab Karten-Start) darf der Floor auf
**0,1 × Default** sinken: Die Region wurde nachweislich beobachtet und
blieb (nahezu) frei von unassoziierten Plots. Unreife Evidenz behält den
Default-Floor — genau die Regel, die die Messerschneiden-Assoziation um
Gründungs-Plots echter Ziele schützt (SPEC.2-Regression bleibt grün, da
Testhorizonte ≪ 1200 s). Metrik `firefly_clutter_cells` exportiert.
Ehrliche Grenzen: Reife unterscheidet nicht zwischen „im Erfassungsbereich
beobachtet" und „außerhalb der Reichweite" (dort ist die Quiet-Behauptung
folgenlos — es entstehen keine Plots); für Nicht-Radar-Sensoren entsteht
eine leere Karte (nur `mark_active`, keine Polar-Zellen), deren
Mature-Quiet-Aussage dieselbe ehrliche Bedeutung trägt.
