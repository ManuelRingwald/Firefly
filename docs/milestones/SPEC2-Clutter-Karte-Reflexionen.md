# SPEC.2 — Räumliche Clutter-Karte + Reflexions-Heuristik

> **Anforderung:** FR-TRK-046 · **ADR:** 0037 · **ICD:** unberührt ·
> **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich

Clutter ist räumlich konzentriert; ein globales λ unterschätzt Hotspots
(Geister-Geburten) und überschätzt saubere Regionen. Mehrwege-Reflexionen
erzeugen Geisterechos hinter echten Zielen. Beides ARTAS-Kernfunktionen.

## Technik

Per-Sensor-Karte (5-km-Ringe × 64 Sektoren, exponentiell vergessene
Ereignisrate τ = 600 s) aus unassoziierten Plots; JPDA nutzt das **lokale
λ pro Track** (`joint_association_probabilities_local` — der Clutter-Term
hängt im Joint-Event am unassignierten Track). Reflexions-Verdacht bei
Geburt (Primary-only, ±2°, ≥ 500 m hinter bestätigtem Track, gleiches
Radar) hebt nur die Bestätigungs-Schwelle (+2 Hits); SSR löscht ihn.

## Die wichtigste Design-Korrektur (ehrlich)

Der Erst-Entwurf erlaubte der Karte, λ **unter** den Default zu senken
(Floor 0,1×). Der Regressions-Test
`jpda_keeps_two_close_parallel_tracks_distinct` riss: zwei Gründungs-Plots
echter Ziele hatten das lokale λ gedrückt und eine knappe Zwei-Ziel-
Assoziation gekippt. Wurzel: der Schätzer zählt nur **Ereignisse**, nie
**Exposition** — Ereignis-Abwesenheit ist kein Sauberkeits-Beleg.
Korrektur: **Floor = Default** (Karte hebt nur an, Deckel 100×). „Saubere
Regionen ehrlich niedriger" braucht Expositions-Buchführung — explizites
Folge-Häppchen. Damit weicht die Umsetzung bewusst von der Ankündigung ab
(die Überschätzung in sauberen Regionen bleibt vorerst bestehen).

## Ehrliche Grenzen

- **Nur Hotspot-Anhebung** (s. o.); Zellen ohne Ereignisse = Default.
- Lernsignal enthält Gründungs-Plots echter Ziele (waschen über τ aus).
- Heuristik modelliert nur **PSR-only**-Geister; SSR-spiegelnde
  Reflexionen (selten, möglich) bleiben unerkannt.
- Ein echtes PSR-only-Ziel exakt hinter einem anderen bestätigt
  2 Umläufe später (nie exekutiert) — dokumentierter Trade.
- Metrik-Ausleitung → Betriebs-Härtung (`clutter_cells_total()` als Hook).
