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
