# ADR 0027 — Per-Track-Provenienz & Source-Ages (CAT062 I062/290 additiv)

- **Status:** akzeptiert
- **Datum:** 2026-06-30
- **Schnittstellen-relevant:** **ja** — CAT062-Ausgabe-Vertrag. Erweitert I062/290
  um **per-Technologie-Alter** (SSR, Mode S, FLARM) **additiv**; `docs/ICD-CAT062.md`
  → **v2.6.0**. Kein Wire-Format-Bruch (bestehende PSR/ES-Subfelder unverändert).
- **Auslöser:** Wayfinder-Issue [#30](https://github.com/manuelringwald/firefly/issues/30)
  (`from-wayfinder`) — Wayfinder klassifiziert die Track-Herkunft (◆ ADS-B / ▢ SSR /
  ○ PSR) heute **heuristisch im Frontend** (`provenance.js`). FLARM ist so gar nicht
  erkennbar. Der Tracker ist der richtige Ort: er kennt die Fusion autoritativ.
- **Baut auf:** ADR 0026 (FLARM-Adapter; die ICAO-nur-bei-echtem-ICAO-Entscheidung
  war die Vorbereitung), ADR 0019/2.4.0 (I062/290 ES-Age), ADR 0010 (Mess-Fusion).

## Kontext

Mit ADS-B (ADR 0019), FLARM (ADR 0026) und Radar (PSR/SSR/Mode S) hat Firefly jetzt
**mehrere Surveillance-Technologien**, die zu einem fusionierten Track beitragen.
Der Track weiß, **welche** Sensoren im letzten Scan beitrugen (`contributing_sensors`,
ADR 0010) und seit wann ein ADS-B-Treffer vorliegt (`adsb_last_hit_time`, AP9.6).
Was fehlt, ist die **autoritative, per-Technologie aufgeschlüsselte Herkunft** im
Ausgabe-Strom — damit das ASD ehrlich „ADS-B", „SSR", „Primär" oder „FLARM" zeigen
kann, statt zu raten.

**Problem 1 — ADS-B vs. FLARM sind heute ununterscheidbar.** Beide sind
`Plot::adsb` → `Measurement::Geodetic` + `DetectionKind::Secondary`. Der Tracker
kann sie nicht trennen.

**Problem 2 — I062/290 trägt nur PSR + ES.** Das Item ist additiv erweiterbar, aber
Fireflys Bit-Belegung (PSR=`0x40`, ES=`0x08`) ist eine **dokumentierte Firefly-
Teilmenge**, die von der rohen EUROCONTROL-Erweiterungs-Reihenfolge abweicht. Ein
„standard-treues" Umsortieren von ES würde Wayfinders Decoder **brechen**.

## Entscheidung

### 1. `SourceKind` am Plot (firefly-core) — explizite Technologie-Herkunft

Neues Enum `SourceKind { Psr, Ssr, ModeS, AdsB, Flarm }` als Feld `Plot.source`.
Die **Produzenten setzen es explizit** (auditierbar, keine fragile Ableitung):

- Simulator: PSR-only → `Psr`; SSR-Antwort mit Mode-3/A → `Ssr`; mit Mode-S-ICAO → `ModeS`.
- `firefly-opensky` → `AdsB`. `firefly-flarm` → `Flarm`.

`DetectionKind` (Primary/Secondary/Combined) bleibt unverändert (es beschreibt die
**Radar-Dwell-Korrelation**); `SourceKind` beschreibt die **Technologie-Familie**.
Ein kombinierter PSR+SSR-Dwell trägt `source = Ssr/ModeS` **und** `kind.has_primary()`
— die Treffer-Buchhaltung (unten) bucht dann **beide** Technologien.

### 2. Per-Technologie-Alter im Track + `SystemTrack.source_ages`

Der Track führt je Technologie eine **letzte-Treffer-Datenzeit** (verallgemeinert das
bestehende `adsb_last_hit_time`). Beim Einbuchen eines Treffers:

- `kind.has_primary()` → PSR-Zeit aktualisieren.
- `match source`: `Ssr`→SSR, `ModeS`→Mode-S, `AdsB`→ADS-B, `Flarm`→FLARM.

`SystemTrack` bekommt `source_ages: SourceAges` (je `psr`/`ssr`/`mode_s`/`adsb`/`flarm`
ein `Option<f64>`-Alter, berechnet zur Berichtszeit). Das bestehende `adsb_age_s`
bleibt als **Alias** für `source_ages.adsb` erhalten (treibt unverändert das ES-Subfeld,
Back-Compat).

### 3. Provenienz wird **abgeleitet**, nicht als Fremd-Feld erfunden

`Provenance { Psr, Ssr, ModeS, AdsB, Flarm, Combined }` wird aus `source_ages`
**abgeleitet** (`SystemTrack::provenance()`): sind ≥ 2 verschiedene Technologien
**frisch** (Alter ≤ Frische-Fenster), ist es `Combined`; sonst die dominante einzelne
Technologie (Präzedenz kooperativ vor PSR). **Kein** non-standardes „provenance"-Item
auf dem CAT062-Draht — der Konsument leitet ◆▢○ aus den vorhandenen Alters-Subfeldern
ab (ASTERIX-Philosophie). Der JSON/Web-Pfad darf `provenance` + `source_ages` explizit
führen (kein Draht-Vertrag).

### 4. CAT062 I062/290 — additive Alters-Subfelder (Firefly-Bit-Map)

**Bestehend unverändert:** PSR=`0x40`, ES=`0x08`. **Neu (additiv, freie Bits):**

| Bit | Subfeld | Quelle | Status |
|-----|---------|--------|--------|
| `0x40` | PSR-Age | `source_ages.psr` / `update_age` | bestehend |
| `0x20` | SSR-Age | `source_ages.ssr` | **neu** |
| `0x10` | Mode-S-Age (MDS) | `source_ages.mode_s` | **neu** |
| `0x08` | ES-Age (ADS-B) | `source_ages.adsb` | bestehend |
| `0x04` | FLARM-Age | `source_ages.flarm` | **neu, Firefly-Vendor-Subfeld** |

Alters-Oktette folgen der Primary-Bit-Reihenfolge MSB→LSB (PSR, SSR, MDS, ES, FLARM),
je `u8`, LSB 0,25 s. Item bleibt **variabel lang** (Länge aus dem Primary-Subfeld);
der Decoder liest sie positionsbasiert (toleranter Decoder, ICD-Regel).

> **Ehrliche Grenze:** FLARM hat **kein** EUROCONTROL-I062/290-Standard-Subfeld
> (1090ES-Welt). Das FLARM-Age (`0x04`) ist ein **dokumentiertes Firefly-Vendor-
> Subfeld** — additiv, vom toleranten Decoder überspringbar. Fireflys I062/290 ist
> ohnehin schon eine dokumentierte Firefly-Bit-Teilmenge (ES@`0x08`), kein roher
> EUROCONTROL-Erweiterungs-Abdruck; #30 bleibt in diesem dokumentierten Rahmen.

### 5. ICD & Versionierung

`docs/ICD-CAT062.md` → **v2.6.0** (additiv): I062/290-Tabelle um SSR/MDS/FLARM-Bits,
Changelog, byte-genaue Encoder-Vektoren. Wayfinder-Folge-Issue (`from-firefly`):
Decoder liest die neuen Subfelder, ersetzt `provenance.js`-Heuristik durch die
abgeleitete Provenienz; FLARM wird erstmals sauber unterscheidbar.

## Umsetzungs-Häppchen

- **A** *(dieser ADR)* — Design. Kein Code.
- **B** — `firefly-core`: `SourceKind` + `Plot.source`; Produzenten (sim/opensky/flarm) setzen es.
- **C** — `firefly-track`: per-Technologie-Treffer-Buchhaltung; `SystemTrack.source_ages` + `Provenance`-Ableitung.
- **D** — `firefly-asterix`: I062/290-Encoder+Decoder um SSR/MDS/FLARM; byte-genaue Vektoren; ICD v2.6.0.
- **E** — `firefly-server`/JSON-Pfad-Durchreichung; Anforderungs-Register, INSTALLATION/TECHNICAL; Cross-Project-Issue an Wayfinder.

## Konsequenzen

- **Positiv:** autoritative, per-Technologie-Herkunft ersetzt Wayfinders Heuristik;
  FLARM wird unterscheidbar; ASTERIX-treu (Alter statt erfundenem Enum-Feld);
  strikt additiv (kein Wire-Bruch). Saubere Audit-Spur (explizite `SourceKind`).
- **Negativ / Grenzen:** `SourceKind` an *jedem* Plot berührt alle Produzenten
  (sim/opensky/flarm) — mechanisch, aber breit; `.ffplots`-Format gewinnt ein Feld
  (serde-`default` für Alt-Dateien). FLARM-Subfeld ist Firefly-spezifisch (ehrlich
  dokumentiert).

## Alternativen erwogen

- **`provenance`-Enum als CAT062-Special-Purpose-Item:** verworfen — non-standardes
  Wire-Feld; die Technologie-Alter (I062/290) tragen die Information ASTERIX-treu,
  der Konsument leitet die Provenienz ab.
- **ES-Bit auf die Standard-Position verschieben (standard-treu):** verworfen —
  bräche Wayfinders bestehenden Decoder; nicht additiv. Fireflys dokumentierte
  Bit-Map bleibt, neue Subfelder auf freien Bits.
- **ADS-B/FLARM aus `icao_address`-Präsenz ableiten:** verworfen — genau die
  Heuristik, die #30 ersetzen will (FLARM trägt teils ICAO-Adressen, ADS-B nicht immer).

## Querverweise

- ICD (maßgeblich, versioniert): `docs/ICD-CAT062.md` (→ v2.6.0).
- ADR 0026 (FLARM-Adapter), ADR 0019 (ADS-B/ES-Age), ADR 0010 (Mess-Fusion).
- Cross-Project: `docs/cross-project/todo-for-wayfinder.md`; Issue #30.
