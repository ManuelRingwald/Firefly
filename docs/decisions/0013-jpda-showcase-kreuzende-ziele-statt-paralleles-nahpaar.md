# ADR 0013 — JPDA-Showcase: kreuzende Ziele statt parallelem Nahpaar

- **Status:** akzeptiert
- **Datum:** 2026-06-12

## Kontext

Die Frankfurt-Showcase-Szene (ADR 0011, Meilenstein M6) enthielt als
JPDA-Schaustück zwei **parallele** West-Anflüge (`arrival_west_a`/`b`), die nur
**~150 m** auseinander flogen. Die Idee: Ihre Tore überlappen sich, und JPDA
soll zeigen, dass es die beiden Spuren trotzdem getrennt hält.

In der laufenden Demo zeigte sich jedoch, dass die beiden Spuren nach gut einer
Minute **vollständig verschmolzen** — auf der Karte lag nur noch ein sichtbarer
Track, der zweite war dauerhaft darunter verborgen. Ein eigens geschriebener
Regressionstest bestätigte das reproduzierbar (minimaler Spurabstand 0,0 m statt
der erwarteten ~150 m).

### Untersuchung

Die naheliegende Diagnose war **Track-Koaleszenz** (siehe Glossar) — die
bekannte JPDA-Schwäche, bei der die weiche Zuordnung naher Tracks deren
Schätzungen zueinander zieht. Der kanonische Gegenentwurf ist **JPDA\*** (Blom &
Bloem, *Probabilistic data association avoiding track coalescence*, IEEE TAC
2000): Statt die Ereignisgewichte je `(Track, Plot)`-Zelle zu **summieren**
(Marginalisierung), behält man das **Maximum** (das wahrscheinlichste gemeinsame
Ereignis) und normiert zeilenweise.

JPDA\* wurde prototypisch implementiert und **empirisch gemessen**. Ergebnis:

| Abstand der Ziele | ≈ Vielfaches von σ | einfaches JPDA | JPDA\* |
|-------------------|--------------------|----------------|--------|
| ≤ 200 m | ≤ 2,9σ | → 0 m (verschmilzt) | → 0 m (verschmilzt) |
| ~280 m | ~4,0σ | 37 m | 40 m |
| ≥ 300 m | ≥ 4,3σ | hält exakt getrennt | hält exakt getrennt |

**Beide Verfahren sind praktisch identisch.** Die entscheidende Erkenntnis:

1. Die klassische, durch JPDA\* behebbare Koaleszenz **tritt in diesem Tracker
   gar nicht auf** — schon das einfache JPDA hält *auflösbare* Paare (≥ ~4σ)
   exakt getrennt. JPDA\* bringt hier **keinen messbaren Vorteil**.
2. Das Verschmelzen unterhalb ~3σ ist **keine Algorithmus-Schwäche, sondern die
   physikalische Auflösungsgrenze** (siehe Glossar): Bei dem in der Szene
   wirksamen Messrauschen (~70 m quer zur Sichtlinie, 1σ) sind 150 m nur ~2,1σ.
   Zwei so nahe Radar-Rückmeldungen lassen sich von *keinem*
   Datenassoziations-Verfahren trennen — die Information dafür steckt nicht in
   den Daten. Das Verschmelzen ist hier **korrektes Verhalten**.

Das ursprüngliche Showcase-Konzept war damit in sich widersprüchlich: Genau dort,
wo sich die Tore stark überlappen (< ~4σ), ist das Paar *grundsätzlich*
unauflösbar; und dort, wo es auflösbar ist (≥ ~4σ), überlappen sich die Tore
kaum noch — es gibt keinen Abstand, bei dem „Tore überlappen **und** parallele
Ziele bleiben unterscheidbar" für *irgendein* Verfahren gilt.

## Entscheidung

**Der JPDA-Showcase wird von einem parallelen Nahpaar auf zwei _kreuzende_ Ziele
umgestellt; JPDA\* wird _nicht_ eingeführt.**

