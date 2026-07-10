# QW.3 — Vertrauens-Flags im Track-Status (I062/080 MON + SPI)

> **Anforderung:** FR-TRK-036 · **ICD:** 3.2.0 (additiv) ·
> **Einstufung:** S2–S3, Schnittstellen-Wirkung → umgesetzt auf Fable 5

## Fachlich: Warum braucht das ASD das?

Der Track-Status I062/080 sagt dem Lotsen, **wie sehr er einem Track trauen
darf**. Bisher trug Firefly nur das Minimal-Set (CNF/TSE/CST). ARTAS füllt
zusätzlich Vertrauens-Flags, die Firefly intern längst kennt:

- **MON (monosensor):** Wird ein Track gerade nur von **einer** Quelle
  gestützt? Ohne zweite Quelle gibt es keine Kreuz-Prüfung — der Track ist
  anfälliger für Ghosts und Sensor-Bias. Das ASD kann Mono-Sensor-Tracks
  dezent kennzeichnen.
- **SPI („Ident"):** Der Pilot drückt auf Anweisung des Lotsen den
  Ident-Knopf; der Transponder sendet den SPI-Puls mit. Für den Lotsen ist
  das die klassische „Welches Ziel bist du?"-Bestätigung.
- **SIM:** Standard-Slot für simulierten Verkehr — dokumentiert belegt,
  Firefly sendet ihn immer 0 (ehrliche Grenze: kein Simulations-Konzept).

## Technisch

### MON — gefenstert statt pro Scan

Die naive Ableitung aus `contributing_sensors` (pro Mess-Gelegenheit geleert)
würde auf einem **asynchronen** Multi-Radar-Feed flattern: zwei Radare landen
selten in derselben 0,5-s-Gelegenheit, jede Einzelmeldung sähe „monosensor"
aus. Deshalb bucht der `Track` jetzt je **distinktem Sensor** die letzte
Treffer-Datenzeit (`sensor_hits`, Teil des serialisierbaren Zustands) und
zählt beim Berichten die Sensoren im **Frische-Fenster**
(`PROVENANCE_FRESH_S` = 30 s — dieselbe Frische-Semantik wie die
Provenienz-Ableitung aus ADR 0027). `monosensor = fresh_sensor_count ≤ 1`;
ein lange coastender Track (0 frische Sensoren) meldet ebenfalls MON.

### SPI — Ende-zu-Ende vom Radar

CAT048 liefert SPI heute schon: I048/020 Oktett 1 Bit 3. Der Decoder liest
das Bit (`TRD_SPI`), der `radar_asterix`-Adapter reicht es via `ModeAC.spi`
durch (ADS-B-/FLARM-Adapter setzen `false` — ihre Quellformate exponieren
SPI nicht). Am `Track` ist SPI bewusst **nicht sticky** (im Gegensatz zu
Squawk/ICAO/Callsign): jede assoziierte Meldung überschreibt es — I062/080
SPI bedeutet „in der **letzten** Meldung vorhanden".

### Draht

I062/080 Oktett 1: MON = `0x80`, SPI = `0x40` (neben CNF `0x02`); Oktett 2:
SIM = `0x80` (immer 0). **Additiv, kein Wire-Bruch** — nur zuvor konstant 0
gesendete Bits werden bei Bedarf gesetzt; ein Multisensor-Track ohne SPI ist
byte-identisch zu ICD 3.1.x (alle bestehenden Referenz-Dumps unverändert
grün). Der firefly-eigene Decoder liest MON/SPI zurück
(`DecodedRecord.monosensor`/`.spi`).

### Schnittstellen-Wirkung (Wayfinder)

Additiv, **kein Lockstep**: ein Decoder, der die Bits ignoriert, verhält sich
wie bisher. Empfehlung an Wayfinder (Cross-Project-Issue, `from-firefly`):
MON/SPI dekodieren und im ASD nutzen (Mono-Sensor-Kennzeichnung im
Label/Detail-Panel; SPI-Highlight beim Ident).

### Bewusst weggelassen: I062/295

Die Roadmap-Notiz zu QW.3 nannte auch I062/295 (Track Data Ages). Es wurde
**bewusst nicht** gebaut: I062/290 trägt seit ICD 2.6.0 bereits die
Update-Alter je Technologie — I062/295 würde dieselbe Information in einem
zweiten, teureren Item duplizieren (Wire-Bytes ohne ASD-Mehrwert).
Betreiber-Freigabe für diesen Zuschnitt: 2026-07-10.

## Tests

- `track::spi_is_transient_not_sticky`, `track::fresh_sensor_count_windows_per_sensor_hits`
- `tracker::monosensor_flag_follows_fresh_sensor_support` (1 Sensor → MON,
  2. Sensor dazu → multi, 2. Sensor verstummt > Fenster → wieder MON),
  `tracker::spi_reflects_the_last_associated_report`
- `cat062::track_status_carries_mon_and_spi_in_octet_one` (inkl.
  Baseline-Unverändertheit), `cat062::mon_and_spi_round_trip`
- `cat048::spi_bit_of_target_report_descriptor_is_decoded`

Gates: `cargo test --workspace` (47 Suiten grün), `cargo clippy` ohne
Befunde, `cargo fmt`. Keine neuen Env-Variablen (INSTALLATION/TECHNICAL
unverändert geprüft).
