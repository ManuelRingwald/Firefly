# ADR 0010 — Multi-Radar-Fusions-Architektur M4 (zentrale Mess-Fusion)

- **Status:** akzeptiert
- **Datum:** 2026-06-10

## Kontext

Bis einschließlich M3 gilt: **ein Radar → ein Tracker → ein Lagebild.** In der
realen Flugsicherung beobachten aber **mehrere** Sensoren (Primär-/Sekundärradar
und perspektivisch ADS-B) mit überlappender Reichweite **dieselben** Flugzeuge.
M4 macht aus diesen mehreren Quellen ein **einziges, konsistentes** Lagebild —
das ist der Kern jeder operativen Luftlagedarstellung.

Drei fachliche Gründe:

1. **Lückenlose Abdeckung** — eine durchgehende Spur über ein Gebiet, das größer
   ist als die Reichweite eines einzelnen Radars; kein Spurabriss beim Übergang
   von einem Radar zum nächsten.
2. **Genauigkeit & Redundanz** — mehrere unabhängige Blicke ergeben eine bessere
   Schätzung und fangen den Ausfall eines Sensors ab.
3. **Identität** — Sekundärradar/ADS-B liefern die weltweit eindeutige
   **Mode-S-Adresse** (24-Bit-ICAO) als starken „dasselbe Flugzeug"-Schlüssel
   über Sensoren hinweg (in M4 Häppchen 4.1 bereits am Track verfügbar).

**Die Gefahr ohne saubere Fusion:** Dasselbe Flugzeug erscheint als zwei (oder
mehr) Tracks — ein „Geist"/Duplikat auf dem Lotsenschirm. Das ist
sicherheitskritisch (Verwirrung, mögliche Fehlalarme der Konfliktwarnung).

### Ist-Zustand im Code (Befund vor der Entscheidung)

- Der **Simulator ist bereits multi-radar-fähig**: `Scenario::add_radar` nimmt
  mehrere Radare (je eigener Standort/`LocalFrame`, Scan-Takt, SSR-Kanal);
  `firefly_sim::run` liefert einen **gemischten, zeitlich geordneten Plot-Strom**,
  und jeder `Plot` trägt seine `SensorId`.
- Der **`Tracker` ist dagegen echt single-sensor**: `convert_plot` rechnet jeden
  Plot polar → kartesisch **relativ zu seinem Sensor** (East/North im Frame
  *dieses* Radars), ohne zu wissen, aus welchem Frame der Plot stammt; und
  `TrackerConfig` hält **ein** `SensorErrorModel`. Würde man den
  Multi-Radar-Strom heute hineingeben, lägen die Plots verschiedener Radare im
  falschen Koordinatenbezug → das Lagebild zerfiele.

Genau diese Lücke schließt M4.

## Die abgewogenen Optionen

In der Surveillance-Data-Processing-Literatur gibt es zwei klassische Muster:

**Option A — Mess-Fusion (zentrales Tracking).**
Alle Plots *aller* Sensoren fließen in **einen** Tracker, der **eine** Menge
System-Tracks hält und jeden Plot (gleich welchen Sensors) direkt zuordnet.

**Option B — Track-Fusion (track-to-track, verteiltes Tracking).**
**Jeder** Sensor bekommt seinen **eigenen** Tracker (lokale Tracks); eine
**Fusions-Schicht** darüber korreliert die lokalen Tracks (Track-to-Track-
Assoziation) und verschmilzt die desselben Flugzeugs zu System-Tracks.

## Entscheidung

**Wir wählen Option A — zentrale Mess-Fusion.** Ein einziger Tracker-Kern nimmt
den gemischten Multi-Sensor-Plot-Strom entgegen und pflegt ein gemeinsames
Lagebild. Konkret bedeutet das drei additive Bausteine:

1. **Gemeinsamer Tracking-Frame.** Der Tracker rechnet in **einem** lokalen
   ENU-Frame (System-Referenzpunkt), unabhängig von jedem einzelnen Sensor.
2. **Plot-Umrechnung in den gemeinsamen Frame.** Jeder Plot (polar, bezogen auf
   *seinen* Sensor) wird in den gemeinsamen Tracking-Frame transformiert —
   Position **und** Kovarianz (letztere gedreht um die Differenz der
   Nord-Richtungen beider Frames).
3. **Pro-Sensor-Rauschmodell.** Der Tracker führt ein `SensorErrorModel` **je**
   `SensorId`, weil Sensoren unterschiedlicher Güte unterschiedlich gewichtet
   werden müssen.

## Begründung

