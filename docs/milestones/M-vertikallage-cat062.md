# Vertikallage im CAT062-Strom (I062/136) + UAP-Standardtreue

> Feature-Doku zu ADR 0015. Begriffe stehen ausführlicher im
> [Glossar](../glossary.md). Schnittstellen-Vertrag: [ICD-CAT062](../ICD-CAT062.md)
> (ab v2.0.0).

**Status:** ✅ umgesetzt · Anforderungen `FR-TRK-027`, `FR-IO-003`, `FR-IO-004`

---

## Das Problem (fachlich)

Der CAT062-Strom war bisher rein **zweidimensional** — Position und
Horizontalgeschwindigkeit, aber keine **Flugfläche (FL)**. Flugverkehrskontrolle
ist jedoch inhärent 3D: ein Air Situation Display zeigt zu jedem Track sein
Flugniveau. Ohne Vertikallage ist das Lagebild fachlich unvollständig — es war
der größte Realitäts-Abstand zu einem echten ARTAS-/SDPS-Feed.

Bei der Umsetzung fiel ein **vorbestehender Standard-Verstoß** auf: I062/500
(Estimated Accuracies) lag auf FRN 16. In der echten EUROCONTROL-CAT062-UAP ist
FRN 16 aber I062/295; I062/500 gehört auf FRN 27. Ein konformer Fremd-Decoder
hätte unser I062/500 als I062/295 fehlinterpretiert.

## Die Lösung (technisch)

### 1. Vertikallage als Pass-through (FR-TRK-027)

Der Tracker-Kern führt **keinen** vertikalen Kalman-Zustand. Stattdessen wird
die zuletzt gemessene Mode-C-Höhe wie die Identität „sticky" durchgereicht:

- `firefly-sim` liefert pro SSR-Plot bereits `ModeAC.flight_level_ft` (Mode-C-
  Höhe in Fuß).
- `Track::update_identity` übernimmt sie (present überschreibt, `None` lässt den
  letzten Wert stehen — ein Primär-only-Treffer löscht nichts).
- `SystemTrack` bekommt das neue Feld `flight_level_ft: Option<f64>`;
  `system_track_from` reicht es durch.

Eine *gemessene*, keine geschätzte Größe — ehrlich und einfach. Ein echter
vertikaler Schätzer (sowie I062/130/135 Berechnungs-Höhen, I062/220 Steig-/
Sinkrate) bleibt bewusst offen.

### 2. I062/136 im Encoder/Decoder (FR-IO-003/004)

- **Encoder:** neues optionales Item auf **FRN 17**, kodiert als signed i16,
  big-endian, LSB = 1/4 FL = 25 ft (`round(ft / 25)`). Wird nur emittiert, wenn
  der Track eine Flugfläche trägt; das FSPEC spiegelt das automatisch.
- **Decoder:** Gegenstück `decode_measured_flight_level` (i16 · 25 ft → Fuß),
  neues optionales `DecodedRecord.flight_level_ft`.

### 3. UAP auf volle Standardtreue (ADR 0015)

I062/500 wandert von FRN 16 auf den echten **FRN 27**. Die Firefly-UAP ist damit
ein **konformes Subset** der EUROCONTROL-CAT062-UAP: alle emittierten Items
sitzen auf ihren echten FRNs, die Lücken sind die nicht emittierten
Standard-Items (FRN 2 Spare, 3, 8, 10, 15, 16, 18–20 …). Weil FRN 27 im 4.
FSPEC-Oktett liegt, hat ein Record jetzt mindestens 4 FSPEC-Oktette (vorher 3).

## Warum das ein Breaking Wire-Change ist

Die FSPEC und die Item-Reihenfolge ändern sich: der byte-genaue Referenz-Dump
(`single_track_matches_reference_dump`) hat jetzt FSPEC `[0x9F, 0x0F, 0x01,
0x04]` und LEN 40 (statt 39). **Wayfinders Decoder muss in lockstep nachziehen**
(I062/500 FRN 16→27, neuer I062/136-Decode) — sonst bricht die Dekodierung.
Deshalb: ICD-Major-Bump auf **2.0.0**, ADR 0015 und Cross-Project-Issue
`from-firefly`.

## Verifikation

- `cat062::flight_level_scales_to_quarter_fl_and_signs_via_twos_complement` —
  Encoding-LSB + Vorzeichen (FL350 = 0x0578, −1000 ft = 0xFFD8).
- `cat062::decode_recovers_flight_level_when_present` — Round-Trip + Koexistenz
  mit den übrigen Items.
- `cat062::single_track_matches_reference_dump` — neu berechneter byte-genauer
  Dump (FSPEC/LEN).
- `tracker::measured_flight_level_reaches_system_track_and_stays_sticky` —
  Pass-through + Stickiness.
- **End-to-end:** Live-Multicast-Mitschnitt des Demo-Streams zeigt FRN 17 und
  FRN 27 gesetzt; I062/136 dekodiert zu einer plausiblen Flugfläche (FL374).
