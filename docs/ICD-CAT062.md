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

**3.1.0** (2026-07-06) — **Additiv (ADR 0033):** CAT063-Records tragen bei einem **degradierten** Sensor optional einen **per-Quelle-Fehlergrund** im **I063/RE** (Reserved Expansion Field, FRN 13): Vendor-Subfeld **`SRC-REASON`** (u8: `1=unreachable`, `2=auth`, `3=rate_limited`). Damit unterscheidet der Konsument, **warum** eine Quelle ausgefallen ist — Netz/Firewall (unreachable) vs. falsche Credentials (auth) vs. Drosselung (rate_limited) — statt nur „degradiert" anzuzeigen (Antwort auf Wayfinder #197). Das RE-Feld ist **selbst-begrenzend** (Längen-Oktett) und wird **nur** bei degradiertem Sensor mit bekanntem Grund gesendet — operationelle Records bleiben die 9-Oktett-Form. **Additiv, kein Wire-Bruch:** ein Decoder, der das RE-Feld nicht auswertet, überspringt es über sein Längen-Oktett (die Vorwärtskompatibilitäts-Regel aus 3.0.0). Grund kommt heute aus den HTTP-ADS-B-Pollern (OpenSky, adsb_aggregator); FLARM/Radar liefern keinen Grund (RE entfällt). Details: Abschnitt 9.

Vorgänger **3.0.0** (2026-07-06) — **BREAKING (ADR 0032):** Die **CAT063-UAP** wird auf die **echten EUROCONTROL-FRN-Positionen** gebracht (spiegelt die CAT062-UAP-Korrektur aus 2.0.0 / ADR 0015). Drei Änderungen am CAT063-Record: (1) **I063/010** trägt jetzt die **SDPS-Identität** (SAC/SIC = `FIREFLY_CAT062_SAC`/`_SIC`, Default 25/2 — dieselbe wie I062/010 und I065/010), **nicht mehr** die Sensor-Identität. (2) Neues Item **I063/050** (Sensor Identifier, FRN 4) trägt die **Sensor-Identität** (SAC 0, SIC = `sensor_id`). (3) I063/030 wandert **FRN 2 → FRN 3**, I063/060 wandert **FRN 3 → FRN 5**. Die FSPEC wächst von `0xE0` auf **`0xB8`** (FRN 1+3+4+5), der Record von 7 auf **9 Oktette**. Zusätzlich sind die **CON-Werte** (I063/060) auf die Standard-Kodierung korrigiert: `0` = operationell, `1` = degradiert, `2` = Initialisierung, `3` = nicht verbunden (Firefly sendet weiter nur `0x00`/`0x40`). **Decoder muss nachziehen** — die Sensor-Identität kommt jetzt aus I063/050 (FRN 4), nicht aus I063/010. CAT062/CAT065 unverändert. Details: Abschnitt 9.

Vorgänger **2.6.1** (2026-07-04) — **Dokumentarisch (ADR 0030, kein Wire-Format-Bruch):** Der Replay-/Szenen-Modus des Senders wurde ausgebaut; Firefly läuft ausschließlich als quellen-getriebener Live-Tracker. Für den Vertrag ändert sich **nichts am Format** — alle Replay-Bezüge dieser ICD (Szenen-Ursprung als I062/100-Referenz, CAT063-Replay-Verhalten, feste Szenen-SICs) sind durch die quellen-getriebenen Formulierungen ersetzt. Eine Instanz **ohne** Quellen sendet weiterhin CAT065-Heartbeats (und CAT063), aber keine CAT062-Tracks — „leerer Himmel" bleibt vom „toten Feed" unterscheidbar (ADR 0018).

