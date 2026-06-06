# M1 — Der Simulator: unsere Datenquelle

> Verständliche Erklärung des ersten Meilensteins. Begriffe, die hier zum ersten
> Mal fallen, stehen ausführlicher im [Glossar](../glossary.md).

## 1. Warum überhaupt ein Simulator?

Ein Tracker verarbeitet **Plots** (einzelne Radar-Detektionen) und macht daraus
**Tracks** (durchgehende Flugspuren). Um den Tracker später zu bauen *und zu
prüfen*, brauchen wir erst einmal Plots zum Füttern.

Echte Radardaten sind schwer zu bekommen und — viel wichtiger zum Lernen und
Testen — wir kennen bei ihnen die **Wahrheit** nicht. Beim Simulator dagegen
wissen wir *exakt*, wo jedes Flugzeug wirklich war. Dadurch können wir später
schwarz auf weiß messen: *Wie nah kommt der berechnete Track an die Wahrheit?*

Der Simulator ist also unser Prüfstand mit bekannter Wahrheit
(„Ground Truth" = die gesicherte Realität, gegen die wir vergleichen).

## 2. Die drei Bausteine (Crates) von M1

Wir haben den Code in drei **Crates** geteilt — eigenständige Bausteine mit je
einer klaren Aufgabe:

| Crate | Aufgabe | Analogie |
|-------|---------|----------|
| `firefly-geo` | Koordinaten korrekt umrechnen | Der „Vermesser" |
| `firefly-core` | Gemeinsame Vokabeln (Plot, Sensor, Zeit, IDs) | Das „Wörterbuch" |
| `firefly-sim` | Flugzeuge fliegen lassen & Radare beobachten lassen | Die „Bühne mit Regie" |

Warum aufteilen? Damit jeder Teil für sich verständlich, testbar und später
austauschbar bleibt. Den „Vermesser" (`geo`) wird später auch der Tracker
nutzen, nicht nur der Simulator.

---

## 3. `firefly-geo` — der Vermesser

### Das fachliche Problem

Ein Radar misst **polar**: „Das Ziel ist 42 km weit weg, in Richtung 284°."
Rechnen (Geschwindigkeiten, gerade Flugbahnen) will man aber lieber **kartesisch**
auf einem flachen Gitter (X nach Osten, Y nach Norden). Und die Welt selbst
beschreibt man in **Längengrad / Breitengrad / Höhe** (WGS84) — auf einer
gekrümmten Erde. Drei Welten, die zusammenpassen müssen:

```
WGS84  <->  ECEF  <->  ENU (lokal flach)  <->  Polar (so misst das Radar)
(Globus)   (Erd-    (Stadtplan ums         (Entfernung + Winkel)
            mittel-  Radar herum)
            punkt)
```

### Wie wir es lösen (in Worten, ohne Formeln)

- **WGS84 → ECEF:** Wir rechnen Längengrad/Breitengrad/Höhe in einen Punkt im
  erdfesten X/Y/Z-System um (Ursprung im Erdmittelpunkt). Das ist die
  Standard-Formel der Geodäsie und auch die Basis von GPS.
- **ECEF → WGS84 (zurück):** etwas kniffliger, weil die Erde abgeplattet ist. Wir
  nutzen ein bewährtes geschlossenes Verfahren (nach *Bowring*), das ohne
  Wiederholungsschleifen auf Bruchteile eines Millimeters genau ist.
- **ECEF → ENU:** Wir „setzen einen flachen Stadtplan" auf den Radarstandort.
  In diesem lokalen System ist Osten = +X, Norden = +Y, oben = +Z. Hier sieht
  geradeaus-Fliegen auch wie eine gerade Linie aus — ideal fürs spätere Tracking.
- **ENU → Polar:** Aus den lokalen X/Y/Z-Werten ergeben sich direkt Entfernung
  (Satz des Pythagoras), Azimut (Himmelsrichtung) und Elevation (Höhenwinkel).

### Woher wir wissen, dass es stimmt

Wir haben **Roundtrip-Tests**: Man rechnet einen Punkt hin und wieder zurück —
kommt (fast) exakt derselbe Punkt heraus, ist die Mathematik konsistent.
Zusätzlich prüfen wir bekannte Referenzpunkte (z. B. liegt der Punkt am Äquator
auf dem Nullmeridian genau auf der X-Achse in Höhe des Erdradius). Diese Tests
laufen automatisch bei jedem Build.

---

## 4. `firefly-core` — das Wörterbuch

Hier wohnen die Begriffe, die **alle** Bausteine gemeinsam verwenden, damit
Simulator und Tracker dieselbe Sprache sprechen:

- **`Plot`** — eine einzelne Detektion: welches Radar, welche Zeit, die polare
  Messung, die Art (PSR / SSR / beides kombiniert) und die SSR-Zusatzdaten.
- **`Sensor`** — ein Radarstandort mit Position und „seinem" lokalen ENU-System.
- **`ModeAC`** — die Sekundärradar-Daten an einem Plot: Squawk, Flugfläche,
  ICAO-Adresse (jeweils „vorhanden oder nicht").
- **Typisierte IDs** (`SensorId`, `TrackId`, `TargetId`) — bewusst *getrennte*
  Typen, damit man eine Sensor-Nummer nie versehentlich mit einer Track-Nummer
  verwechseln kann. (So fängt der Compiler einen ganzen Fehlertyp ab, bevor er
  passiert.)
- **`Timestamp`** — ein Zeitpunkt in Sekunden.

### Eine fachliche Feinheit, die schon hier sichtbar wird

Der `DetectionKind` unterscheidet **Primary**, **Secondary** und **Combined**.
Das spiegelt die Realität: Ein Primärradar sieht ein Echo *ohne* Identität/Höhe;
ein Sekundärradar bekommt eine Transponder-Antwort *mit* Identität/Höhe; oft
fallen beide im selben Antennenblick zusammen („kombiniert"). Genau dieser
Unterschied macht später das Tracking anspruchsvoll: Primär-Ziele (z. B. ein
Flugzeug ohne aktiven Transponder) sind der schwierige Fall.

---

## 5. `firefly-sim` — die Bühne mit Regie

Hier passiert das Eigentliche: Flugzeuge fliegen, Radare schauen zu, Plots
entstehen.

### 5.1 Wie ein Flugzeug fliegt — „Legs"

Ein Ziel (`Target`) hat einen Startzustand (Ort, Geschwindigkeit, Steuerkurs,
Steig-/Sinkrate) und eine Folge von **Legs** („Beine" einer Route). Jedes Leg
hält für eine bestimmte Dauer drei Stellgrößen konstant:

- **Drehrate** (Kurve nach links/rechts),
- **Längsbeschleunigung** (schneller/langsamer),
- **Steig-/Sinkrate**.

Daraus lassen sich alle Grundmanöver bauen: `cruise` (gerade & konstant),
`turn` (Kurve), `accelerate`, `climb`. Der Simulator rechnet die Flugbahn in
kleinen Zeitschritten aus (Standard: alle 0,1 s) — fein genug, dass auch Kurven
sauber aussehen.

Warum so einfach gehalten? Wir brauchen *keine* echte Flugdynamik, sondern nur
glaubwürdige Bewegungsmuster, an denen sich der Tracker bewähren muss. Mehr wäre
für M1 verschwendete Mühe.

### 5.2 Wie ein Radar „sieht" — das Sensormodell

Jedes Radar (`Radar`) hat ein Fehler- und Geometriemodell (`RadarParams`):

- **Scan-Periode** — wie oft sich die Antenne dreht (z. B. alle 4 s ein Blick).
- **Erfassungswahrscheinlichkeit (Pd)** — die Chance, ein Ziel pro Blick
  wirklich zu sehen (z. B. 90 %). Mit der Restwahrscheinlichkeit *fehlt* der
  Plot — genau wie in echt.
- **Messrauschen** für Entfernung, Azimut und Elevation — als
  Standardabweichung (Sigma). Real misst kein Radar exakt.
- **Reichweite & tiefster Strahl** — außerhalb sieht das Radar nichts.
- **SSR ja/nein** — ob es einen Sekundärkanal (Identität/Höhe) hat.

Für jeden Antennenblick und jedes Ziel macht das Radar:
1. Wahre Position des Ziels → ins eigene Polarsystem umrechnen (über `geo`).
2. Außer Reichweite oder unter dem Horizont? → kein Plot.
3. „Würfeln" gegen die Erfassungswahrscheinlichkeit → mit etwas Pech kein Plot.
4. Sonst: Messrauschen aufschlagen und einen `Plot` ausgeben — bei
   SSR-Ausrüstung samt Squawk, Flugfläche und ICAO-Adresse.

### 5.3 Eine bewusste, wichtige Designentscheidung: Rauschen *im Polarsystem*

Das Rauschen schlagen wir auf **Entfernung und Winkel** auf — nicht auf X/Y.
Warum? Weil ein Radar physikalisch genau so danebenliegt: Die Entfernung ist
recht präzise, der *Winkel* ist relativ ungenau. In großer Entfernung wird aus
einem kleinen Winkelfehler ein großer seitlicher Versatz. Diese „zigarrenförmige"
Unsicherheit (längs der Sichtlinie schmal, quer dazu breit) ist eine zentrale
Eigenschaft von Radardaten — und der Tracker muss sie später korrekt behandeln.
Würden wir gleich in X/Y rauschen, hätten wir diese Realität wegmodelliert.

### 5.4 Der Ablauf — ein zeitlich geordneter Plot-Strom

Der Runner (`run`) lässt alle Radare über die ganze Szenariodauer „scannen" und
sammelt alle entstandenen Plots ein, **sortiert nach Zeit**. Genau das ist die
Form, in der auch ein echter Tracker seine Daten bekommt: ein fortlaufender Strom
von Detektionen, Scan für Scan.

### 5.5 Reproduzierbarkeit — der eigene Zufallsgenerator

Zufall (verpasste Plots, Rauschen) erzeugen wir mit einem eigenen, einfachen
**Pseudo-Zufallsgenerator** (PCG32) plus „Seed". Derselbe Seed liefert immer
*exakt dieselbe* Folge von Zufallszahlen — und damit dasselbe Szenario, auf jedem
Rechner. Das ist Gold wert: Wenn der Tracker später einen Fehler zeigt, können
wir die Situation **identisch** wieder herstellen und untersuchen. Außerdem
bekommt jedes Radar seinen eigenen Zufalls-„Kanal", damit das Hinzufügen eines
zweiten Radars die Plots des ersten nicht verändert.

---

## 6. Selbst ausprobieren

```bash
cargo test --workspace                     # alle Tests (24 + 1 Doctest)
cargo run --example demo -p firefly-sim    # ein Beispiel-Szenario ausgeben
```

Das Demo baut ein Radar nahe München und zwei Flugzeuge: eines mit Transponder
(erscheint als „PSR+SSR" mit Flugfläche), eines ohne (nur „PSR", die schwierigere
Sorte). Man sieht, wie Entfernung und Azimut sich über die Antennenblicke
plausibel entwickeln.

---

## 7. Was M1 bewusst noch NICHT kann (→ Brücke zu M2 und später)

- **Keine Falschalarme/Clutter:** Bisher stammt jeder Plot von einem echten Ziel.
  Echte Radare liefern auch „Geister" (Wetter, Vögel). Bauen wir später ein.
- **Kein Tracking:** M1 erzeugt nur die Rohdaten. Das Zusammensetzen zu Tracks —
  Gating, Datenassoziation, Kalman-Filter, Track-Lebenszyklus — ist die Aufgabe
  von **M2**.
- **Höhe vereinfacht:** Wir verwenden die geometrische Höhe als Platzhalter für
  die barometrische (Mode-C-)Höhe; ein echtes Luftdruckmodell kommt bei Bedarf
  später.

Damit ist die Datenquelle fertig — und der Boden bereitet, auf dem in M2 der
eigentliche Tracker entsteht.
