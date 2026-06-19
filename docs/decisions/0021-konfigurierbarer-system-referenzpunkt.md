# ADR 0021 — Konfigurierbarer System-Referenzpunkt (Single Source of Truth)

- **Status:** akzeptiert
- **Datum:** 2026-06-19
- **Folgeentscheidung zu:** ADR 0006 (CAT062-Ausgabe-Kontrakt), ADR 0010
  (zentrale Mess-Fusion), ADR 0020 (Live-Tracker-Modus)

## Kontext

CAT062 trägt die Track-Position doppelt:

- **I062/105** = WGS84 (absolut, kontextfrei) — die *primäre* Position, aus der
  das ASD (Wayfinder) rendert.
- **I062/100** = System-Stereografisch X/Y in Metern, **relativ zu einem
  Referenzpunkt** (dem Tangentenpunkt der Projektion).

Bis zu dieser Entscheidung existierten im Code **zwei voneinander unabhängige**
Referenzpunkte:

1. **Tracking-Frame-Ursprung** — der ENU-Ursprung, in dem der Tracker rechnet
   (ADR 0010). Szenenabhängig hartkodiert (`DEMO_ORIGIN` = 48/11,
   `FRANKFURT_ORIGIN` = 50,04/8,56) bzw. im Live-Modus die Mitte der
   OpenSky-Bounding-Box.
2. **I062/100-Projektionsreferenz** — gesetzt über
   `FIREFLY_CAT062_REF_LAT/_LON` mit Default **48/11**, getragen als Feld
   `MulticastConfig::reference_point`.

Diese beiden stimmten nur im Demo-Fall (zufällig beide 48/11) überein. Bei
Frankfurt und im Live-Modus liefen sie auseinander: Der Tracker rechnete um
Frankfurt bzw. die Bbox-Mitte, während I062/100 X/Y relativ zu 48/11 kodierte.
Ein Konsument, der I062/100 ernst nimmt, bekäme damit einen kartesischen
Nullpunkt, der nichts mit dem operativen Referenzpunkt des Systems zu tun hat.
Die ICD vermerkte dies als offenen Punkt („bis dahin ist I062/100 nur im
Demo-Kontext sinnvoll interpretierbar").

## Entscheidung

Es gibt genau **einen System-Referenzpunkt** je laufender Instanz. Er ist die
**Single Source of Truth** und speist **beides**: den Tracking-Frame-Ursprung
*und* die I062/100-Projektionsreferenz. Der Referenzpunkt ist eine Eigenschaft
des **Systems**, nicht des Multicast-Adapters.

Auflösung des Referenzpunkts nach Betriebsmodus:

- **Replay (Demo/Frankfurt):** = der **Szenen-Ursprung**
  (`scene_reference_point`). Die Szene definiert Sensoren und Ziele relativ zu
  diesem Ursprung; I062/100 wird damit automatisch kohärent mit dem
  Tracking-Frame. Kein separater Override (eine unabhängige Referenz würde
  genau die Inkohärenz wieder einführen, die diese Entscheidung beseitigt).
- **Live (ADS-B):** konfigurierbar über `FIREFLY_SYSTEM_REF_LAT/_LON` (Grad).
  Ohne Override = **Mitte der OpenSky-Bounding-Box** (bisheriges Verhalten des
  Tracking-Frames, ADR 0020). Derselbe Punkt speist Tracking-Frame und
  I062/100-Encoder.

Das Feld `MulticastConfig::reference_point` und die Variablen
`FIREFLY_CAT062_REF_LAT/_LON` **entfallen** — der Referenzpunkt wird nicht mehr
beim Multicast-Adapter angesiedelt, sondern beim System.

## Konsequenzen

### Positiv

- **Kohärenz per Konstruktion:** I062/100 misst immer ab demselben Ursprung, in
  dem der Tracker rechnet. Die ICD-Einschränkung „nur im Demo-Kontext sinnvoll"
  entfällt.
- **Aus dem Demo-Kontext gelöst:** Der Live-Betrieb lässt sich auf einen
  beliebigen operativen Referenzpunkt setzen (z. B. ein Standort jenseits von
  Frankfurt).
- **Einfacheres mentales Modell:** ein Begriff statt zwei.

### Neutral / Grenzen

- **Keine sichtbare Wirkung auf das ASD-Bild.** Wayfinder rendert aus I062/105
  (WGS84, absolut). Der Referenzpunkt betrifft ausschließlich die optionale
  I062/100-Systemebene. Dies ist eine **Korrektheits-/Schnittstellen-
  Verbesserung**, kein Anzeige-Feature.
- **Numerische Konditionierung:** Der Tracking-Frame-Ursprung beeinflusst nur
  die interne Rechengenauigkeit (am besten nahe dem Ursprung). Für regionalen
  Luftraum ist der Effekt vernachlässigbar; die Ausgabe bleibt absolut (WGS84).

### Schnittstellen-Wirkung (Config)

- **Entfernt:** `FIREFLY_CAT062_REF_LAT`, `FIREFLY_CAT062_REF_LON`.
- **Neu:** `FIREFLY_SYSTEM_REF_LAT`, `FIREFLY_SYSTEM_REF_LON` (nur Live-Modus
  wirksam; Replay nutzt den Szenen-Ursprung).
- Der CAT062-**Draht**-Vertrag (FSPEC/UAP, Item-Kodierung) ändert sich **nicht**
  — I062/100 wird weiterhin als 24-Bit-Zweierkomplement, LSB 0,5 m kodiert.
  Damit ist **keine** ICD-Versionserhöhung nötig; der Changelog vermerkt nur
  die präzisierte Referenzpunkt-Semantik. Wayfinder ist nicht betroffen
  (nutzt I062/105).

## Alternativen

- **`FIREFLY_CAT062_REF_*` als unabhängigen Override behalten.** Verworfen: Das
  hält die Möglichkeit zur Inkohärenz offen und widerspricht dem Ziel „eine
  Wahrheit".
- **Referenzpunkt auch im Replay konfigurierbar machen.** Verworfen: Im Replay
  ist der Ursprung durch die Szene (Sensor-/Ziel-Geometrie) festgelegt; ein
  abweichender I062/100-Referenzpunkt wäre per Definition inkohärent.