- **Präzision (ausschlaggebend).** A verarbeitet **rohe Messungen** direkt in
  einen gemeinsamen Zustand und ist damit theoretisch optimal. B fusioniert
  bereits gefilterte Tracks und muss dafür die **korrelierten** Fehler der
  lokalen Filter explizit mitführen (gemeinsames Prozessrauschen); unterlässt man
  das, wird das Fusionsergebnis zu optimistisch — ein bekanntes, mathematisch
  unangenehmes Problem von B. Der Projektverantwortliche hat Präzision priorisiert.
- **Keine Cloud-Nachteile (geprüft).** Der Tracker ist *heute schon*
  zustandsbehaftet (Track-Liste, per Snapshot/Replay sicherbar — ADR 0003/0007)
  und **deterministisch** (`process_scan(time, plots)` ist eine reine Funktion).
  A verbreitert nur die *Eingabe* (mehr Plots je Scan, jeder mit `SensorId`),
  bricht aber weder Determinismus noch Wiederherstellbarkeit. Das B-Argument
  „ein Tracker-Pod je Sensor" greift erst bei sehr großen Netzen (Sharding nach
  Luftraum); für unseren Maßstab (wenige überlappende Radare) ist **ein**
  Tracker-Dienst, gefüttert über den Datenstrom, der einfachere und gleich
  cloud-taugliche Schnitt.
- **Synergie mit ADR 0006.** Der „gemeinsame Tracking-Frame" ist zugleich der
  **System-Referenzpunkt**, den die System-Stereografische CAT062-Ausgabe
  (ADR 0006-Nachtrag) ohnehin braucht. Beide Themen laufen hier zusammen, statt
  zweimal gelöst zu werden.
- **Additiv, nicht invasiv für die Adapter (ADR 0006).** Der neutrale
  `SystemTrack` und die Ausgabe-Adapter (JSON, CAT062) bleiben unverändert; A
  ändert nur den *Eingang* und das *innere* Rechnen des Kerns.

Der ehrliche Gegenpunkt für B (Modularität, jede Stufe einzeln testbar, mehr
Lern-Schritte) wurde bewusst niedriger gewichtet, weil der Lernfaktor in M4 eine
untergeordnete Rolle spielt und A die bessere Genauigkeit liefert.

## Abgrenzung (was hier *nicht* entschieden wird)

- **Sensor-Registrierung / Bias-Korrektur** (systematische Versätze einzelner
  Radare in Entfernung/Azimut/Zeit) ist ein eigenes, späteres Thema — für A
  *und* B gleichermaßen relevant, kein A-spezifischer Nachteil. Eigener Häppchen-
  bzw. ADR-Schritt, sobald die Fusion steht.
- **ADS-B als eigene Quelle** (CAT021) wird hier nur als Richtung genannt; die
  konkrete Einspeisung folgt als eigenes Häppchen.
- **Asynchrone Scan-Zeiten / Out-of-order** am Eingang: Standard bleibt „nach
  Datenzeit ordnen, zu Spätes verwerfen" (STATUS, offener Punkt) — Detail des
  Umsetzungs-Häppchens, kein Architektur-Bruch.
- **Mode-3/A & Mode-S in der Assoziation als Schlüssel:** in A vereinfacht sich
  die Identitäts-Korrelation, weil es ohnehin nur *einen* Track je Flugzeug gibt;
  die Identität wird am Track gepflegt (Häppchen 4.1) und in CAT062 kodiert
  (Häppchen 4.2). Eine eigene Track-to-Track-Korrelation entfällt mit A.

## Konsequenzen

- **`firefly-track`** wird auf Multi-Sensor erweitert: gemeinsamer Tracking-Frame,
  Plot-Umrechnung vor der Assoziation, Pro-Sensor-Rauschmodell. Der bestehende
  Single-Sensor-Pfad bleibt als Sonderfall (ein Sensor) gültig.
- **`firefly-geo`** bekommt einen neuen, isoliert testbaren Baustein:
  Transformation von Position **und** Kovarianz von einem `LocalFrame` in einen
  anderen (Verkettung Sensor-ENU → WGS84 → Tracking-ENU samt Kovarianz-Rotation).
- **Keine neue Crate nötig** (anders als bei B, das eine `firefly-fusion`-Schicht
  gebraucht hätte) — A bleibt im bestehenden Kern.
- M4 wird in Häppchen geschnitten: **4.A.1** Geo-Frame-zu-Frame-Transformation →
  **4.A.2** Tracker auf Multi-Sensor → **4.A.3** Multi-Radar-Szenario + E2E-Test
  (ein Flugzeug, zwei Radare, **ein** Track) → **4.A.4** Sensor-Provenienz im
  `SystemTrack`. **4.2** (CAT062-Identitätsfelder) läuft unabhängig daneben.
