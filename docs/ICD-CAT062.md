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

**1.1.0** (2026-06-13) — UTC Time-of-Day Migration (Issue #9).

### Changelog

| Version | Datum | Änderung |
|---------|-------|----------|
| 1.1.0 | 2026-06-13 | **UTC Time-of-Day in I062/070.** I062/070 wird jetzt als echte ASTERIX-Time-of-Day kodiert (Sekunden seit UTC-Mitternacht des Simulationstags), nicht relativ zur Szenario-Start-Zeit. `Scenario` trägt `simulation_start_time_of_day: f64` (Default 0 = Mitternacht); `Timestamp` bleibt intern deterministisch (Offset seit Szenario-Start). `Cat062Encoder` nimmt die Startzeit im Konstruktor entgegen. |
| 1.0.0 | 2026-06-13 | Erstfassung, extrahiert aus `firefly-asterix::cat062` und Wayfinders `CLAUDE.md` Abschnitt 2. |

---

## 1. Transport

| Eigenschaft | Wert |
|-------------|------|
| Protokoll | UDP-Multicast |
| Default-Gruppe | `239.255.0.62` (Env: `FIREFLY_CAT062_GROUP`) |
| Default-Port | `8600` (Env: `FIREFLY_CAT062_PORT`) |
| TTL | 1 (subnetz-lokal, Default) |
| Framing | **Ein Datagramm = ein vollständiger CAT062-Datenblock = ein Scan.** Keine zusätzliche Anwendungs-Rahmung (keine Sequenznummern, keine Extra-Header). |

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
| 11 | I062/380 | Aircraft Derived Data (nur Target-Address-Subfeld) | variabel | Primary Subfield Bit 8 (`ADR`, `0x80`) + 24-Bit Mode-S-Adresse, nur wenn vorhanden |
| 12 | I062/040 | Track Number | 2 Oktette | u16 BE |
| 13 | I062/080 | Track Status | variabel mit FX | siehe 4.1 |
| 14 | I062/290 | System Track Update Ages | variabel | siehe 4.2 |
| 16 | I062/500 | Estimated Accuracies | variabel | siehe 4.3 |

Items werden **nur kodiert, wenn der Wert vorhanden ist** — I062/060 und
I062/380 erscheinen nur, wenn der Track eine Mode-3/A- bzw. ICAO-Identität
trägt; das FSPEC spiegelt das automatisch.

### 4.1 I062/080 — Track Status

Variable Länge, Oktette verkettet über `FX = 0x01` (Bit 1 jedes Oktetts: 1 =
weiteres Oktett folgt).

| Oktett | Bit | Bedeutung |
|--------|-----|-----------|
| 1 | `0x02` (CNF) | gesetzt = Track ist noch **tentativ** (nicht bestätigt) |
| 4 | `0x80` (CST) | gesetzt = Track ist **coasting** (kein aktuelles Update) |

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

## 5. Koordinaten

- **I062/105 (WGS-84) ist die primäre, format-neutrale Position.** Wayfinder
  rendert direkt daraus — **keine** Rückprojektion nötig.
- **I062/100 (System-Stereografisch)** ist eine zusätzliche Systemebene,
  optional verwertbar (z. B. für Debugging/Vergleich). Referenzpunkt der
  Projektion ist aktuell der Demo-Ursprung (siehe Firefly-Roadmap: "Konfigurierbarer
  System-Referenzpunkt", offen).

## 6. Zeit (I062/070)

**✅ v1.1.0 (2026-06-13): I062/070 kodiert jetzt echte ASTERIX Time-of-Day (UTC).**
Jede `Timestamp` wird relativ zu `Scenario.simulation_start_time_of_day`
interpretiert. Beispiel: Wenn die Szenario um 06:00 UTC beginnt (21600 Sekunden)
und eine `Timestamp(3600.0)` ankommt, wird I062/070 als 07:00:00 UTC kodiert.
Der Simulator bleibt deterministisch (gleicher Input → gleicher Output), während
die Ausgabe semantisch korrekt ist.

## 7. Referenzen

- Referenz-Encoder: `crates/firefly-asterix/src/cat062.rs` (Firefly).
- Byte-genauer Referenz-Dump/Test: `single_track_matches_reference_dump`
  (Firefly, `firefly-asterix`).
- Architekturentscheidungen: Fireflys ADR 0006 (Integration/CAT062), ADR 0014
  (Produktions-Pivot, Wayfinder konsumiert CAT062/UDP).
- Kurzfassung für Wayfinder: Wayfinders `CLAUDE.md` Abschnitt 2.
