# ADR 0036: Identität als weicher Schlüssel + JPDA-Koaleszenz-Wächter

**Status:** akzeptiert (2026-07-14, Betreiber-Go SPEC.1) ·
**Bezug:** ARTAS-Gap-Roadmap AP-SPEC (SPEC.1), FR-TRK-031/037/045,
`docs/design/korrelation-code-duplikate-weeze.md` (Betriebs-Lektion)

## Kontext

Zwei reale Störfälle: (1) **Duplikat-Identitäten** — Squawks sind per
ORCAM nur regional eindeutig (an Grenzen wie Weeze doppelt belegt,
Conspicuity 1000 per Design mehrdeutig), und selbst die „global
eindeutige" ICAO-Adresse dupliziert im Feld durch Transponder-
Fehlkonfiguration. Der bisherige ICAO-Fastpath (FR-TRK-031) assoziierte
einen Adress-Match **ohne Gate-Prüfung** hart — bei zwei Luftfahrzeugen
mit derselben Adresse teleportiert der Träger-Track zwischen beiden
(„springender Misch-Track"), und die Registrierungs-Paarung
(FR-TRK-037) korreliert Sichtungen zweier verschiedener Maschinen zu
falscher Bias-Evidenz. (2) **Track-Koaleszenz** — die bekannte
strukturelle JPDA-Schwäche: ein dauerhaft unaufgelöstes Track-Paar teilt
jeden Plot probabilistisch, beide Schätzungen driften auf den
gemeinsamen Mittelpunkt (gemessen: 150-m-Paar → ≤ 113 m in 40 Scans).

## Entscheidung

1. **Identität ist ein weicher Schlüssel.** Der Fastpath assoziiert nur
   noch **im kinematischen Gate** hart; ein Match außerhalb fällt in
   JPDA/Initiierung durch und gründet einen eigenen Track. Duplikate
   werden **getrennt geführt und geflaggt** (`identity_conflict`,
   Voll-Rescan je Fusions-Gelegenheit, WARN beim Neuauftreten) — niemals
   gemergt. Das Flag bleibt vorerst tracker-intern (Log = Betriebs-
   Sichtbarkeit); eine Draht-/WS-Ausleitung ist bewusst aufgeschoben,
   bis die Flugplan-Korrelation (AP-FPL) den Konsumenten definiert.
2. **Koaleszenz-Wächter statt JPDA-Umbau.** Für statistisch
   unauflösbare Paare (d² < 4 unter kombinierter Positions-Kovarianz)
   wird je **geteilter** Messung die Hypothese des schwächeren
   Anwärters beschnitten (β-Masse → dessen β₀; deterministischer
   Tie-Break). Einfachstes Mitglied der Pruning-Familie: chirurgisch,
   im Normalverkehr No-op, kein Eingriff in die JPDA-Mathematik selbst.
3. **Registrierungs-Deckel.** Korrespondenzen mit > 5 km
   Sichtungs-Abstand sind Identitäts-Kontamination, keine Bias-Evidenz
   (echte Biases: Meter bis wenige hundert Meter) — verworfen, bevor
   sie die Least-Squares-Schätzung vergiften. Kinematisch statt
   zeitfenster-basiert: ein Zeitfenster kann Duplikat nicht von
   Scan-Wiederbesuch unterscheiden (azimutabhängige Datenzeiten).

**Verworfen:** Track-Merge bei Identitätsgleichheit (zerstört im
Duplikatfall ein echtes Ziel); exklusive Nearest-Neighbor-Assoziation
generell (verliert die JPDA-Stärken im Normalfall); Koaleszenz-Abwehr
über Repulsions-Terme (unphysikalische Zustands-Manipulation).

## Konsequenzen

- Kein Wire-/ICD-Bezug, keine Env-Variablen; `identity_conflict` ist
  serde-additiv im Track-Snapshot.
- Ein legitimer schneller Manöver-Ausreißer eines ICAO-Trägers, der das
  Gate sprengt, wird nicht mehr hart eingefangen — er läuft über
  JPDA/Koast und re-assoziiert kinematisch; bewusster Trade zugunsten
  der Duplikat-Sicherheit.
- Der Wächter greift erst bei 2σ-Unauflösbarkeit — sich kreuzende, klar
  getrennte Ziele behalten die volle probabilistische JPDA-Behandlung.
