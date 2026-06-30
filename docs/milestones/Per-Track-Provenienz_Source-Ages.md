# Meilenstein — Per-Track-Provenienz & Source-Ages (ADR 0027, #30)

> Antwort auf Wayfinder-Issue #30 (`from-wayfinder`): Wayfinder klassifizierte die
> Track-Herkunft (◆ ADS-B / ▢ SSR / ○ PSR) bisher **heuristisch im Frontend**
> (`provenance.js`) und konnte **FLARM gar nicht** erkennen. Der Tracker ist der
> richtige Ort — er kennt die Fusion autoritativ. Diese Arbeit hebt die Herkunft
> aus der Heuristik in den **Ausgabe-Vertrag** (CAT062 I062/290) und den JSON-Pfad.

## Fachlichkeit — *warum*

Mit ADS-B (ADR 0019), FLARM (ADR 0026) und Radar (PSR/SSR/Mode S) fusioniert
Firefly inzwischen **mehrere Surveillance-Technologien** zu einem Track. Für den
Lotsen ist die **Herkunft** einer Spur sicherheitsrelevant: Ein primär-only-Track
(nur Radar-Echo, keine Identität) ist anders zu lesen als ein ADS-B-Selbstbericht
oder eine echte Mehrfach-Sensor-Fusion. Das ASD soll **ehrlich** „ADS-B", „SSR",
„Primär", „FLARM" oder „kombiniert" zeigen — nicht raten.

Zwei konkrete Probleme:

1. **ADS-B vs. FLARM ununterscheidbar.** Beide kamen als
   `Measurement::Geodetic` + `DetectionKind::Secondary` herein — der Tracker
   konnte sie nicht trennen.
2. **I062/290 trug nur PSR + ES.** Die übrige Technologie-Herkunft fehlte im
   Strom; Wayfinder musste raten.

## Technik — *wie*

### 1. `SourceKind` am Plot (`firefly-core`)

Neues Enum `SourceKind { Psr, Ssr, ModeS, AdsB, Flarm }` als Feld `Plot.source`.
Die **Produzenten setzen es explizit** (auditierbar, keine fragile Ableitung):

| Produzent | `source` |
|-----------|----------|
| Simulator PSR-only | `Psr` |
| Simulator SSR-Antwort (Mode 3/A) | `Ssr` |
| Simulator mit Mode-S-ICAO | `ModeS` |
| `firefly-opensky` | `AdsB` |
| `firefly-flarm` | `Flarm` |

`DetectionKind` (Primary/Secondary/Combined) bleibt die **Radar-Dwell-
Korrelation**; `SourceKind` ist die **Technologie-Familie**. Ein kombinierter
PSR+SSR-Dwell trägt `source = Ssr/ModeS` **und** `kind.has_primary()` — die
Treffer-Buchhaltung bucht dann **beide** Technologien.

### 2. Per-Technologie-Treffer-Buchhaltung im Track (`firefly-track`)

`Track` führt `SourceHits` (je PSR/SSR/Mode-S/ADS-B/FLARM eine letzte-Treffer-
Datenzeit, monoton). `record_source_hit(source, time, has_primary)` bucht beim
Einbuchen eines Treffers die Technologie (und PSR bei `has_primary`). Verdrahtet
an **allen drei** Treffer-Stellen in `fuse_simultaneous_plots`:

- **ICAO-Pre-Sort** (direkte Adress-Korrelation),
- **JPDA-Best** (kinematische Assoziation),
- **Track-Geburt** (founding plot).

> ⚠️ **Gefundener Bug.** Vor dieser Arbeit buchte nur die ICAO-Pre-Sort-Stelle
> Treffer; JPDA-Best und Track-Geburt fehlten — ein primär-only-Track hätte gar
> kein PSR-Alter geführt. Die neuen Tracker-Tests
> (`primary_only_track_records_psr_age_and_provenance`) deckten das auf.

### 3. `SystemTrack.source_ages` + abgeleitete `Provenance` (`firefly-core`)

`system_track_from` berechnet `source_ages: SourceAges` (je Technologie
`Option<f64>`-Alter = `(report_time − last_hit).max(0)`). `adsb_age_s` bleibt
**Alias** für `source_ages.adsb` (Back-Compat, treibt unverändert das ES-Subfeld).

`SystemTrack::provenance() -> Provenance { Unknown, Psr, Ssr, ModeS, AdsB, Flarm,
Combined }` **leitet ab**, statt ein Fremd-Feld zu erfinden:

