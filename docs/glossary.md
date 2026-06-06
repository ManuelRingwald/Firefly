# Glossar — Fachbegriffe in einfacher Sprache

Dieses Lexikon wächst mit dem Projekt. Jeder Begriff wird einmal so erklärt, dass
man ihn ohne Vorwissen versteht — oft mit einer Analogie. Reihenfolge:
thematisch, nicht alphabetisch, damit Zusammenhänge sichtbar bleiben.

---

## Luftlage & Sensorik (die Fachwelt)

**ANSP** (*Air Navigation Service Provider*, Flugsicherungsorganisation)
Eine Organisation, die den Luftverkehr überwacht und lenkt (z. B. DFS in
Deutschland, Austro Control in Österreich). Sie betreibt die Radare und die
Systeme, die daraus ein Luftlagebild erzeugen.

**Luftlage / Luftlagedarstellung**
Das Gesamtbild „Wer fliegt gerade wo, wie schnell, in welche Richtung?". Der
Tracker ist das Rechenherz, das dieses Bild aus rohen Radarmeldungen erzeugt.

**Primärradar (PSR, *Primary Surveillance Radar*)**
Sendet einen Funkimpuls aus und misst das Echo, das vom Flugzeugrumpf
zurückgeworfen wird — wie eine Taschenlampe, deren Lichtreflex man sieht. Es
liefert nur **Entfernung und Richtung**, *keine* Identität und *keine* Höhe. Es
sieht aber auch Objekte, die nicht „antworten" wollen.

**Sekundärradar (SSR, *Secondary Surveillance Radar*)**
Stellt eine Frage („Interrogation") an das Flugzeug; ein Gerät an Bord (der
**Transponder**) antwortet aktiv. Dadurch bekommt man zusätzlich Identität und
Höhe — aber nur von kooperierenden Flugzeugen. Wie ein Zuruf „Wer da?", auf den
das Flugzeug seinen Namen und seine Höhe zurückruft.

**Transponder**
Das Antwortgerät an Bord, das auf SSR-Abfragen reagiert.

**Mode A / Mode C / Mode S** (SSR-Antwortarten)
- **Mode A:** liefert den **Squawk** — einen 4-stelligen Code (oktal, 0000–7777),
  den der Lotse dem Flug zuweist, zur Unterscheidung.
- **Mode C:** liefert die **barometrische Höhe** (aus dem Luftdruck), meist als
  *Flugfläche*.
- **Mode S:** moderner, selektiv adressierbar; liefert u. a. die weltweit
  eindeutige **24-Bit-ICAO-Adresse** des Flugzeugs und Zusatzdaten.

**Squawk (Mode-3/A-Code)**
Der 4-stellige Identifikationscode, z. B. „7000". `7700` = Notfall, `7600` =
Funkausfall, `7500` = Entführung.

**Flugfläche (FL, *Flight Level*)**
Höhe in Hunderten Fuß, bezogen auf einen genormten Standard-Luftdruck
(1013,25 hPa). „FL350" ≈ 35 000 Fuß ≈ 10 668 m. Wichtig: bezogen auf Druck, nicht
auf den echten geometrischen Abstand zum Boden.

**ADS-B** (*Automatic Dependent Surveillance – Broadcast*)
Das Flugzeug bestimmt seine Position selbst (per Satellitennavigation) und
funkt sie laufend aus. Kein Radar nötig — der Empfänger hört einfach zu.

**ASTERIX** (*All-purpose Structured Eurocontrol Surveillance Information
Exchange*)
Das europäische Standard-Datenformat, in dem Radare und Systeme ihre Meldungen
austauschen. In „Kategorien" gegliedert:
- **CAT048:** Einzelradar-Zielmeldungen (Plots/Tracks eines Radars).
- **CAT021:** ADS-B-Meldungen.
- **CAT062:** fertige System-Tracks (die fusionierte Luftlage).

---

## Tracking-Grundbegriffe (was der Tracker tut)

**Plot (Zielmeldung)**
Eine **einzelne Detektion** eines Ziels bei **einem** Antennenumlauf eines
Radars: „Zum Zeitpunkt t war bei dieser Entfernung/Richtung etwas." Roh,
verrauscht, evtl. fehlend. Der Rohstoff.