Vorgänger **2.6.0** (2026-06-30) — **Additiv (ADR 0027, Firefly #30):** I062/290 (System Track Update Ages) trägt jetzt optional **per-Technologie-Alter** — **SSR** (`0x20`), **Mode S** (`0x10`) und **FLARM** (`0x04`, Firefly-Vendor-Subfeld) — zusätzlich zu PSR (`0x40`) und ES/ADS-B (`0x08`). Die Age-Oktette folgen der Bit-Priorität MSB→LSB. Damit liefert Firefly die **autoritative Track-Provenienz** im Strom; der Konsument leitet ◆ ADS-B / ▢ SSR / ○ PSR / FLARM aus den Age-Subfeldern ab statt zu raten (ersetzt Wayfinders `provenance.js`-Heuristik). Strikt additiv — bestehende PSR/ES-Subfelder unverändert, kein Wire-Format-Bruch. Details: Abschnitt 4.2.

Vorgänger **2.5.0** (2026-06-25) — **Additiv:** Neue Kategorie **CAT063** (Sensor Status Messages, `0x3F`) auf demselben Multicast-Strom. Periodische Per-Sensor-Statusmeldung (Default 5 s, `FIREFLY_CAT063_PERIOD`): je Tick ein Block mit einem Record pro registriertem Sensor (I063/010 SAC/SIC, I063/030 ToD, I063/060 NOGO operationell/degradiert). Erlaubt dem Konsumenten einen ausgefallenen Sensor von einem leeren Himmel zu unterscheiden — Grundlage für Wayfinders Sensor-Degradierungs-Banner. Konsument dispatcht am CAT-Oktett (`0x3F`). Details: Abschnitt 9.

> ℹ️ **Geltungsbereich.** Diese ICD beschreibt den **gesamten
> Multicast-Ausgabe-Vertrag** zwischen Firefly und Wayfinder. Seit 2.3.0
> trägt der Strom mehrere ASTERIX-Kategorien: **CAT062** (System-Tracks,
> Abschnitte 2–6), **CAT065** (SDPS-Service-Status / Heartbeat, Abschnitt 8)
> und seit 2.5.0 **CAT063** (Sensor Status Messages, Abschnitt 9). Der
> Dateiname (`ICD-CAT062.md`) bleibt aus Historie erhalten.

### Changelog

| Version | Datum | Änderung |
|---------|-------|----------|
| 3.1.0 | 2026-07-06 | **Additiv (ADR 0033).** CAT063 trägt bei einem degradierten Sensor optional den **per-Quelle-Fehlergrund** im **I063/RE** (Reserved Expansion Field, FRN 13): Vendor-Subfeld **`SRC-REASON`** (u8: `1=unreachable`, `2=auth`, `3=rate_limited`). RE-Layout: `[LEN=0x03][SUBFIELD=0x80][SRC-REASON]`, selbst-begrenzend. **Nur** bei degradiertem Sensor mit bekanntem Grund gesendet (operationelle Records unverändert 9 Oktette). FSPEC wächst dann auf 2 Oktette (`0xB9 0x04`). **Kein Wire-Bruch:** ein Decoder, der RE nicht auswertet, überspringt es über sein Längen-Oktett. Grund aus den HTTP-ADS-B-Pollern (OpenSky/adsb_aggregator); FLARM/Radar ohne Grund (kein RE). Antwort auf Wayfinder #197. Details: Abschnitt 9. |
| 3.0.0 | 2026-07-06 | **BREAKING (ADR 0032).** **CAT063-UAP-Standardisierung** — die Sensor-Status-Records folgen jetzt den echten EUROCONTROL-FRN-Positionen (analog zur CAT062-Korrektur aus 2.0.0). (1) **I063/010** = **SDPS**-Identität (SAC/SIC = `FIREFLY_CAT062_SAC`/`_SIC`, Default 25/2), nicht mehr der Sensor. (2) Neues **I063/050** (Sensor Identifier, FRN 4) = **Sensor**-Identität (SAC 0, SIC = `sensor_id`). (3) I063/030 → FRN 3, I063/060 → FRN 5. FSPEC `0xE0` → **`0xB8`**, Record 7 → **9 Oktette**. CON-Werte (I063/060) auf Standard korrigiert: `0` op / `1` degradiert / `2` Init / `3` nicht verbunden (Firefly sendet weiter `0x00`/`0x40`). **Decoder muss nachziehen**: Sensor-Identität aus I063/050 lesen, FSPEC `0xB8` erwarten. Kein Eingriff in CAT062/CAT065. Details: Abschnitt 9. |
| 2.6.1 | 2026-07-04 | **Dokumentarisch (ADR 0030).** Replay-/Szenen-Modus des Senders ausgebaut — Firefly ist ausschließlich quellen-getrieben (`FIREFLY_SOURCES`/Adapter-Envs). **Kein Wire-Format-Bruch:** CAT062/065/063 byte-identisch. ICD-Anpassungen rein redaktionell: I062/100-Referenz = System-Referenzpunkt (Union-Bbox-Mitte bzw. `FIREFLY_SYSTEM_REF_*`), CAT063-SICs = `sensor_id` der Quellen, CAT063-Liveness folgt immer dem echten Plot-Eingang. Instanz ohne Quellen: CAT065/CAT063 laufen, keine CAT062-Tracks (leerer Himmel). |
| 2.6.0 | 2026-06-30 | **Additiv (ADR 0027, Firefly #30).** I062/290 (System Track Update Ages) trägt jetzt **per-Technologie-Alter**: zusätzlich zu PSR (`0x40`) und ES/ADS-B (`0x08`) optional **SSR-Age** (`0x20`), **Mode-S-Age** (`0x10`) und **FLARM-Age** (`0x04`). Die Age-Oktette folgen der Bit-Priorität MSB→LSB im Primary-Subfeld (PSR → SSR → MDS → ES → FLARM), je 1 Oktett, u8, LSB 0,25 s. Ein Age-Oktett ist nur vorhanden, wenn das zugehörige Bit gesetzt ist (Track hat einen Treffer dieser Technologie). PSR-only-Tracks: **kein** Unterschied zum bisherigen Wire-Format (2 Byte). Quelle: `SystemTrack.source_ages`. Damit kann der Konsument die **Track-Provenienz** (◆ ADS-B / ▢ SSR / ○ PSR / FLARM) aus den Age-Subfeldern ableiten, statt im Frontend zu raten. **Firefly-Bit-Map bleibt:** `0x40`/`0x08` unverändert (Wayfinder-Decoder bricht nicht); neue Subfelder auf freien Bits; **FLARM (`0x04`) ist ein dokumentiertes Firefly-Vendor-Subfeld** (kein EUROCONTROL-Standard-Subfeld, vom toleranten Decoder überspringbar). **Konsument (Wayfinder): kein Breaking Change** — I062/290 muss ohnehin variabel lang dekodiert werden (Länge/Reihenfolge aus dem Primary-Subfeld). Details: Abschnitt 4.2. |
| 2.5.0 | 2026-06-25 | **Additiv (ADR 0022, Firefly #32).** Neue Kategorie **CAT063** (Sensor Status Messages, CAT-Oktett `0x3F`) auf **derselben** Multicast-Gruppe/Port wie CAT062/CAT065. Periodische Per-Sensor-Statusmeldung (wall-clock-getaktet, Default 5 s, `FIREFLY_CAT063_PERIOD`): **ein Block pro Tick mit einem Record je registriertem Sensor**, FSPEC `0xE0` → I063/010 (SAC/SIC), I063/030 (Time of Day, 1/128 s), I063/060 (NOGO: `0x00` operationell / `0x40` degradiert). Ein Sensor gilt als degradiert, wenn er innerhalb von `2.5 × scan_period` keinen Plot geliefert hat. Erlaubt dem Konsumenten, einen **ausgefallenen Sensor** von einem **leeren Himmel** zu unterscheiden (CAT065 sagt „SDPS lebt", CAT063 sagt „welche Sensoren liefern"). **Konsument muss am CAT-Oktett dispatchen** (`0x3E` Track, `0x41` Heartbeat, `0x3F` Sensor-Status) und unbekannte Kategorien überspringen — robuste-Decoder-Regel galt ohnehin. Kein Eingriff in CAT062/CAT065. Details: Abschnitt 9. |
| 2.4.0 | 2026-06-18 | **Additiv (AP9.5).** I062/290 (System Track Update Ages) trägt jetzt optional das **ES-Age-Subfeld** (Extended Squitter / ADS-B): Bit `0x08` im primären Subfeld-Oktett signalisiert, dass ein ES-Age-Byte folgt. Das ES-Age-Byte kodiert das ADS-B-Trefferalter in 1/4-Sekunden (identisch zum PSR-Age). Ist `0x08` nicht gesetzt, fehlt das Byte und das Item ist weiterhin 2 Byte lang (Subfeld + PSR-Age). Für Tracks ohne ADS-B-Treffer: kein Unterschied zum bisherigen Wire-Format. **Konsument (Wayfinder): kein Breaking Change** — vorhandene Decoder müssen I062/290 als variabel lang behandeln (bisher in der Praxis immer 2 Byte; robust implementiert wenn der Decoder `bytes.len()` prüft). Die ES-Age-Präsenz signalisiert „dieser Track hat mindestens einen ADS-B-Update erhalten" und kann von Wayfinder als ADS-B-Badge genutzt werden (AP9.9). |
| 2.3.0 | 2026-06-15 | **Additiv (ADR 0018).** Neue Kategorie **CAT065** (SDPS Service Status, „Heartbeat") auf **derselben** Multicast-Gruppe/Port wie CAT062. Periodische SDPS-Status-Meldung (I065/000 = 1) mit I065/010, I065/000, I065/015, I065/030 (Time of Day), I065/040 (NOGO operationell/degradiert). Wall-clock-getaktet (Default 1 s, `FIREFLY_CAT065_PERIOD`). **Konsument muss am führenden CAT-Oktett dispatchen** (`0x3E` → Track, `0x41` → Status) und unbekannte Kategorien überspringen — die robuste-Decoder-Regel verlangte das ohnehin. Kein Eingriff in das CAT062-Record-Format. Details: Abschnitt 8. |
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
| Framing | **Ein Datagramm = ein vollständiger ASTERIX-Datenblock einer Kategorie.** Für CAT062 ist das ein Scan (Tracks); für CAT065 ein SDPS-Status (Heartbeat). Keine zusätzliche Anwendungs-Rahmung (keine Sequenznummern, keine Extra-Header). |
| Kategorien | **CAT062** (Tracks, `0x3E`), **CAT065** (Heartbeat, `0x41`, seit 2.3.0) **und CAT063** (Per-Sensor-Status, `0x3F`, seit 2.5.0) auf **derselben** Gruppe/Port. Der Empfänger **dispatcht am führenden CAT-Oktett**: `0x3E` (62) → Track-Datenblock (Abschnitt 2), `0x41` (65) → SDPS-Status (Abschnitt 8), `0x3F` (63) → Sensor-Status (Abschnitt 9). Unbekannte Kategorien werden verworfen, nicht als Fehler behandelt. |

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
Das Item ist **variabel lang**: PSR-Age ist immer vorhanden (1 primäres
Subfeld-Byte + 1 Byte); seit ICD 2.4.0 folgt optional das ES-Age, seit
ICD 2.6.0 (ADR 0027) zusätzlich SSR-, Mode-S- und FLARM-Age. Die Age-Oktette
folgen der **Bit-Priorität MSB→LSB** im Primary-Subfeld (PSR → SSR → MDS → ES
→ FLARM), je 1 Oktett.

| Primary-Subfield-Bit | Subfeld | Länge | Kodierung | Quelle | Seit |
|----------------------|---------|-------|-----------|--------|------|
| Bit 7 (`0x40`, "PSR") | PSR-Age (generisches Track-Update-Alter) | 1 Oktett (immer) | u8, LSB = 0,25 s | `SystemTrack.update_age` | 1.0.0 |
| Bit 6 (`0x20`, "SSR") | SSR-Age (Mode A/C) | 1 Oktett (nur wenn Bit gesetzt) | u8, LSB = 0,25 s | `SystemTrack.source_ages.ssr` | **2.6.0** |
| Bit 5 (`0x10`, "MDS") | Mode-S-Age | 1 Oktett (nur wenn Bit gesetzt) | u8, LSB = 0,25 s | `SystemTrack.source_ages.mode_s` | **2.6.0** |
| Bit 4 (`0x08`, "ES") | ES-Age (Extended Squitter / ADS-B) | 1 Oktett (nur wenn Bit gesetzt) | u8, LSB = 0,25 s | `SystemTrack.source_ages.adsb` (= `adsb_age_s`) | 2.4.0 |
| Bit 3 (`0x04`, "FLARM") | FLARM-Age (**Firefly-Vendor-Subfeld**) | 1 Oktett (nur wenn Bit gesetzt) | u8, LSB = 0,25 s | `SystemTrack.source_ages.flarm` | **2.6.0** |

Ein Age-Byte ist nur vorhanden, wenn das zugehörige Bit im Primary-Subfeld
gesetzt ist (d. h. der Track hat einen Treffer dieser Technologie erhalten).
Trägt ein Track nur PSR, ist das Item weiterhin 2 Byte lang (bisheriges
Wire-Format, keine Änderung für Radar-only-Tracks).

> **Firefly-Bit-Map (wichtig).** Fireflys I062/290-Bit-Belegung ist ein
> **dokumentiertes Subset**, das von der rohen EUROCONTROL-Erweiterungs-
> Reihenfolge abweicht (ES sitzt historisch auf `0x08`, dem Standard-ADS-Bit).
> Die ADR-0027-Subfelder bleiben in diesem dokumentierten Rahmen **additiv** auf
> freien Bits — `0x40`/`0x08` bleiben unverändert, damit der bestehende
> Wayfinder-Decoder nicht bricht. **FLARM** hat **kein** EUROCONTROL-Standard-
> Subfeld in I062/290; `0x04` ist ein **Firefly-Vendor-Subfeld** (ein toleranter
> Decoder darf es überspringen).

**Konsument**: I062/290 robust als variabel langes Item dekodieren — Länge und
Reihenfolge der Age-Oktette aus dem Primary-Subfeld bestimmen (MSB→LSB-Priorität,
ein Oktett je gesetztem Bit), **nicht** hardcoded auf eine feste Länge. Aus den
vorhandenen Age-Subfeldern leitet der Konsument die **Provenienz** ab (≥ 2
frische Technologien → „kombiniert"; sonst die dominante einzelne; PSR-only →
Primär) — das ersetzt die bisherige Frontend-Heuristik (Wayfinder
`provenance.js`). Die Präsenz des ES-Subfelds signalisiert weiterhin
„ADS-B-Anteil vorhanden"; FLARM wird damit erstmals sauber unterscheidbar.

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
  Projektion ist der **System-Referenzpunkt** (ADR 0021) — die *eine* Quelle,
  die zugleich der Tracking-Frame-Ursprung ist, sodass I062/100 stets kohärent
  mit der Track-Berechnung ist. Standardmäßig ist das die Mitte der
  Union-Bounding-Box der konfigurierten Quellen, überschreibbar über
  `FIREFLY_SYSTEM_REF_LAT/_LON`. (Der frühere Replay-Modus mit festem
  Szenen-Ursprung wurde entfernt — ADR 0030, ICD 2.6.1.)
  I062/105 (WGS-84) bleibt unabhängig davon die primäre, kontextfreie Position.
  **Hinweis:** Diese Klärung betrifft nur die *Semantik* des Referenzpunkts —
  das Wire-Format von I062/100 (24-Bit-Zweierkomplement, LSB 0,5 m) ist
  unverändert, daher keine ICD-Versionserhöhung.

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

## 8. CAT065 — SDPS Service Status (Heartbeat, seit 2.3.0)

**Zweck.** CAT065 ist der periodische **Lebenssignal-Strom** des
Datenverarbeitungssystems (SDPS). Er erlaubt dem Konsumenten, **„leerer
Himmel"** (gültig, keine Tracks) von **„toter Feed"** (nichts kommt mehr an) zu
unterscheiden — Grundlage für Staleness-Erkennung und ein Readiness-Signal
(ADR 0018). CAT065 läuft auf **derselben** Gruppe/Port wie CAT062; der Empfänger
erkennt es am CAT-Oktett `0x41` (65).

**Normative Referenz.** EUROCONTROL **SUR.ET1.ST05.2000-STD-13-01** („CAT065
SDPS Service Status Messages"). Wir senden ein bewusstes Subset (periodische
SDPS-Status-Meldung).

**Datenblock.**
```
[CAT = 0x41] [LEN: u16 BE] [Record]
```
- `CAT` = 1 Oktett, immer `0x41` (65).
- `LEN` = 2 Oktette, big-endian, Gesamtlänge inkl. 3-Oktett-Header.
- Ein Datagramm trägt **einen** SDPS-Status-Record.

**Record (FSPEC/UAP).** Gleiche FSPEC-Mechanik wie CAT062 (Abschnitt 3). Die
periodische SDPS-Status-Meldung setzt die FRNs **{1, 2, 3, 4, 6}** → ein
einzelnes FSPEC-Oktett `0xF4`.

| FRN | Item | Länge | Inhalt |
|-----|------|-------|--------|
| 1 | I065/010 | 2 | Data Source Identifier (SAC/SIC). |
| 2 | I065/000 | 1 | Message Type. **`1` = SDPS Status** (der Heartbeat). |
| 3 | I065/015 | 1 | Service Identification (`FIREFLY_CAT065_SERVICE_ID`, Default 1). |
| 4 | I065/030 | 3 | Time of Day, 24-Bit, **1/128 s** seit UTC-Mitternacht (wie I062/070). **Wall-clock-Aussendezeit**, nicht Datenzeit. |
| 6 | I065/040 | 1 | SDPS Configuration & Status. **NOGO-Feld** (Bits 8/7): `00` = operationell (`0x00`), sonst degradiert (wir senden `0x40`). |

> FRN 5 (I065/020 Batch Number) und FRN 7 (I065/050 Service Status Report) sind
> Teil der CAT065-UAP, gehören aber zu anderen Message-Types und werden vom
> Heartbeat **nicht** gesendet. Ein Decoder soll sie tolerieren (1 Oktett je).

**Byte-genauer Referenz-Dump** (Service-ID 1, Mitternacht, operationell):
```
0x41 0x00 0x0C 0xF4 SAC SIC 0x01 0x01 0x00 0x00 0x00 0x00
```
(`LEN` = 12; FSPEC `0xF4`; I065/000 = `0x01`; I065/015 = `0x01`; I065/030 =
`00 00 00`; I065/040 = `0x00`.)

**Takt.** Wall-clock-periodisch, Default **1 s** (`FIREFLY_CAT065_PERIOD`).
Konsument-Empfehlung für Staleness: Feed als *stale* werten, wenn länger als
**~3 × Periode** kein Heartbeat ankam.

**Konfiguration (Sender).** `FIREFLY_CAT065_ENABLED` (Default `true`, greift nur
wenn der Feed via `FIREFLY_CAT062_ENABLED` läuft), `FIREFLY_CAT065_PERIOD`
(Sekunden), `FIREFLY_CAT065_SERVICE_ID`. SAC/SIC stammen aus den
`FIREFLY_CAT062_SAC`/`_SIC`.

## 7. Referenzen

- Referenz-Encoder CAT062: `crates/firefly-asterix/src/cat062.rs` (Firefly).
- Referenz-Encoder/Decoder CAT065: `crates/firefly-asterix/src/cat065.rs`
  (Firefly); byte-genauer Test `cat065::status_matches_reference_dump`.
- Byte-genauer CAT062-Referenz-Dump/Test: `single_track_matches_reference_dump`
  (Firefly, `firefly-asterix`).
- Referenz-Encoder/Decoder CAT063: `crates/firefly-asterix/src/cat063.rs`
  (Firefly); byte-genaue Tests `cat063::single_operational_sensor_matches_reference_dump`
  und `cat063::degraded_sensor_with_reason_appends_re_field` (I063/RE SRC-REASON).
- Architekturentscheidungen: Fireflys ADR 0006 (Integration/CAT062), ADR 0014
  (Produktions-Pivot, Wayfinder konsumiert CAT062/UDP), **ADR 0018 (CAT065
  Heartbeat)**, **ADR 0022 (CAT063 Sensor Status)**, **ADR 0032 (CAT063
  UAP-Standardisierung)**, **ADR 0033 (CAT063 SRC-REASON / I063/RE)**.
- Kurzfassung für Wayfinder: Wayfinders `CLAUDE.md` Abschnitt 2.

## 9. CAT063 — Sensor Status Messages (seit 2.5.0)

**Zweck.** CAT065 (Abschnitt 8) sagt „das **Datenverarbeitungssystem** (SDPS)
lebt und ist operationell". Es sagt aber **nichts** darüber, ob die einzelnen
**Sensoren** (Radare, ADS-B-Empfänger) noch Daten liefern. Fällt ein Radar aus,
läuft der Tracker (und damit der CAT065-Heartbeat) ungestört weiter — das
Lagebild wird nur in der Abdeckung dieses Sensors ärmer, ohne dass irgendein
Signal das anzeigt. CAT063 schließt diese Lücke: es ist der periodische
**Per-Sensor-Statusbericht** des SDPS und erlaubt dem Konsumenten, einen
**ausgefallenen Sensor** von einem **leeren Himmel** zu unterscheiden. Für
Wayfinder ist das die Grundlage des Sensor-Degradierungs-Banners (gelb).

> **Abgrenzung der drei Kategorien.** CAT062 = „*was* fliegt" (Tracks),
> CAT065 = „*lebt das SDPS*" (globaler Herzschlag), CAT063 = „*welche Sensoren
> speisen das SDPS*" (Per-Sensor-Liveness).

**Normative Referenz.** EUROCONTROL **SUR.ET1.ST05.2000-STD-04-01** („CAT063
Sensor Status Messages"), UAP-Positionen zusätzlich gegen die
CroatiaControl-ASTERIX-Referenzdefinition (CAT063, ed. 1.3) verifiziert. Wir
senden ein bewusstes Subset (periodischer Per-Sensor-Status).

> ⚠️ **UAP-Standardisierung ab 3.0.0 (ADR 0032, BREAKING).** Bis 2.6.1 nutzte
> CAT063 eine **kompaktierte, nicht-standardkonforme** UAP-Nummerierung
> (FRN 1 = I063/010 mit der *Sensor*-Identität, FRN 2 = I063/030, FRN 3 =
> I063/060, FSPEC `0xE0`). Ab 3.0.0 gelten die **echten EUROCONTROL-FRN-Slots**:
> I063/010 trägt die **SDPS**-Identität (FRN 1), die **Sensor**-Identität kommt
> im neuen I063/050 (FRN 4), I063/030 liegt auf FRN 3 und I063/060 auf FRN 5.
> FSPEC `0xB8`, Record 9 Oktette. Die folgende Beschreibung ist die **3.0.0**-Form.

**Datenblock.**
```
[CAT = 0x3F] [LEN: u16 BE] [Record]...
```
- `CAT` = 1 Oktett, immer `0x3F` (63).
- `LEN` = 2 Oktette, big-endian, Gesamtlänge inkl. 3-Oktett-Header.
- Danach folgen **mehrere Records ohne Trenner** — **einer je registriertem
  Sensor**. Jeder Record ist über sein FSPEC selbst-begrenzend (Abschnitt 3).
  Sind keine Sensoren registriert, trägt der Block null Records (nur Header).

**Record (FSPEC/UAP).** Gleiche FSPEC-Mechanik wie CAT062 (Abschnitt 3). Die
periodische Sensor-Status-Meldung setzt die FRNs **{1, 3, 4, 5}** → ein einzelnes
FSPEC-Oktett `0xB8`.

| FRN | Item | Länge | Inhalt |
|-----|------|-------|--------|
| 1 | I063/010 | 2 | Data Source Identifier (SAC/SIC) des **SDPS** — dieselbe Identität wie I062/010 und I065/010 (SAC = `FIREFLY_CAT062_SAC`, SIC = `FIREFLY_CAT062_SIC`, Default 25/2). Identifiziert **wer** meldet, nicht den einzelnen Sensor. |
| 3 | I063/030 | 3 | Time of Day, 24-Bit, **1/128 s** seit UTC-Mitternacht (wie I062/070). **Wall-clock-Aussendezeit**, nicht Datenzeit. |
| 4 | I063/050 | 2 | Sensor Identifier (SAC/SIC) des **Sensors**, über den dieser Record berichtet. SAC = `0` (Firefly-Konvention für lokale Sensoren); **SIC identifiziert den einzelnen Sensor** (die `sensor_id` der jeweiligen Quelle). |
| 5 | I063/060 | 1+ | Sensor Configuration & Status. Erstes Oktett: **CON-Feld** (Bits 8/7): `00` = operationell (`0x00`), `01` = degradiert (`0x40`), `10` = Initialisierung (`0x80`), `11` = nicht verbunden (`0xC0`). Bits 6–2 = PSR/SSR/MDS/ADS/MLT-GO/NOGO, Bit 1 = FX. Firefly sendet nur `0x00` (aktiv) oder `0x40` (kein Plot innerhalb `2.5 × scan_period`), FX clear. |

> **I063/010 vs I063/050.** In CAT063 identifiziert I063/010 das **meldende
> SDPS** (dieselbe SAC/SIC wie in CAT062/CAT065), I063/050 den **berichteten
> Sensor**. Der Konsument liest die Sensor-Identität aus **I063/050** (FRN 4).

> Weitere CAT063-UAP-Items (I063/015 Service Identification, I063/070–I063/092
> Zeit-/Positions-/Bias-Statistik, SP) gehören zu anderen Reports und werden
> vom periodischen Sensor-Status **nicht** gesendet. Ein Decoder soll ihre
> Präsenz-Bits tolerieren bzw. — wenn er sie nicht auswertet — ihre
> Längen-Regeln beachten (Vorwärtskompatibilität, Abschnitt 3).

**I063/RE — per-Quelle-Fehlergrund (`SRC-REASON`, seit 3.1.0, ADR 0033).** Bei
einem **degradierten** Sensor mit **bekanntem** Grund trägt der Record zusätzlich
das **Reserved Expansion Field** (FRN 13). Damit die FSPEC-Position FRN 13
markiert ist, wächst die FSPEC auf **zwei Oktette**: `0xB9 0x04` (FRN 1+3+4+5 +
FX, dann FRN 13). Das RE-Feld ist **selbst-begrenzend** über sein erstes Oktett
(Länge inkl. sich selbst):

```
[LEN = 0x03] [SUBFIELD_SPEC = 0x80] [SRC-REASON]
```

- `LEN` = Gesamtlänge des RE-Felds in Oktetten (hier 3).
- `SUBFIELD_SPEC` = Präsenz-Oktett im RE-eigenen Subfeld-Raum: Bit 8 (`0x80`)
  markiert `SRC-REASON` vorhanden; Bit 1 = FX (0).
- `SRC-REASON` (u8, Firefly-Vendor): `1 = unreachable` (Netz/Firewall — Credentials
  ok), `2 = auth` (HTTP 401/403 — falsche/fehlende Credentials), `3 = rate_limited`
  (HTTP 429). `0` (ok) wird **nie** gesendet — ein operationeller Sensor trägt
  kein RE-Feld.

**Sende-Regel.** Das RE-Feld erscheint **nur**, wenn der Sensor degradiert ist
**und** ein Grund bekannt ist. Ein operationeller Sensor bleibt die 9-Oktett-Form
(FSPEC `0xB8`); ein degradierter Sensor **ohne** bekannten Grund (z. B. eine still
gewordene FLARM-/Radar-Quelle — nur die HTTP-ADS-B-Poller OpenSky/adsb_aggregator
klassifizieren heute Gründe) trägt ebenfalls kein RE-Feld. Der Grund wird beim
nächsten erfolgreichen Plot **zurückgesetzt** (Sensor wieder erreichbar).

**Konsument.** Wer das RE-Feld nicht auswertet, **überspringt** es über sein
`LEN`-Oktett (Vorwärtskompatibilität, Abschnitt 3) — additiv, kein Wire-Bruch.
Wer es auswertet, liest `SUBFIELD_SPEC` und — bei gesetztem `0x80` — das
`SRC-REASON`-Oktett; ein unbekannter Grund-Code sollte tolerant als „degradiert,
Grund unbekannt" behandelt werden.

**Byte-genauer Referenz-Dump** (degradierter Sensor SIC = 1, Grund `unreachable`):
```
0x3F 0x00 0x10 0xB9 0x04 0x19 0x02 0x00 0x00 0x00 0x00 0x01 0x40 0x03 0x80 0x01
```
(`LEN` = 16; FSPEC `0xB9 0x04`; I063/010 = `19 02`; I063/030 = `00 00 00`;
I063/050 = `00 01`; I063/060 = `0x40` degradiert; I063/RE = `03 80 01`.)

**Byte-genauer Referenz-Dump** (SDPS 25/2, ein operationeller Sensor SIC = 1,
SAC = 0, Mitternacht):
```
0x3F 0x00 0x0C 0xB8 0x19 0x02 0x00 0x00 0x00 0x00 0x01 0x00
```
(`LEN` = 12; FSPEC `0xB8`; I063/010 = `19 02` (SDPS 25/2); I063/030 = `00 00 00`;
I063/050 = `00 01` (Sensor 0/1); I063/060 = `0x00`.)

**Zwei Sensoren in einem Block** (SIC 1 operationell, SIC 2 degradiert):
```
0x3F 0x00 0x15
0xB8 0x19 0x02 0x00 0x00 0x00 0x00 0x01 0x00    # Sensor 1 operationell
0xB8 0x19 0x02 0x00 0x00 0x00 0x00 0x02 0x40    # Sensor 2 degradiert (CON 0x40)
```
(`LEN` = 21 = 3 Header + 2 × 9 Record.)

**Takt.** Wall-clock-periodisch, Default **5 s** (`FIREFLY_CAT063_PERIOD`).
Langsamer als der CAT065-Heartbeat (1 s), weil Sensor-Liveness sich auf der
Zeitskala der Antennenumläufe (4–12 s) ändert, nicht im Sekundentakt.

**Degradiertes-Kriterium (Sender).** Ein Sensor gilt als **aktiv**, solange er
innerhalb von `2.5 × scan_period` Sekunden mindestens einen Plot geliefert hat
(`SensorHealthMonitor`); andernfalls **degradiert** (NOGO `0x40`). Die
Liveness wird aus dem echten Plot-Eingang des jeweiligen Quell-Adapters
(OpenSky/FLARM/Radar) abgeleitet (seit ADR 0030 der einzige Betrieb).

**Konfiguration (Sender).** `FIREFLY_CAT063_PERIOD` (Sekunden, Default 5).
CAT063 läuft mit, sobald **sowohl** der Feed (`FIREFLY_CAT062_ENABLED`) **als
auch** der Heartbeat (`FIREFLY_CAT065_ENABLED`, Default an) aktiv sind — es gibt
keinen eigenen Enable-Schalter, weil Per-Sensor-Status und Heartbeat denselben
Zweck (Feed-/Sensor-Liveness) bedienen. Die **SDPS**-Identität (I063/010) ist
`FIREFLY_CAT062_SAC`/`_SIC` (Default 25/2); die **Sensor**-Identität (I063/050)
hat SAC `0` (Firefly-Konvention für lokale Sensoren), die SICs sind die der
registrierten Sensoren.