- `arrival_west_a`/`arrival_west_b` werden durch `crossing_northeast` und
  `crossing_southeast` ersetzt: zwei Ziele, die sich an einem gemeinsamen Punkt
  (ENU ≈ (−30 km, 0)) zur gemeinsamen Zeit (t ≈ 120 s) und auf gleicher Höhe
  treffen, mit 90° auseinanderliegenden Kursen (NE 45° bzw. SE 135°,
  je 180 m/s).
- Kreuzende Ziele sind der **fachlich aussagekräftige** JPDA-Fall: Am
  Kreuzungspunkt sind die Plots kurz mehrdeutig (Tore überlappen), aber jedes
  Ziel trägt einen **kinematischen Unterschied** (Geschwindigkeitsrichtung). Der
  Geschwindigkeitszustand im Kalman-/IMM-Filter trägt jede Spur auf die richtige
  Seite weiter, sodass die Identität erhalten bleibt und **kein Identitätstausch**
  (siehe Glossar) auftritt — die Gefahr, der eine *harte* 1:1-Zuordnung am
  Kreuzungspunkt ausgesetzt wäre.
- **JPDA\* wird verworfen.** Es würde dem Code Komplexität hinzufügen, ohne in
  diesem Tracker einen messbaren Nutzen zu stiften. Eine „Lösung" zu liefern, die
  nachweislich nichts ändert (und sie mit einem ADR zu rechtfertigen), widerspräche
  dem Charta-Prinzip der *ehrlichen Grenze*. Sollte sich später ein Szenario
  zeigen, in dem klassische Koaleszenz bei *auflösbaren* Zielen real auftritt,
  kann JPDA\* mit dann belastbarer Begründung nachgezogen werden.

### Verifikation

Der frühere Test `frankfurt_close_pair_does_not_coalesce` (rote Lampe für die
unauflösbare Situation) wird ersetzt durch
`frankfurt_crossing_pair_keeps_identity_through_the_crossing`. Er prüft aus dem
Frame-Strom allein:

1. Das Paar kommt sich real nahe (minimaler Abstand < 1 km — die Tore überlappen
   tatsächlich),
2. trennt sich danach wieder weit (Endabstand > 10 km — **kein** Verschmelzen),
3. und beide Kurse bleiben über den ganzen Lauf in **disjunkten Quadranten**
   (NE-Flieger durchgehend < 90°, SE-Flieger > 90°) — ein Identitätstausch an der
   Kreuzung würde diese Invariante verletzen.

## Konsequenzen

**Positiv**

- Der Showcase demonstriert jetzt eine echte JPDA-Stärke (Identitätserhalt durch
  eine Kreuzung) statt einer physikalisch unmöglichen Forderung.
- Keine zusätzliche Komplexität im JPDA-Kern (`jpda.rs` bleibt unverändert).
- Die Doku (Glossar: *Auflösungsgrenze*, *Identitätstausch*, präzisierte
  *Track-Koaleszenz*) hält die Erkenntnis fest — auch als Beleg dafür, *warum*
  bestimmte „Bugs" keine sind.

**Negativ / Grenzen**

- Der Tracker bleibt bei *parallelen* Zielen näher als ~4σ ohne Trennung. Das ist
  bewusst akzeptiert: Es ist die physikalische Grenze, kein behebbarer Mangel.
  Wer sie verschieben will, muss an der *Sensorauflösung* (engeres Azimut-σ)
  ansetzen, nicht an der Datenassoziation.
- Die Aussage „JPDA\* bringt hier nichts" gilt für die *aktuelle*
  Tracker-Architektur (eingefrorene Fusions-Referenz, ADR 0011; enges
  Assoziations-Tor). Bei größeren, dichteren Clustern könnte sich das ändern —
  dann ist diese Entscheidung neu zu bewerten.
