# ADR 0006 — Integrationsziel Phoenix WebInnovation (ASD) & Ausgabe-Kontrakt

- **Status:** akzeptiert
- **Datum:** 2026-06-09

## Kontext

Firefly soll perspektivisch den **Legacy-Phoenix-Tracker** in der Plattform
**Phoenix WebInnovation** ablösen. Diese Plattform ist bereits cloud-nativ und
stellt zwei Konsumenten bereit:

- **ASD** (*Air Situation Display*) — die Lagedarstellung für den Lotsen.
- **EFS** (*Electronic Flight Strips*) — die elektronischen Flugstreifen.

Heute füttert der Legacy-Tracker das ASD. Firefly muss sich an dieses
**bestehende ASD andocken** können — also genau den Output liefern, den das ASD
erwartet, über den dort genutzten Transport.

## Entscheidung

1. **Ausgabe-Kontrakt: ASTERIX CAT062 (System-Tracks).** Das ist das Format, in
   dem das ASD die fertigen Tracks erwartet.
2. **Ports & Adapters (Hexagonale Architektur).** Der **Tracker-Kern bleibt
   format-neutral** und liefert einen klar definierten, geodätischen
   **`SystemTrack`** (ID, Position in WGS84, Geschwindigkeit, Track-Qualität,
   Status, Zeit, optional Mode 3/A, Höhe, Identität). Ein **austauschbarer
   Adapter** übersetzt diesen neutralen Output in CAT062 und auf den jeweiligen
   Transport.
3. **Offen (später klären):** der **Transport** (UDP-Multicast / Message-Bus /
   WebSocket) und der **Koordinatenbezug** des ASD (WGS84 vs. System-
   Stereografisch). Bis dahin ist die Standardausgabe **WGS84**; eine
   Projektion in eine Systemebene wäre ein reiner Adapter-Zusatz.

## Begründung

- Ein bestehender Konsument hat einen festen Eingangs-Kontrakt — den müssen wir
  bedienen, nicht neu erfinden.
- Die Trennung Kern ↔ Adapter hält den Tracker unabhängig vom Draht-Format,
  erleichtert Tests und entkoppelt die Kern-Zertifizierung vom Transport
  (passt zu ADR 0003 cloud-nativ und ADR 0004 zertifizierungs-fähig).
- CAT062 stand ohnehin in unserem ASTERIX-Plan (ADR 0001), jetzt mit klarer
  Priorität als **Ausgabe**.

## Konsequenzen

- Es entsteht ein neutraler **`SystemTrack`**-Ausgabetyp (eingeführt, wenn wir
  den Output bauen — Richtung M3).
- Der Tracker muss seine Tracks **nach WGS84 zurückprojizieren** können; dazu
  muss er die **geodätische Frame-Referenz** des Sensors mitführen (heute lebt
  der Track im lokalen ENU-Frame). Kleiner, aber wichtiger Design-Hinweis für
  die nächsten Häppchen.
- **EFS-Andockung** bedeutet später Korrelation der Tracks mit Flugplänen
  (Callsign/Identität) — fällt in die Identitäts-/Fusions-Arbeit von **M4**.
- CAT062-Kodierung + Transport-Adapter werden ein eigener, klar abgegrenzter
  Baustein (M3/M4), nicht Teil des Tracker-Kerns.

## Nachtrag (Häppchen B): Transport & Koordinatenbezug entschieden

Die in Punkt 3 offen gelassenen Fragen sind geklärt:

- **Transport: UDP-Multicast.** Der CAT062-Adapter sendet die kodierten
  Bytes als UDP-Pakete an eine Multicast-Adresse; ASD, EFS und ggf. weitere
  Konsumenten (Recorder) hören unabhängig voneinander mit. Entspricht dem in
  der Flugsicherung üblichen ASTERIX-Verteilweg (ED-109A-Umfeld) und passt zur
  Entkopplungs-Anforderung aus ADR 0003 — der Sender kennt seine Empfänger
  nicht.
- **Koordinatenbezug: System-Stereografisch.** Das ASD erwartet Positionen in
  CAT062 als **I062/100** (X/Y relativ zu einem System-Referenzpunkt), nicht
  als I062/105 (WGS84).

### Konsequenzen für den CAT062-Adapter (`firefly-asterix`)

- Der Tracker-Kern bleibt unverändert **WGS84-neutral** (`SystemTrack`,
  Punkt 2 dieses ADR) — die Projektion ist reine Adapter-Aufgabe.
- `firefly-asterix` braucht eine **Projektion WGS84 → System-Stereografisch**
  (Referenzpunkt + Projektionsparameter als Konfiguration) und einen
  **I062/100-Encoder** zusätzlich bzw. anstelle von I062/105.
- Ein **UDP-Multicast-Versand-Adapter** wird ein eigener, kleiner Baustein
  (vermutlich in `firefly-server` oder einer neuen Crate) — nicht Teil des
  Tracker-Kerns.
- Beide Punkte sind **noch nicht umgesetzt**; sie sind jetzt als Zielbild
  festgehalten und werden in eigenen Häppchen geplant (vermutlich im Umfeld
  von M4, da Multi-Sensor-Provenienz und ASD-Andockung zusammenhängen).

## Nachtrag (M3.X.4): Adapter bleiben unabhängig voneinander

Bei der Fertigstellung des CAT062-Adapters (`firefly-asterix`, Häppchen 3.X)
stellte sich die Frage, ob er eine Komfortfunktion bekommen soll, die direkt
aus dem JSON-Zwischenformat (`firefly-io::Frame`, Häppchen 3.1) übersetzt.
**Entschieden: nein.** Beide Adapter (`firefly-io` für JSON, `firefly-asterix`
für CAT062) übersetzen unabhängig voneinander **denselben** `SystemTrack` aus
dem Tracker-Kern — keiner hängt vom anderen ab. Eine `Frame → CAT062`-Brücke
hätte eine unnötige Kopplung zwischen den Adaptern eingeführt und zudem
verlustbehaftet aus `FrameTrack`s abgeleiteter Geschwindigkeit (Betrag/Kurs)
die für I062/185 nötigen kartesischen Komponenten zurückrechnen müssen.
Details siehe `docs/milestones/M3X-cat062-encoder.md`.
