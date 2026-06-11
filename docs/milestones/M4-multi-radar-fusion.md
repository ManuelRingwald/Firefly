# M4 — Mehrere Radare, ein Lagebild: Mess-Fusion und SSR-Identität

> Verständliche Erklärung des vierten Meilensteins. Begriffe stehen
> ausführlicher im [Glossar](../glossary.md).

Bis M3 galt: **ein Radar → ein Tracker → ein Lagebild.** In der echten
Flugsicherung beobachten aber mehrere Sensoren mit überlappender Reichweite
*dieselben* Flugzeuge — und liefern zusätzlich eine **Identität**
(Mode-3/A-Code, Mode-S-ICAO-Adresse) über das Sekundärradar (SSR). M4 macht aus
mehreren Quellen **ein** konsistentes Lagebild, in dem jedes Flugzeug genau
**einen** Track behält und seine Identität trägt.

Die Gefahr ohne saubere Fusion: Dasselbe Flugzeug erscheint als zwei (oder
mehr) Tracks — ein „Geist"/Duplikat auf dem Lotsenschirm. Sicherheitskritisch,
weil es zu Verwirrung und Fehlalarmen der Konfliktwarnung führen kann.

---

## Häppchen 4.1 — SSR-Identität bis zum `SystemTrack`

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-009`

### Das Problem (fachlich)

Ein Primärradar (PSR) sieht nur „da ist etwas" — Position, sonst nichts. Das
Sekundärradar (SSR) befragt das Flugzeug aktiv und bekommt eine Antwort
zurück: den **Mode-3/A-Code** (vom Lotsen zugewiesener 4-stelliger Oktalcode,
umgangssprachlich „Squawk") und bei Mode-S-fähigen Flugzeugen die weltweit
**eindeutige 24-Bit-ICAO-Adresse**. Diese Identität soll am Track "kleben
bleiben" — auch wenn ein einzelner Scan mal nur eine Primär-Antwort liefert.

### Die Lösung (technisch)

`Track::update_identity(&mode_ac)` übernimmt `mode_3a` und `icao_address` aus
einem zugeordneten Plot — **sticky**: ein `None` (reiner Primär-Treffer)
löscht eine bekannte Identität *nicht*, eine neue Antwort überschreibt sie.
`SystemTrack` (firefly-core) führt beide Felder als `Option<u16>` /
`Option<u32>` mit und gibt sie an die Ausgabe-Adapter weiter.

---

## Häppchen 4.0 — Architektur-Entscheidung: zentrale Mess-Fusion (ADR 0010)

**Status:** ✅ umgesetzt

### Die Frage

Wie sollen mehrere Radare zu *einem* Lagebild verschmolzen werden? Zwei
klassische Muster standen zur Wahl:

- **Option A — Mess-Fusion (zentrales Tracking):** Alle Plots aller Sensoren
  laufen in **einen** Tracker, der **ein** gemeinsames Lagebild pflegt.
- **Option B — Track-Fusion (track-to-track):** *Jeder* Sensor bekommt seinen
  **eigenen** Tracker (lokale Tracks); eine Fusionsschicht darüber korreliert
  und verschmilzt die lokalen Tracks zu System-Tracks.

### Die Entscheidung (und warum)

**Option A — zentrale Mess-Fusion** (ADR 0010). Drei additive Bausteine:

1. **Gemeinsamer Tracking-Frame** — der Tracker rechnet in *einem* lokalen
   ENU-Bezugsrahmen (System-Referenzpunkt), unabhängig von jedem Sensorstandort.
2. **Plot-Umrechnung in diesen Frame** — jeder Plot (polar, bezogen auf seinen
   Sensor) wird vor Gating/Assoziation in den gemeinsamen Frame transformiert,
   Position **und** Kovarianz.
3. **Pro-Sensor-Rauschmodell** — ein `SensorErrorModel` je `SensorId`, weil
   unterschiedlich gute Sensoren unterschiedlich gewichtet werden.

Ausschlaggebend war **Präzision**: A verarbeitet Rohmessungen direkt in einen
gemeinsamen Zustand und ist damit theoretisch optimal; B müsste die
*korrelierten* Fehler bereits gefilterter lokaler Tracks explizit mitführen,
sonst wird die Fusion zu optimistisch. Geprüft wurde außerdem, dass A **keine**
Cloud-Nachteile bringt: Der Tracker bleibt zustandsbehaftet
(Snapshot/Replay, ADR 0003/0007) und deterministisch — A verbreitert nur die
*Eingabe* (mehr Plots je Scan, je mit `SensorId`). Zusätzliche Synergie: der
gemeinsame Tracking-Frame ist zugleich der System-Referenzpunkt, den die
System-Stereografische CAT062-Ausgabe (ADR 0006-Nachtrag) ohnehin braucht.

---

## Häppchen 4.A.1 — Frame-zu-Frame-Transformation (Position + Kovarianz)

**Status:** ✅ umgesetzt · Anforderung `FR-GEO-003`

### Das Problem (fachlich)

Ein Plot von Radar B ist zunächst eine Position **relativ zu Radar B** — mit
einer Unsicherheits-Ellipse, deren Achsen entlang *Radar Bs* Blickrichtung
liegen (Entfernung sehr genau, Querrichtung weniger). Um ihn mit Radar As
Tracks zu vergleichen, müssen sowohl die **Position** als auch diese
**Ellipse** in den gemeinsamen Tracking-Frame umgerechnet werden — die Ellipse
muss sich dabei mitdrehen.

### Die Lösung (technisch)

`firefly-geo::LocalFrame` bekommt zwei neue Bausteine:

- **`horizontal_rotation_from`** — die 2×2-Drehmatrix zwischen den
  Nord-Richtungen zweier `LocalFrame`s (geodätisch verkettet: Quell-ENU →
  WGS84 → Ziel-ENU).
- **`horizontal_from(source, z, r)`** — transformiert eine Messung
  `z` (Position) und ihre Kovarianz `r` (2×2-Matrix) vom `source`-Frame in
  `self`. Die Position läuft über die Geodäsie (ENU → WGS84 → ENU), die
  Kovarianz wird mit `R' = T·R·Tᵀ` gedreht (`T` = Drehmatrix).

