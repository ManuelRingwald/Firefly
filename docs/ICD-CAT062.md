# ICD — CAT062/UDP-Multicast (Firefly ↔ Wayfinder)

> **Interface Control Document.** Dies ist die **maßgebliche, versionierte
> Beschreibung** des einzigen Berührungspunkts zwischen Firefly (Sender) und
> Wayfinder (Empfänger): der ASTERIX-CAT062-Datenstrom über UDP-Multicast.
> Kein gemeinsamer Code, keine Bibliotheks-Abhängigkeit — beide Seiten
> implementieren diesen Vertrag unabhängig.
>
> **Eigentümerschaft & Änderungsprozess:** Dieses Dokument lebt im
> Firefly-Repo, da Firefly der Sender/Encoder ist. Jede Änderung ist
> **schnittstellen-relevant**:
> 1. Änderung hier per ADR in Firefly begründen (Firefly `CLAUDE.md` §4/§9).
> 2. Versionsnummer unten erhöhen, Änderung im Changelog eintragen.
> 3. Wayfinder informieren — Issue mit Label `from-firefly` im
>    Wayfinder-Repo, referenziert in Wayfinders
>    `docs/cross-project/todo-for-wayfinder.md`.
> 4. Wayfinders `CLAUDE.md` Abschnitt 2 (Kurzfassung des Vertrags) entsprechend
>    nachziehen.

---

## Version

**2.2.0** (2026-06-15) — **Additiv:** Track-Ende-Signal (TSE) in I062/080
Oktett 2 (ADR 0016).

### Changelog

| Version | Datum | Änderung |
|---------|-------|----------|
| 2.2.0 | 2026-06-15 | **Additiv (ADR 0016).** I062/080 (Track Status) trägt jetzt das **TSE-Bit** (*Track Service End*, Oktett 2, Bit 7, `0x40`): es markiert die **letzte** Meldung für einen Track (er wird gelöscht). Erscheint nur bei gelöschten Tracks; ein gelöschter Track wird damit **genau einmal** mit gesetztem TSE gemeldet und danach nicht mehr. I062/080 ist bereits ein variabel langes FX-Item (FRN 13, in jedem Record) — kein FSPEC-Wachstum, kein Breaking Change. **Konsument muss TSE als „Track entfernen" interpretieren** (sonst Ein-Frame-Geist). |
| 2.1.0 | 2026-06-15 | **Additiv (AP7).** Neues optionales Item **I062/245** (Target Identification / Callsign, FRN 10, 7 Oktette: STI/spare-Oktett + 8 × 6-Bit-IA-5-Zeichen) — nur wenn der Track jemals eine Mode-S-Kennung getragen hat (sticky wie Mode 3/A). FRN 10 liegt im bereits vorhandenen 2. FSPEC-Oktett — kein Wachstum der FSPEC-Länge, kein Breaking Change für bestehende Decoder. |
| 2.0.0 | 2026-06-14 | **BREAKING (ADR 0015).** (1) Neues optionales Item **I062/136** (Measured Flight Level, FRN 17, signed i16, LSB 1/4 FL = 25 ft) — nur wenn der Track eine Mode-C-Flugfläche trägt. (2) **I062/500** (Estimated Accuracies) wandert von **FRN 16 → FRN 27**, dem echten EUROCONTROL-UAP-Slot; FRN 16 (I062/295) bleibt reserviert/ungenutzt. Die Standard-Record-FSPEC wächst dadurch von 3 auf 4 Oktette. Decoder **muss** beide Änderungen nachziehen. |
| 1.1.1 | 2026-06-14 | **Doku-Politur (kein Wire-Format-Change).** Normative Spec-Edition referenziert (Abschnitt 0), Update-Rate/Scan-Period dokumentiert (Abschnitt 1), Mitternachts-Rollover von I062/070 präzisiert (Abschnitt 6), Stand zum I062/100-Referenzpunkt verlinkt (Abschnitt 5). |
| 1.1.0 | 2026-06-13 | **UTC Time-of-Day in I062/070.** I062/070 wird jetzt als echte ASTERIX-Time-of-Day kodiert (Sekunden seit UTC-Mitternacht des Simulationstags), nicht relativ zur Szenario-Start-Zeit. `Scenario` trägt `simulation_start_time_of_day: f64` (Default 0 = Mitternacht); `Timestamp` bleibt intern deterministisch (Offset seit Szenario-Start). `Cat062Encoder` nimmt die Startzeit im Konstruktor entgegen. |
| 1.0.0 | 2026-06-13 | Erstfassung, extrahiert aus `firefly-asterix::cat062` und Wayfinders `CLAUDE.md` Abschnitt 2. |

