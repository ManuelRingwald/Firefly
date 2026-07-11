# REG.3 — Sensor-Registrierung: Bias-Statistik auf den Draht (CAT063)

> **Anforderung:** FR-IO-008 · **ADR:** 0034 · **ICD:** 3.3.0 (additiv) ·
> **Einstufung:** S3 (Roadmap-Empfehlung Sonnet 4.6; wegen
> Schnittstellen-Wirkung auf Fable 5 umgesetzt)

## Fachlich: Warum?

Mit REG.2b korrigiert Firefly Radar-Biases intern — nach außen war dieser
Kalibrierungs-Zustand bislang unsichtbar. ARTAS publiziert seine
Registrierungs-Ergebnisse im CAT063-Sensor-Status, aus gutem Grund:
Nachgelagerte Konsumenten (ASD, Auswerte-/Recording-Systeme, Nachbarsysteme)
sollen je Sensor sehen, *wie verschoben* er misst und was das SDPS gerade
herausrechnet. Für Supervisor/Technik ist das ein Gesundheitssignal: Ein
Radar, dessen Bias plötzlich wächst, kündigt ein Kalibrierungs- oder
Hardware-Problem an, bevor das Lagebild leidet. Wayfinder kann das je Sensor
anzeigen (analog zur bestehenden CAT063-Liveness).

## Technik

**Encoder/Decoder** (`firefly-asterix::cat063`, additiv):

- **I063/080** (FRN 7, 4 Oktette): `SRG` Range Gain (16-Bit-Zweierkomplement,
  LSB 10⁻⁵ — Firefly schätzt keinen Gain, sendet **immer 0**) + `SRB` Range
  Bias (16-Bit-Zweierkomplement, LSB **1/128 NM ≈ 14,47 m**).
- **I063/081** (FRN 8, 2 Oktette): `SAB` Azimut-Bias (16-Bit-Zweierkomplement,
  LSB **360/2¹⁶ ° ≈ 0,0055°**).
- Skalierung **sättigt** an den i16-Grenzen statt zu wrappen (ein geklemmter
  Extremwert ist ehrlicher als ein umgeklappter).
- Decoder liest beide Items einzeln FSPEC-getrieben zurück
  (`DecodedSensorStatus.ssr_bias`) — Grundwahrheit für Wayfinder.

**Wert-Quelle & Sende-Regel (die zwei bewussten Entscheidungen):**

1. Publiziert wird die **angewandte** Korrektur (REG.2b) — das, was das SDPS
   tatsächlich von den Messungen abzieht —, nicht der rohe Schätzwert. Der
   Draht beschreibt den Zustand des Lagebilds, nicht den Forschungsstand des
   Schätzers.
2. Die Items erscheinen **nur bei in Kraft befindlicher Korrektur**
   (`FIREFLY_REGISTRATION_APPLY` aktiv und Anwendungs-Gate bestanden).
   **Absenz = „keine Korrektur"** — eine gesendete 0 würde fälschlich „Bias
   exakt Null bestätigt" behaupten. Ohne Korrektur ist der Record
   byte-identisch zur Vor-REG.3-Form (kein Wire-Bruch); mit Korrektur wächst
   die FSPEC auf `0xBB 0x80` (Record 16 Oktette).

**Datenfluss:** `LiveTracker`-Tick → `Metrics.registration_applied_biases`
(bestehende Mutex-Map) → Bias-Provider-Closure des `run_cat063_sender`
(einmal je Sendetakt abgefragt, kein neuer geteilter Zustand) →
`SensorReport.ssr_bias` → Wire. `firefly-multicast` bleibt vom Server-Zustand
entkoppelt (Closure-Injektion, Ports & Adapters).

**Nicht gesendet (ehrliche Grenzen):** I063/070 (Time Stamping Bias) und
I063/090–092 (PSR-Bias) — Firefly schätzt (noch) keinen Zeitstempel- und
keinen PSR-spezifischen Bias; Absenz statt Null gilt auch hier.

## Schnittstellen-Wirkung

**ICD 3.2.0 → 3.3.0, additiv** — ein Decoder ohne Bias-Auswertung überspringt
die Items FSPEC-getrieben (feste Längen, Standard-UAP-Positionen); kein
Lockstep nötig. Wayfinder-Nachzug (Decoder + Anzeige) per
`from-firefly`-Issue, referenziert in
`docs/cross-project/todo-for-wayfinder.md`.

## Tests

`firefly-asterix` (5 neu): byte-genauer Referenz-Dump (FSPEC `0xBB 0x80`,
SRB=10, SAB=55), Round-Trip innerhalb eines LSB inkl. negativer Werte, keine
Items ohne Korrektur (byte-identisch zur alten Form), gemischte Records in
einem Block, Sättigung statt Wrap. `firefly-multicast` (1 neu + 1 erweitert):
End-to-End über UDP — Sensor mit Korrektur trägt seinen Bias auf dem Draht,
Sensor ohne bleibt blank. Fuzzing: der bestehende `cat063_decode`-Fuzz-Target
deckt die neuen Decoder-Pfade automatisch mit ab. Gates:
`cargo test --workspace`, `clippy`, `fmt` grün.