Fünf Tests in `frame_transform.rs` decken Identität (gleicher Frame),
Positions-Treffer im Zielframe, Hin-und-Rückweg-Konsistenz, Erhalt der
Ellipsen-*Form* (Eigenwerte) und die tatsächliche *Drehung* der Ellipse bei
konvergierenden Frames ab.

---

## Häppchen 4.A.2 — `firefly-track` auf Multi-Sensor

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-010`

### Das Problem (fachlich/technisch)

`TrackerConfig` kannte bisher genau **ein** `SensorErrorModel` und der
Tracker rechnete implizit im Frame dieses einen Sensors. Für mehrere Sensoren
braucht es: einen gemeinsamen Frame, eine Zuordnung Sensor → (Frame,
Rauschmodell), und — die eigentliche Knacknuss — eine Verarbeitungsreihenfolge,
die **nicht** zu Geistern führt.

### Die Lösung (technisch)

- `TrackerConfig` trägt jetzt `tracking_frame: LocalFrame` und
  `sensors: BTreeMap<SensorId, SensorModel>` (`SensorModel` = Frame +
  Rauschmodell). `with_sensor(...)` registriert einen Sensor,
  `single_sensor(...)` ist die Ein-Sensor-Abkürzung (reproduziert das
  Vor-M4-Verhalten).
- **Das Geister-Problem:** Verarbeitet man die Plots *aller* Sensoren in
  **einem** gemeinsamen Assoziationsschritt, kann ein Track in einer 1:1-
  Zuordnung nur **einem** Plot zugeordnet werden — sehen zwei Radare dasselbe
  Flugzeug, "gewinnt" eines, das andere spawnt einen zweiten Track für
  dasselbe Flugzeug.
- **Die Lösung:** Sensoren werden **sequenziell** verarbeitet (deterministisch
  nach `SensorId`-Reihenfolge, dank `BTreeMap`). Jeder Sensor assoziiert seine
  Plots gegen die *aktuelle* Track-Liste — inklusive der Tracks, die ein
  früherer Sensor in *diesem selben Scan* schon aktualisiert oder neu
  gegründet hat. Sensor Bs Plot trifft also auf den Track, den Sensor A gerade
  schon bearbeitet hat, statt einen Geist zu gründen. Treffer/Fehltreffer wird
  **einmal pro Scan** gebucht (`BTreeSet<TrackId>`), sodass der
  M-aus-N-Lebenszyklus für den Ein-Sensor-Fall unverändert bleibt.
- Plots von einem nicht registrierten Sensor werden verworfen (sie können
  nicht geolokalisiert werden).

---

## Häppchen 4.A.3 — Multi-Radar-Szenario + Ende-zu-Ende-Test

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-010`

### Der Beweis

`firefly-player/tests/multi_radar.rs` führt die ganze Kette aus: Simulator →
Multi-Sensor-Tracker → Frame-Strom. Zwei rauschfreie Radare, ~45 km
auseinander, beobachten **ein** Flugzeug, das durch ihren Überlappungsbereich
fliegt (je 51 Plots). Das Ergebnis:

