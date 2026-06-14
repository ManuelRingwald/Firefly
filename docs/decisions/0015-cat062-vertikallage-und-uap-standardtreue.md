# ADR 0015 — CAT062 Vertikallage (I062/136) & UAP-Standardtreue

- **Status:** akzeptiert
- **Datum:** 2026-06-14
- **Schnittstellen-relevant:** ja (CAT062-Ausgabe-Vertrag, ICD → 2.0.0)

## Kontext

Der CAT062-Ausgabestrom war bisher rein **zweidimensional**: Position
(I062/105 WGS-84, I062/100 X/Y), Horizontalgeschwindigkeit (I062/185), Status,
Alter, Genauigkeit und optional Identität. Eine **Vertikallage** (Flugfläche)
fehlte vollständig.

Flugverkehrskontrolle ist inhärent 3D: ein Air Situation Display zeigt zu jedem
Track seine **Flugfläche (FL)**. Ohne sie ist das Lagebild fachlich
unvollständig — der größte Realitäts-Abstand zu einem echten ARTAS-/SDPS-Feed
(siehe Wayfinder-Review der ICD).

Bei der Einarbeitung fiel zusätzlich ein **vorbestehender Standard-Verstoß**
auf: I062/500 (Estimated Accuracies) lag im FSPEC auf **FRN 16**. In der echten
EUROCONTROL-CAT062-UAP (SUR.ET1.ST05.2000-STD-09-01) ist FRN 16 jedoch
I062/295 (Track Data Ages); I062/500 gehört auf **FRN 27**. Die übrigen zehn
Items von Firefly saßen bereits auf ihren echten Standard-FRNs (mit Spare auf
FRN 2) — nur I062/500 war falsch platziert und hätte von einem konformen
Fremd-Decoder als I062/295 fehlinterpretiert.

## Entscheidung

1. **Vertikallage über I062/136 (Measured Flight Level).** Neuer optionaler
   FRN-17-Slot, kodiert als signed i16, LSB 1/4 FL (25 ft). Wird nur emittiert,
   wenn der Track eine (Mode-C-)Flugfläche trägt — wie I062/060 und I062/380.
2. **Pass-through, kein vertikaler Filter.** Der Tracker-Kern führt **keine**
   eigene vertikale Kalman-Schätzung. `SystemTrack.flight_level_ft` ist die
   **zuletzt gemessene** Mode-C-Höhe (in Fuß), „sticky" wie die Identität: ein
   Primär-only-Treffer löscht den letzten bekannten Wert nicht. (Ein echter
   vertikaler Tracker bleibt mögliche Folgearbeit.)
3. **UAP auf volle Standardtreue ziehen.** I062/500 wandert von FRN 16 auf den
   echten **FRN 27**. Die Firefly-UAP ist damit ein **konformes Subset** der
   EUROCONTROL-CAT062-UAP: alle emittierten Items sitzen auf ihren echten FRNs,
   die Lücken sind die nicht-emittierten Standard-Items (FRN 2 Spare, 3, 8, 10,
   15, 16, 18–20 …). Ein konformer Fremd-Decoder (ARTAS-gespeistes ASD) liest
   den Strom ohne privates Profil.

## Begründung

- **Fachlich:** FL ist Pflicht-Information eines ASD; die 2D-Lücke war der
  größte Realitäts-Abstand.
- **Pass-through statt vertikalem Filter:** ehrlicher und einfacher — wir geben
  nur weiter, was gemessen wurde, ohne eine Genauigkeit vorzutäuschen, die ein
  fehlender vertikaler Schätzer nicht hat. Spiegelt das bestehende Muster der
  Identitäts-Felder.
- **Standardtreue:** Der gemeinsame Zeitpunkt (beide Repos vor-produktiv, beide
  Enden kontrolliert) ist der günstigste, um den I062/500-FRN-Fehler zu
  korrigieren, statt eine nicht-konforme UAP einzufrieren. Realitätsnähe zu
  ARTAS ist erklärtes Ziel.

## Konsequenzen

- **Breaking Wire-Change.** Die FSPEC eines Standard-Records wächst von 3 auf 4
  Oktette (FRN 27 liegt im 4. FSPEC-Oktett); I062/500 steht jetzt hinter dem
  optionalen I062/136. **Wayfinders Decoder muss in lockstep nachziehen**
  (FRN-16→27 für I062/500, neuer FRN-17-Decode für I062/136) — sonst bricht die
  Dekodierung. Cross-Project-Issue `from-firefly` + ICD 2.0.0.
- `SystemTrack` bekommt das Feld `flight_level_ft: Option<f64>` (Fuß). Der
  JSON-Frame-Adapter (`firefly-io`) ist davon **unberührt** (eigener
  `FrameTrack`-Wire-Typ); nur der CAT062-Adapter nutzt das Feld.
- Anforderungen: neues **FR-TRK-027** (FL-Pass-through), Erweiterung von
  FR-IO-003/004 um I062/136 und die FRN-Korrektur.
- Byte-genauer Referenz-Dump (`single_track_matches_reference_dump`) neu
  berechnet (FSPEC `[0x9F,0x0F,0x01,0x04]`, LEN 39→40).

## Ehrliche Grenze

I062/136 ist eine **gemessene** (Mode-C-)Flugfläche, kein geglätteter
vertikaler Track-Zustand. Geometrische/barometrische Berechnungs-Höhen
(I062/130/135), Steig-/Sinkrate (I062/220) und ein vertikaler Schätzer bleiben
bewusst offen.