**Track (Spur)**
Die **fortlaufend geschätzte Bahn** eines Objekts über die Zeit, zusammengesetzt
aus vielen Plots: geglättete Position, Geschwindigkeit, Richtung, Identität. Das
Endprodukt. Aus vielen Einzelpunkten („Plots") wird eine durchgehende Linie.

**Scan / Antennenumlauf**
Eine Radarantenne dreht sich. Ein voller Umlauf (z. B. alle 4 s) ist ein „Scan";
pro Scan entsteht höchstens ein Plot je Ziel.

**Erfassungswahrscheinlichkeit (Pd, *Probability of Detection*)**
Die Chance, ein vorhandenes Ziel bei einem Scan tatsächlich zu sehen (z. B. 0,9
= 90 %). Real verpasst ein Radar manchmal ein Ziel — der Tracker muss damit
umgehen.

**Falschalarm / Clutter**
Eine Detektion, hinter der gar kein echtes Flugzeug steckt (Vögel, Wetter,
Reflexionen). Der Tracker muss solche „Geister" aussortieren.

**Track-Initiierung**
Das Geburtsverfahren eines Tracks: Aus bisher nicht zugeordneten Plots wird ein
neuer („tentativer") Track gebildet, der sich erst über mehrere Scans bewähren
muss (oft „M aus N": z. B. in 3 von 4 Scans wiedergesehen).

**Coasting**
Wenn ein bestätigter Track in einem Scan keinen Plot bekommt (Ziel kurz nicht
gesehen), „segelt" er auf Basis der Vorhersage weiter, statt sofort zu sterben.

**Gating**
Bevor man fragt „Welcher Plot gehört zu welchem Track?", grenzt man den
Suchbereich ein: Nur Plots in einem plausiblen Fenster um die Vorhersage kommen
in Frage. Spart Rechenzeit und verhindert Unsinn. Das „Tor" (Gate) ist dieses
Plausibilitätsfenster.

**Datenassoziation**
Die Kernfrage: Welcher der vorhandenen Plots gehört zu welchem Track? Verfahren
(von einfach zu komplex):
- **NN / GNN** (*(Global) Nearest Neighbor*): nimm den/die nächstgelegenen Plots.
- **JPDA / MHT**: berücksichtigen mehrere Möglichkeiten gleichzeitig — wichtig
  bei dichtem Verkehr, wo Zuordnungen mehrdeutig sind.

**Kalman-Filter**
Das mathematische Herzstück der Glättung. Es kombiniert (a) eine **Vorhersage**
(„wo müsste das Ziel jetzt sein, wenn es so weiterfliegt?") mit (b) einer neuen,
verrauschten **Messung**, und gewichtet beide nach ihrer Unsicherheit. Ergebnis:
eine bessere Schätzung als jede der beiden allein. Analogie: ein erfahrener
Beobachter, der seine Erwartung und das, was er gerade sieht, klug
zusammenbringt.

**Bewegungsmodell**
Die Annahme darüber, *wie* ein Ziel sich bewegt:
- **CV** (*Constant Velocity*): gleichförmig geradeaus.
- **CA** (*Constant Acceleration*): gleichmäßig beschleunigend.
- **CT** (*Coordinated Turn*): saubere Kurve mit konstanter Drehrate.

**IMM** (*Interacting Multiple Model*)
Lässt mehrere Bewegungsmodelle parallel laufen und gewichtet sie laufend — gut
für Flugzeuge, die mal geradeaus fliegen, mal Kurven fliegen.

**Multi-Radar-Fusion**
Mehrere Radare sehen dasselbe Ziel. Fusion bedeutet, ihre Meldungen zeitlich
abzugleichen, systematische Messfehler (Bias) zu korrigieren und zu *einem*
gemeinsamen Track zusammenzuführen.

---

## Geometrie & Koordinaten (wo ist „wo"?)

**Polar vs. kartesisch**
- **Polar:** Position als Entfernung + Winkel — so *misst* ein Radar (es kennt
  „wie weit" und „in welche Richtung").
- **Kartesisch:** Position als X/Y/Z-Werte auf einem Gitter — so *rechnet* man
  bequem (Geschwindigkeiten, gerade Linien).
Der Tracker übersetzt ständig zwischen beiden Welten.

**Range / Azimut / Elevation** (die polaren Messgrößen)
- **Range:** Schrägentfernung vom Radar zum Ziel (Meter).
- **Azimut:** Horizontalwinkel, von Nord im Uhrzeigersinn (0°=Nord, 90°=Ost).
- **Elevation:** Höhenwinkel über dem Horizont.

**WGS84**
Das weltweite Standard-Erdmodell (eine leicht abgeplattete Kugel, „Ellipsoid"),
auf das sich Längengrad/Breitengrad/Höhe beziehen — auch die Grundlage von GPS.

**ECEF** (*Earth-Centered, Earth-Fixed*)
Ein kartesisches Koordinatensystem mit Ursprung im Erdmittelpunkt, das sich mit
der Erde mitdreht. Praktischer Zwischenschritt beim Umrechnen.

**ENU** (*East-North-Up*, lokales Tangentialsystem)
Ein lokales X/Y/Z-System, das in einem Bezugspunkt (z. B. dem Radarstandort) auf
die Erdoberfläche „aufgesetzt" wird: X=Ost, Y=Nord, Z=Hoch. In diesem flachen
System ist Fliegen näherungsweise „geradeaus = gerade Linie" — ideal fürs
Tracking. Wie ein lokaler Stadtplan statt eines Globus.

**Geodäsie**
Die Lehre vom Vermessen der Erde — hier konkret das korrekte Umrechnen zwischen
WGS84, ECEF, ENU und polaren Radarkoordinaten.

---

## Mathematik & Statistik (das Handwerkszeug)

**Messrauschen**
Die zufällige Ungenauigkeit jeder Messung. Ein Radar misst nie exakt — die
Entfernung „streut" um den wahren Wert.

**Standardabweichung (σ, „Sigma")**
Ein Maß dafür, *wie stark* eine Größe um ihren Mittelwert streut. Kleines σ =
präzise, großes σ = ungenau.

**Normalverteilung / Gauß**
Die typische „Glockenkurve" der Streuung: kleine Abweichungen häufig, große
selten. Mess- und Modellfehler werden meist so beschrieben.

**Kovarianz / Kovarianzmatrix**
Die „Unsicherheit in mehreren Dimensionen" — beschreibt nicht nur, wie ungenau
einzelne Werte sind, sondern auch, wie ihre Fehler zusammenhängen (z. B.
Position und Geschwindigkeit). Der Kalman-Filter rechnet ständig damit.

**Mahalanobis-Distanz**
Ein „fairer" Abstand, der die Unsicherheit berücksichtigt: Ein Plot, der in
Richtung großer Unsicherheit abweicht, zählt als „näher" als einer, der in
Richtung kleiner Unsicherheit gleich weit weg liegt. Grundlage des Gatings.

**RMSE** (*Root Mean Square Error*, Wurzel des mittleren quadratischen Fehlers)
Eine Kennzahl, wie weit die Schätzung im Schnitt von der Wahrheit abweicht.
Damit messen wir später, *ob* der Tracker gut funktioniert.

---

## Software-Begriffe (das Werkzeug Rust)

**Rust**
Die Programmiersprache, in der wir die Rechen-Engine bauen. Stark auf
Korrektheit und Geschwindigkeit ausgelegt.

**Crate**
Ein einzelnes Rust-Paket/Baustein (Bibliothek oder Programm). Firefly besteht aus
mehreren Crates, jede mit einer klaren Aufgabe.

**Workspace**
Ein Verbund mehrerer Crates, die zusammen verwaltet und gebaut werden — wie ein
Projektordner mit mehreren Teil-Modulen.

**Test (Unit-/Integrationstest)**
Kleines Prüfprogramm, das automatisch nachweist, dass ein Stück Code das
Richtige tut. Unsere Absicherung gegen Fehler.

**Clippy / `cargo fmt`**
Werkzeuge, die den Code auf typische Fehler prüfen (Clippy) bzw. einheitlich
formatieren (`fmt`).

**PRNG / Seed**
Ein *Pseudo*-Zufallsgenerator erzeugt Zufallszahlen rechnerisch. Mit demselben
Startwert („Seed") liefert er **exakt dieselbe** Folge — so werden unsere
verrauschten Szenarien reproduzierbar.

**ADR** (*Architecture Decision Record*)
Eine kurze Notiz, die eine wichtige Entscheidung samt Begründung festhält —
damit man später nachvollziehen kann, *warum* etwas so gebaut wurde.