---

## 0. Normative Referenz

Alle Item-Kodierungen (Längen, LSB-Werte, Bit-Layouts) in Abschnitt 4 sind
gegen **EUROCONTROL SUR.ET1.ST05.2000-STD-09-01, Edition 1.10** ("CAT062
System Track Data") verifiziert — siehe Doc-Kommentare in
`crates/firefly-asterix/src/cat062.rs` (jede Item-Kodierung referenziert die
jeweilige Spec-Sektion, z. B. §5.2.20, §5.2.24, §5.2.26). Diese ICD beschreibt
ein **bewusst gewähltes Subset** dieser Edition (siehe Abschnitt 4); sie
ersetzt die Edition nicht als normative Quelle.

---

## 1. Transport

| Eigenschaft | Wert |
|-------------|------|
| Protokoll | UDP-Multicast |
| Default-Gruppe | `239.255.0.62` (Env: `FIREFLY_CAT062_GROUP`) |
| Default-Port | `8600` (Env: `FIREFLY_CAT062_PORT`) |
| TTL | 1 (subnetz-lokal, Default) |
| Framing | **Ein Datagramm = ein vollständiger CAT062-Datenblock = ein Scan.** Keine zusätzliche Anwendungs-Rahmung (keine Sequenznummern, keine Extra-Header). |

**Update-Rate.** Es gibt **keine feste, globale Update-Periode** — jeder Sensor
hat seine eigene `scan_period` (typisch 4–12 s, konfiguriert pro Radar, siehe
ADR 0013). Jeder abgeschlossene Scan eines Sensors erzeugt einen
Datenblock/Scan im Sinne dieser ICD; Wayfinder muss daher mit Datenblöcken in
unregelmäßigem Takt rechnen, nicht mit einem festen Intervall.

## 2. Datenblock-Format

```
[CAT = 0x3E] [LEN: u16 BE] [Record]...
```

- `CAT` = 1 Oktett, immer `0x3E` (62).
- `LEN` = 2 Oktette, big-endian, **Gesamtlänge inklusive** des 3-Oktett-Headers
  (`CAT` + `LEN`).
- Danach folgen **mehrere Records ohne Trenner** — jeder Record ist über sein
  **FSPEC** selbst-begrenzend (siehe Abschnitt 3).

## 3. Record-Format (FSPEC/UAP)

Jeder Record beginnt mit einem **FSPEC** (Field Specification): eine Folge von
Oktetten, deren Bits 8–2 angeben, welche Items (FRNs 1–7 je Oktett) vorhanden
sind; Bit 1 (`FX = 0x01`) zeigt an, ob ein weiteres FSPEC-Oktett folgt. Danach
folgen die Items **in UAP-Reihenfolge** (User Application Profile), wie im
FSPEC markiert.

**Vorwärtskompatibilität:** Der Decoder ist **tolerant** gegenüber unbekannten/
zusätzlichen FSPEC-Bits — unbekannte Items werden anhand ihrer Längen-Regeln
übersprungen, nicht als Fehler behandelt.

## 4. Items (FRN → Inhalt)

| FRN | Item | Bedeutung | Länge | Kodierung |
|-----|------|-----------|-------|-----------|
| 1 | I062/010 | Data Source Identifier (SAC/SIC) | 2 Oktette | `[SAC, SIC]`, verbatim |
| 4 | I062/070 | Time of Track Information | 3 Oktette | u24 BE, Ticks zu 1/128 s seit Mitternacht (Time-of-Day) |
| 5 | I062/105 | Calculated Track Position (WGS-84) | 8 Oktette | Lat, Lon je i32 BE, LSB = 180/2²⁵° |
| 6 | I062/100 | Calculated Track Position (System-Stereografisch X/Y) | 6 Oktette | X, Y je i24 BE (Zweierkomplement), LSB = 0,5 m |
| 7 | I062/185 | Calculated Track Velocity (Cartesian Vx/Vy) | 4 Oktette | Vx, Vy je i16 BE, LSB = 0,25 m/s |
| 9 | I062/060 | Track Mode 3/A Code | 2 Oktette | 12-Bit-Antwort (4 Oktal-Ziffern) in den unteren 12 Bit |
| 10 | I062/245 | Target Identification (Callsign, nur wenn vorhanden) | 7 Oktette | STI/spare-Oktett + 8 × 6-Bit-IA-5-Zeichen; siehe 4.5 |
| 11 | I062/380 | Aircraft Derived Data (nur Target-Address-Subfeld) | variabel | Primary Subfield Bit 8 (`ADR`, `0x80`) + 24-Bit Mode-S-Adresse, nur wenn vorhanden |
| 12 | I062/040 | Track Number | 2 Oktette | u16 BE |
| 13 | I062/080 | Track Status | variabel mit FX | siehe 4.1 |
| 14 | I062/290 | System Track Update Ages | variabel | siehe 4.2 |
| 17 | I062/136 | Measured Flight Level (nur wenn vorhanden) | 2 Oktette | signed i16 BE, LSB = 1/4 FL = 25 ft; siehe 4.4 |
| 27 | I062/500 | Estimated Accuracies | variabel | siehe 4.3 |

> **UAP-Standardtreue (ADR 0015).** Die FRNs folgen der echten EUROCONTROL-
> CAT062-UAP (SUR.ET1.ST05.2000-STD-09-01). Die Lücken sind die nicht
> emittierten Standard-Items: FRN 2 (Spare), 3 (I062/015), 8 (I062/210),
> 15 (I062/200), **16 (I062/295 — reserviert, ungenutzt)**, 18–20
> (I062/130/135/220). Ein konformer Fremd-Decoder liest den Strom ohne privates
> Profil. Weil I062/500 auf FRN 27 (4. FSPEC-Oktett) liegt, hat ein Record
> mindestens **4 FSPEC-Oktette**.

Items werden **nur kodiert, wenn der Wert vorhanden ist** — I062/060, I062/245
und I062/380 erscheinen nur bei vorhandener Mode-3/A-, Callsign- bzw.
ICAO-Identität, I062/136 nur bei vorhandener Mode-C-Flugfläche; das FSPEC
spiegelt das automatisch.

### 4.1 I062/080 — Track Status

Variable Länge, Oktette verkettet über `FX = 0x01` (Bit 1 jedes Oktetts: 1 =
weiteres Oktett folgt).

| Oktett | Bit | Bedeutung |
|--------|-----|-----------|
| 1 | `0x02` (CNF) | gesetzt = Track ist noch **tentativ** (nicht bestätigt) |
| 2 | `0x40` (TSE) | gesetzt = **letzte** Meldung für den Track (er wird gelöscht); Konsument **entfernt** den Track (ADR 0016) |
| 4 | `0x80` (CST) | gesetzt = Track ist **coasting** (kein aktuelles Update) |

Das Item verlängert sich nur so weit wie das höchste gesetzte Flag: CST →
Oktett 4, sonst TSE → Oktett 2, sonst nur Oktett 1. Ein lebender, nicht
coastender Track bleibt ein einzelnes Oktett (TSE/CST default 0). Ein gelöschter
Track ist typischerweise zugleich coasting — dann sitzt TSE in Oktett 2 und CST
in Oktett 4 desselben Records.

### 4.2 I062/290 — System Track Update Ages

Compound Item: Primary Subfield (1 Oktett) + Subfelder je gesetztem Bit.

| Primary-Subfield-Bit | Subfeld | Länge | Kodierung |
|----------------------|---------|-------|-----------|
| Bit 7 (spec Bit 15, `0x40`, "PSR") | PSR-Age | 1 Oktett | u8, LSB = 0,25 s |

Aktuell wird nur das PSR-Age-Subfeld kodiert, aus `SystemTrack.update_age`.

### 4.3 I062/500 — Estimated Accuracies

Compound Item: Primary Subfield (1 Oktett) + Subfelder je gesetztem Bit.

| Primary-Subfield-Bit | Subfeld | Länge | Kodierung |
|----------------------|---------|-------|-----------|
| Bit 8 (spec Bit 16, `0x80`, "APC") | Accuracy of Calculated Position (Cartesian) | 4 Oktette (X, Y je u16 BE) | LSB = 0,5 m |

Aktuell wird nur APC kodiert, aus `SystemTrack.position_uncertainty` (1σ,
isotrop — gleicher Wert für X und Y).

### 4.4 I062/136 — Measured Flight Level

Zwei Oktette, signed 16-Bit (Zweierkomplement), big-endian; LSB = 1/4 FL =
25 ft. Der kodierte Wert ist `round(flight_level_ft / 25)`. Negative
Flugflächen (unter dem 1013,25-hPa-Datum) sind regulär per Zweierkomplement
abgebildet.

Quelle ist die **zuletzt gemessene Mode-C-Höhe** (`SystemTrack.flight_level_ft`,
in Fuß). Es ist eine *gemessene* Größe, kein geglätteter vertikaler
Track-Zustand — Firefly führt (noch) keinen vertikalen Schätzer (ADR 0015).
Das Item erscheint nur, wenn der Track jemals eine Mode-C-Antwort getragen hat
(sticky wie die Identität).

### 4.5 I062/245 — Target Identification (Callsign)

Sieben Oktette:

| Oktett(e) | Inhalt | Kodierung |
|-----------|--------|-----------|
| 1 | STI (Source of Target Identification, Bits 8/7) + 6 Spare-Bits | `0x00` = "Downlinked Target Identification" (Mode-S-Downlink-Antwort, unverändert durchgereicht) |
| 2–7 | Callsign / Flight ID, 8 Zeichen | 8 × 6-Bit-IA-5-Code, MSB-first (48 Bit = 6 Oktette) |

**6-Bit-IA-5-Kodierung** (ICAO Annex 10): `A`–`Z` → 1–26, `0`–`9` → 48–57,
Leerzeichen (und jeder andere Code defensiv beim Decoder) → 32. Das Callsign
wird auf 8 Zeichen mit Leerzeichen aufgefüllt bzw. abgeschnitten.

Quelle ist die **zuletzt empfangene Mode-S-Kennung**
(`SystemTrack.callsign`) — ein reiner Durchreich-Wert, kein von Firefly
generierter Bezeichner. Das Item erscheint nur, wenn der Track jemals eine
Kennung getragen hat (sticky wie Mode 3/A und die Flugfläche).

## 5. Koordinaten

- **I062/105 (WGS-84) ist die primäre, format-neutrale Position.** Wayfinder
  rendert direkt daraus — **keine** Rückprojektion nötig.
- **I062/100 (System-Stereografisch)** ist eine zusätzliche Systemebene,
  optional verwertbar (z. B. für Debugging/Vergleich). Referenzpunkt der
  Projektion ist aktuell der Demo-Ursprung (Frankfurt-Szenario,
  `Cat062Encoder::new(source, system_reference_point)` in
  `crates/firefly-asterix/src/cat062.rs`). Ein frei konfigurierbarer
  System-Referenzpunkt ist als offener Punkt in
  `docs/decisions/0006-integration-phoenix-asd-cat062.md` (Abschnitt
  "Nachtrag (Häppchen C.1–C.3)", Unterabschnitt "Ehrliche Grenze") und in der
  Firefly-Roadmap ("Konfigurierbarer System-Referenzpunkt") vermerkt — bis
  dahin ist I062/100 nur im Demo-Kontext sinnvoll interpretierbar, I062/105
  (WGS-84) bleibt die primäre, kontextfreie Position.

## 6. Zeit (I062/070)

**✅ v1.1.0 (2026-06-13): I062/070 kodiert jetzt echte ASTERIX Time-of-Day (UTC).**
Jede `Timestamp` wird relativ zu `Scenario.simulation_start_time_of_day`
interpretiert. Beispiel: Wenn die Szenario um 06:00 UTC beginnt (21600 Sekunden)
und eine `Timestamp(3600.0)` ankommt, wird I062/070 als 07:00:00 UTC kodiert.
Der Simulator bleibt deterministisch (gleicher Input → gleicher Output), während
die Ausgabe semantisch korrekt ist.

**Mitternachts-Rollover.** I062/070 ist ein 24-Bit-Zähler in 1/128-s-Ticks seit
UTC-Mitternacht (Wertebereich 0 … 86 399,99…, max. 11 059 200 Ticks < 2²⁴) und
wird als `(simulation_start_time_of_day + timestamp) % 86400` Sekunden
kodiert — der Zähler **springt bei Mitternacht auf 0 zurück**, unabhängig
davon, wie lange das Szenario schon läuft. Wayfinder darf daraus **keinen
monoton steigenden Zeitstempel über Mitternacht hinweg ableiten**; ein
Sprung von einem Wert nahe 86 400 s auf einen kleinen Wert ist ein normaler
Tageswechsel, kein Datenfehler.

## 7. Referenzen

- Referenz-Encoder: `crates/firefly-asterix/src/cat062.rs` (Firefly).
- Byte-genauer Referenz-Dump/Test: `single_track_matches_reference_dump`
  (Firefly, `firefly-asterix`).
- Architekturentscheidungen: Fireflys ADR 0006 (Integration/CAT062), ADR 0014
  (Produktions-Pivot, Wayfinder konsumiert CAT062/UDP).
- Kurzfassung für Wayfinder: Wayfinders `CLAUDE.md` Abschnitt 2.