- über den **gesamten Lauf** genau **eine** Track-ID,
- in **keinem** Frame mehr als ein Track gleichzeitig (kein Geist),
- der fusionierte Track ist über den größten Teil des Laufs **bestätigt**.

Damit ist der zentrale Architektur-Entscheid (ADR 0010) Ende-zu-Ende
nachgewiesen — nicht nur in Tracker-internen Unit-Tests.

---

## Häppchen 4.A.4 — Sensor-Provenienz im `SystemTrack`

**Status:** ✅ umgesetzt · Anforderung `FR-TRK-010`

### Das Problem (fachlich)

Bei der Mess-Fusion sieht ein Track oft **mehrere** Sensoren gleichzeitig.
„Wie alt ist die letzte Messung?" (`update_age`, ADR 0008) beantwortet das
nicht — sie sagt nichts darüber aus, *welcher* Sensor gerade beiträgt. Für
CAT062 I062/290 (Update-Alter je Sensor-Technologie) war das bisher eine
**Single-Sensor-Vereinfachung**.

### Die Lösung (technisch)

`Track` merkt sich pro Scan, welche Sensoren ihn getroffen haben
(`contributing_sensors: BTreeSet<SensorId>`): zu Beginn jedes
`process_scan` wird die Menge geleert, dann trägt jeder Sensor, dessen Plot
diesen Track aktualisiert oder gegründet hat, sich ein
(`record_hit_from`). `SystemTrack` gibt sie als sortierten
`Vec<SensorId>` aus. Anders als die SSR-Identität ist das **nicht sticky** —
beim Coasten (kein Sensor hat getroffen) ist die Liste leer. Das beantwortet
„wer sieht diesen Track *gerade jetzt*?".

---

## Häppchen 4.2 — CAT062-Identitätsfelder

**Status:** ✅ umgesetzt · Anforderung `FR-IO-003`, `FR-TRK-009`

### Das Problem (fachlich)

Die in 4.1 am Track verfügbare SSR-Identität muss noch den Weg ins
ASTERIX-CAT062-Format finden, damit das ASD sie anzeigen kann.

### Die Lösung (technisch)

`Cat062Encoder::record` fügt **zusätzlich** zwei Items ein, *wenn* der Track
eine Identität trägt:

- **I062/060** (Mode 3/A Code): zwei Oktette, der 12-Bit-Oktalcode in den
  unteren Bits, Validierungs-Flags (V/G/CH) auf 0 — der Tracker meldet einen
  bereits bestätigten Code.
- **I062/380 / ADR-Subfeld** (Aircraft Derived Data → Target Address): die
  24-Bit-ICAO-Adresse, mit gesetztem ADR-Präsenz-Bit (`0x80`).

Beide Items entfallen bei einem reinen Primär-Track — der `RecordBuilder`
(FSPEC/UAP-Mechanik aus 3.X.1) sorgt automatisch dafür, dass die zugehörigen
FSPEC-Bits (FRN 9 bzw. 11) dann *nicht* gesetzt sind. LSB-Werte und
Subfeld-Layout wurden gegen SUR.ET1.ST05.2000-STD-09-01 (Ed. 1.10) verifiziert.

---

## M4 — Fazit

M4 macht aus dem Single-Radar-Tracker (M2/M3) einen **Multi-Radar-Tracker**:

- Ein gemeinsamer Tracking-Frame und eine Frame-zu-Frame-Transformation
  (Position + Kovarianz, `firefly-geo`) heben jeden Plot in diesen Frame.
- Der Tracker verarbeitet mehrere Sensoren **sequenziell** je Scan — das löst
  das Geister-Problem ohne separate Track-Fusionsschicht.
- Ein Ende-zu-Ende-Test (zwei Radare, ein Flugzeug → ein Track) belegt das.
- Jeder Track trägt seine SSR-Identität (sticky) und seine aktuelle
  Sensor-Provenienz (nicht sticky) — beide erreichen über `SystemTrack` auch
  den CAT062-Adapter (I062/060, I062/380/ADR).

**Offen (später, ADR 0010-Abgrenzung):**

- **Sensor-Registrierung / Bias-Korrektur** — systematische Versätze einzelner
  Radare (Entfernung/Azimut/Zeit) werden noch nicht herausgerechnet. Eigenes,
  späteres Thema (S5).
- **ADS-B als eigene Quelle** (CAT021) — bisher nur als Richtung benannt.
- **Transport & Koordinatenbezug der CAT062-Ausgabe** (UDP-Multicast,
  System-Stereografische Projektion / I062/100) — in ADR 0006-Nachtrag als
  Zielbild festgehalten, noch nicht umgesetzt.
