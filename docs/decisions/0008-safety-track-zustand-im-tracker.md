# ADR 0008 — Safety-relevante Track-Zustandsentscheidung liegt im Tracker

- **Status:** akzeptiert
- **Datum:** 2026-06-10

## Kontext

Eine Lagedarstellung (ASD) zeigt dem Lotsen Tracks. Dabei gibt es eine
**safety-relevante Entscheidung**: Ist ein Track vertrauenswürdig genug, um als
Teil der Luftlage gezeigt zu werden — oder ist er durch fehlende Plots so
unsicher geworden, dass er anders markiert oder gar nicht mehr gezeigt werden
darf? Diese Entscheidung ist in **beide Richtungen** gefährlich: Eine schlechte
Spur als verlässlich zu zeigen ist riskant — eine **echte** Spur fälschlich
fallenzulassen (ein Luftfahrzeug verschwindet aus der Lage) ist mindestens so
riskant.

Die Frage ist: **Wo** fällt diese Entscheidung? Würde sie in der **ASD** fallen
(„die Daten sind zu alt, blende den Track aus"), würde aus der eigentlich
darstellenden ASD ein **safety-kritisches Element mit hoher Assurance-Stufe
(SWAL)** — die Entscheidungslogik läge in einem Bauteil, das dafür nicht gebaut
und nicht abgesichert ist. Das ist genau die Vermischung, die ein zuverlässiges
System vermeiden muss (vgl. ADR 0004 Zertifizierungs-Fähigkeit, ED-153 SWAL).

## Entscheidung

1. **Die safety-relevante Track-Zustandsentscheidung liegt im Tracker.** Der
   Tracker entscheidet — auf **Datenzeit**, deterministisch (ADR 0003) — ob ein
   Track bestätigt, *coasting* (extrapoliert, ohne frische Messung) oder zu
   unsicher zum Weiterführen ist. Das **Fallenlassen** (Löschung) gibt es bereits
   (`delete_misses_*`, gezählt in Fehltreffern); ergänzt wird ein **expliziter
   Coasting-/Güte-Zustand** vor dem Drop.
2. **Der Tracker liefert diesen Zustand explizit und selbstbeschreibend aus.**
   Der `SystemTrack` trägt neben Position/Geschwindigkeit den **safety-relevanten
   Status**: Coasting-Indikator, **Update-Alter** (Datenzeit seit letztem realen
   Treffer) und ein **Genauigkeits-/Unsicherheitsmaß** (aus der Filter-Kovarianz
   `P`). Damit muss kein nachgelagertes Bauteil Vertrauenswürdigkeit *herleiten*.
3. **Adapter kodiert, ASD stellt nur dar.** Der CAT062-Adapter **überträgt** den
   Tracker-Zustand treu ins Draht-Format; die ASD **rendert** ihn (z. B. coasting
   gestrichelt). Beide treffen **keine** safety-relevante Entscheidung mehr — die
   hohe SWAL bleibt im Tracker.
4. **Policy ist Konfiguration, Durchsetzung bleibt im Tracker.** Schwellenwerte
   (wie lange coasten, ab welcher Unsicherheit fallenlassen) sind konfigurierbar
   und rückverfolgbar; *entschieden und durchgesetzt* werden sie im Tracker, nicht
   in einem „dummen" Konsumenten.

## Begründung

- **SWAL-Allokation:** Das Bauteil, das die Entscheidung trifft, muss die
  Assurance tragen. Hält man die Entscheidung im (ohnehin assured) Tracker, bleibt
  die ASD ein darstellendes Element niedriger Kritikalität.
- **Die Norm ist genau so gebaut:** ASTERIX CAT062 trägt **Track Status**
  (I062/080: u. a. CNF confirmed/tentative, CST coasting), **Track Update Ages**
  (I062/290, in Datenzeit) und **Estimated Accuracies** (I062/500, die Kovarianz).
  CAT062 *erwartet*, dass der Tracker Status + Alter + Genauigkeit liefert und der
  Konsument nur darstellt — sonst gäbe es diese Felder nicht.
- **Kein Widerspruch zum No-Wanduhr-Prinzip (NFR-CLOUD-004):** Die
  Coasting-/Drop-Entscheidung läuft auf **Datenzeit** (Update-Alter in
  Datensekunden / Fehltreffer-Zahl), nicht auf der Wanduhr. Der Tracker bleibt
  gleichzeitig immun gegen Verarbeitungs-Verzug **und** die Safety-Autorität.

## Abgrenzung (was hier *nicht* entschieden wird)

- Das konkrete **Drop-Kriterium** (reine Coast-Anzahl als Proxy vs. direktes
  **Kovarianz**-Maß) wird im umsetzenden Häppchen festgelegt; beide sind
  datenzeit-/zustandsbasiert und tracker-seitig.
- **Kosmetische** Darstellungsregeln (Farbe, Strichelung) bleiben Sache der ASD —
  das ist Darstellung, keine Entscheidung.

## Konsequenzen

- Der `SystemTrack` wird um den safety-relevanten Status erweitert
  (Coasting-Indikator, Update-Alter, Unsicherheitsmaß) — eigenes kleines Häppchen,
  das direkt CAT062 (I062/080, /290, /500) vorbereitet.
- Neue Anforderung im Register (Tracker als Entscheider über die Track-Güte/den
  Lebenszyklus-Status der Ausgabe), rückverfolgbar getestet.
- Die ASD-/Adapter-Seite bleibt darstellend; ihre Assurance betrifft *treue
  Wiedergabe*, nicht *Entscheidung*.