- ≥ 2 Technologien **frisch** (Alter ≤ `PROVENANCE_FRESH_S` = 30 s) → `Combined`,
- sonst die einzelne frische Technologie,
- sonst die **jüngste** überhaupt gesehene,
- sonst `Unknown`.

### 4. CAT062 I062/290 — additive Alters-Subfelder (`firefly-asterix`)

Bestehend **unverändert:** PSR (`0x40`, generisches `update_age`), ES (`0x08`,
ADS-B). **Neu (additiv, freie Bits):**

| Bit | Subfeld | Quelle | Status |
|-----|---------|--------|--------|
| `0x40` | PSR-Age | `update_age` | bestehend |
| `0x20` | SSR-Age | `source_ages.ssr` | **neu** |
| `0x10` | Mode-S-Age (MDS) | `source_ages.mode_s` | **neu** |
| `0x08` | ES-Age (ADS-B) | `source_ages.adsb` | bestehend |
| `0x04` | FLARM-Age | `source_ages.flarm` | **neu, Firefly-Vendor-Subfeld** |

Age-Oktette folgen der Primary-Bit-Reihenfolge **MSB→LSB** (PSR, SSR, MDS, ES,
FLARM), je `u8`, LSB 0,25 s. Encoder hängt nur gesetzte Subfelder an; **PSR-only-
Tracks behalten das 2-Byte-Wire-Format** (byte-genauer Referenz-Dump stabil). Der
Decoder liest die Oktette **positionsbasiert** (tolerant gegen unbekannte Bits)
und füllt `DecodedRecord.source_ages`.

> **Ehrliche Grenze.** FLARM hat **kein** EUROCONTROL-I062/290-Standard-Subfeld;
> `0x04` ist ein **dokumentiertes Firefly-Vendor-Subfeld** (additiv, vom
> toleranten Decoder überspringbar). Fireflys I062/290 ist ohnehin eine
> dokumentierte Bit-Teilmenge (ES@`0x08`), kein roher EUROCONTROL-Abdruck —
> #30 bleibt in diesem Rahmen, ohne Wayfinders Decoder zu brechen.

### 5. JSON/Web-Pfad (`firefly-io`)

`FrameTrack` (web-freundliche Wire-Form) führt **explizit** `provenance`
(serialisiert `"psr"`/`"ssr"`/`"mode_s"`/`"adsb"`/`"flarm"`/`"combined"`/
`"unknown"`) und die rohen `source_ages` — der JSON-Pfad ist **kein** Draht-
Vertrag und darf die abgeleitete Herkunft direkt tragen. Das ASD-Showcase
rendert daraus die Herkunfts-Symbolik.

## Schnittstellen-Wirkung (Wayfinder)

- **ICD → v2.6.0** (additiv): I062/290-Tabelle um SSR/MDS/FLARM-Bits, Changelog,
  Encoder-Vektoren (Abschnitt 4.2).
- **Kein Breaking Change:** `0x40`/`0x08` unverändert; I062/290 war schon immer
  variabel-lang zu dekodieren.
- **Cross-Project-Issue** (`from-firefly`) an Wayfinder: Decoder liest die neuen
  Subfelder, ersetzt die `provenance.js`-Heuristik durch die abgeleitete
  Provenienz; **FLARM erstmals sauber unterscheidbar**.

## Verifikation

| Ebene | Test |
|-------|------|
| Provenienz-Ableitung | `firefly-core`: `system_track::provenance_*` (5 Fälle: Unknown, einzeln-frisch, kombiniert, stale-Fallback, Grenz-Alter) |
| Treffer-Buchhaltung (E2E) | `firefly-track`: `tracker::primary_only_track_records_psr_age_and_provenance`, `tracker::combined_mode_s_dwell_books_psr_and_mode_s_and_combines` |
| I062/290 Encode | `firefly-asterix`: `cat062::update_ages_appends_per_technology_subfields_in_priority_order` (byte-genau `0x7C …`) |
| I062/290 Round-Trip | `firefly-asterix`: `cat062::update_ages_per_technology_round_trip` |
| Reference-Dump-Stabilität | `cat062::single_track_matches_reference_dump` (unverändert grün — PSR-only-Pfad bit-stabil) |

Rückverfolgbarkeit: **FR-TRK-034** (Provenienz), erweitert **FR-IO-003** (CAT062)
und **FR-IO-001** (JSON-Pfad). ADR **0027**.
