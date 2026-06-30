# ADR 0028 — Radar-ASTERIX-Eingangs-Adapter (`radar_asterix`, CAT048 über UDP)

- **Status:** akzeptiert
- **Datum:** 2026-06-30
- **Schnittstellen-relevant:** Eingangs-Kontrakt — `radar_asterix` wechselt von
  **reserviert → unterstützt**; `docs/source-input-contract.md` → **v1.3.0**
  (additiv: neue Radar-Felder). Der **Ausgabe**-Vertrag (CAT062/UDP) bleibt
  unberührt. **Kein** CAT062-ICD-Eingriff.
- **Auslöser:** Wayfinder-Issue [#35](https://github.com/manuelringwald/firefly/issues/35)
  (`from-wayfinder`) — die letzte im Quell-Kontrakt (ADR 0023) reservierte Quelle
  `radar_asterix` nachliefern. ADS-B (ADR 0019/0024) und FLARM (ADR 0026) sind erledigt.

## Kontext

Firefly trackt live (ADR 0020) aus einer env-getriebenen Quell-Liste (ADR 0023).
Zwei Adapter existieren — OpenSky-ADS-B und FLARM/OGN —, **beide geodätisch**
(`Measurement::Geodetic`, Selbstbericht). Es fehlt die **klassische Radar-Quelle**:
ein realer **Monoradar-Sensor** (PSR/SSR/Mode S), der seine Ziel-Meldungen als
**ASTERIX CAT048** („Monoradar Target Reports", SUR.ET1.ST05.2000-STD-04-01) über
das Netz schickt. Das ist die **eigentliche** Surveillance-Quelle eines ATC-Systems
— der Grund, warum Firefly ein Radar-*Tracker* ist.

**Fachliche Bedeutung.** CAT048 liefert Ziele in **Polar-Koordinaten** (RHO/THETA)
**relativ zum Radar-Standort** — anders als ADS-B/FLARM, die WGS84 selbst berichten.
Damit schließt sich der Kreis zum Simulator: dessen `radar.rs` erzeugt genau solche
Polar-Plots intern; `radar_asterix` ist der **reale** Eingang derselben Plot-Art.

**Sicherheits-Relevanz.** Der Decoder verarbeitet **ungeprüfte Netz-Datagramme**
(Charta §7 Wayfinder / §8 Firefly). Er muss **robust** sein: längen-/grenzen-geprüft,
**kein Panic** auf Eingabe, fehlerhafte Records verworfen statt Absturz, byte-genau
gegen Referenz-Vektoren getestet.

## Entscheidung

### 1. Transport: UDP-Listener (uni-/multicast), gespiegelt am Ausgabe-Receiver

Radar-/FEP-Feeds laufen typischerweise über **UDP** (oft Multicast). Der Adapter
bindet einen UDP-Socket auf eine konfigurierte Gruppe/Port (`listen`), liest
Datagramme und decodiert **ein Datagramm = ein CAT048-Datenblock**. Die Socket-
Mechanik spiegelt `firefly-multicast`s Empfänger (Multicast-Join, `recv`). Ein
toter Feed beendet den Server **nicht**; der Listener läuft weiter.

### 2. Decoder: CAT048-Subset, robust, mit vollständigem Längen-/Skip-Modell

Eigenes Modul `firefly-asterix::cat048` (neben `cat062`/`cat063`/`cat065`,
gemeinsame `fspec`-Mechanik). Es **decodiert die plot-relevanten Items** und
**überspringt** alle übrigen Standard-CAT048-Items längen-korrekt:

| FRN | Item | genutzt? |
|-----|------|----------|
| 1 | I048/010 SAC/SIC | ✓ Sensor-Zuordnung |
| 2 | I048/140 Time-of-Day (1/128 s) | ✓ Datenzeit |
| 3 | I048/020 Target Report Descriptor (TYP) | ✓ PSR/SSR/Mode-S → `DetectionKind`+`SourceKind` |
| 4 | I048/040 Measured Position Polar (RHO 1/256 NM, THETA 360/2¹⁶°) | ✓ Position |
| 5 | I048/070 Mode-3/A | ✓ Identität |
| 6 | I048/090 Flight Level (Mode C, 1/4 FL) | ✓ Vertikallage |
| 8 | I048/220 Aircraft Address (ICAO 24 Bit) | ✓ Identität |
| 9 | I048/240 Aircraft Identification (Callsign) | ✓ Identität |
| 11 | I048/161 Track Number | ✓ (informativ) |
| übrige | I048/130, /250, /042, /200, /170, /210, /030, /080, /100, /110, /120, /230, /260, /055, /050, /065, /060, /SP, /RE | **übersprungen** (längen-korrekt) |

Das Längen-Modell kennt jede Standard-Item-Form: **fixed**, **extended** (FX-Kette),
**repetitive** (REP-Zähler), **compound** (Primär-Subfeld + Subfelder) und
**explicit** (LEN-Oktett). Ein **unbekannter** (vokabular-fremder) FRN → `DecodeError`
(Record verworfen, nicht still fehl-geparst — wie `cat062`s `UnknownItem`).

> **Ehrliche Grenze.** Decodiert wird der **Monoradar-Target-Report-Kern**. Items
> wie Mode-S-MB-Daten (I048/250-Inhalt), Radial-Doppler (I048/120) o. ä. werden
> **strukturell übersprungen**, nicht interpretiert — Firefly braucht sie für die
> Plot-Bildung nicht. Erweiterbar, falls später nötig.

### 3. TYP → `DetectionKind` + `SourceKind` (Provenienz-treu, ADR 0027)

I048/020 Oktett 1, Bits 8–6 (TYP) klassifizieren die Detektion. Mapping:

| TYP | Bedeutung | `DetectionKind` | `SourceKind` |
|-----|-----------|-----------------|--------------|
| 001 | Single PSR | `Primary` | `Psr` |
| 010 | Single SSR | `Secondary` | `Ssr` |
| 011 | SSR+PSR (combined) | `Combined` | `Ssr` |
| 100/101 | Mode S All-/Roll-Call | `Secondary` | `ModeS` |
| 110/111 | Mode S + PSR | `Combined` | `ModeS` |
| 000 | No detection | Record verworfen | — |

Das speist die Per-Track-Provenienz (ADR 0027) **aus echtem Radar** korrekt: ein
SSR+PSR-Dwell bucht PSR **und** SSR.

### 4. Radar-Standort ist Konfiguration (CAT048 trägt ihn nicht)

CAT048-Target-Reports sind **polar relativ zum Radar** und enthalten **keinen**
Sensor-Standort. Firefly muss ihn kennen, um Polar → ENU → WGS84 zu heben. Daher
trägt eine `radar_asterix`-Quelle zusätzlich zu `sac`/`sic` die **Geodäsie des
Radar-Standorts** (`lat`, `lon`, `height_m`) und den **Listen-Endpoint**
(`listen` = `group:port`). Der Tracker registriert den Sensor mit diesem
Standort als `LocalFrame`; Polar-Plots werden wie die des Simulators verarbeitet
(FR-TRK-010, Mess-Fusion).

### 5. Adapter-Struktur wie OpenSky/FLARM (Ports & Adapters)

Eigenes Crate `firefly-radar`: `RadarConfig` (12-Factor, `FIREFLY_RADAR_*`, plus
`from-spec`-Bau aus `FIREFLY_SOURCES`), `target_report_to_plot` (DecodedReport +
Sensor-Standort → `firefly_core::Plot`, `Measurement::Polar`), und `run` (UDP-
Listener → `plots_tx`). Tracker-Kern und Ausgabe bleiben format-neutral.

### 6. Vokabular & Kontrakt

`radar_asterix` wird **unterstützt**: Pflicht `sac`/`sic`, `lat`/`lon` und
`listen`; optional `height_m`, `sensor_id`. `source-input-contract.md` → **v1.3.0**
(additiv: Radar-Standort + Listen-Endpoint).

## Umsetzungs-Häppchen (je für sich testbar, eigener Commit)

- **Schritt A** *(dieser ADR)* — Design ratifizieren. *Kein Code.*
- **Schritt B — `firefly-asterix::cat048`-Decoder:** Datenblock → `DecodedTargetReport`;
  robustes Längen-/Skip-Modell; byte-genaue Referenz-Vektoren + Trunkierungs-/
  Mutations-Tests (kein Panic). **Keine** Verdrahtung.
- **Schritt C — Crate `firefly-radar`:** `RadarConfig`, `target_report_to_plot`,
  UDP-`run`-Listener. Voll unit-getestet.
- **Schritt D — Kontrakt + Verdrahtung:** `sources.rs` mappt `radar_asterix` →
  `RadarConfig` (raus aus `skipped`); `main.rs` spawnt den Listener (Sensor-
  Registrierung mit Radar-Frame, Sensor-Health-Monitor, `firefly_radar_*`-Metrik);
  `source-input-contract.md` → v1.3.0; Anforderungs-Register/INSTALLATION/TECHNICAL;
  Cross-Project-Issue #35 schließen.

## Sicherheit & Robustheit

- **Kein Vertrauen ins Datagramm:** jede Länge wird geprüft, FX-/REP-/Compound-/
  Explicit-Strukturen grenzen-sicher; ein truncierter/korrupter Record wird verworfen,
  nicht gepanict. Mutations-/Trunkierungs-Tests sichern „kein Panic auf Eingabe".
- **Spoofing-Grenze (ehrlich):** ASTERIX-UDP ist **nicht authentifiziert**. Schutz
  ist Netz-/Quellen-Isolation (ADR 0017), nicht Krypto — wie beim CAT062-Ausgabepfad.
- **Verfügbarkeit:** ein toter Feed beendet den Server nicht; andere Quellen laufen weiter.

## Konsequenzen

- **Positiv:** die **klassische** Radar-Quelle wird erschlossen — Firefly wird zum
  realen Multi-Sensor-Tracker (PSR/SSR/Mode S **und** ADS-B/FLARM fusioniert);
  Charta-konformer Ports-&-Adapters-Eingang; schließt die letzte reservierte Quelle
  aus #35; speist die Provenienz (ADR 0027) mit echtem Radar.
- **Negativ / Grenzen:** Decoder deckt den Target-Report-**Kern** ab, nicht jedes
  CAT048-Item (dokumentiert, erweiterbar); Radar-Standort muss konfiguriert werden
  (CAT048 trägt ihn nicht); UDP-Quelle ungesichert (Spoofing-Grenze).

## Alternativen erwogen

- **Schwergewichtige ASTERIX-Fremd-Crate:** verworfen — ungeprüfte Angriffsfläche auf
  sicherheits-relevantem Eingangspfad; die `fspec`-Mechanik existiert bereits (cat062),
  ein fokussierter Decoder ist analysierbarer (Charta/ADR 0004).
- **Jedes CAT048-Item interpretieren:** verworfen — Firefly braucht nur den Target-
  Report-Kern für die Plot-Bildung; übrige Items strukturell überspringen genügt.
- **Radar-Standort aus dem Strom ableiten:** unmöglich — CAT048 trägt ihn nicht;
  Konfiguration ist der einzige korrekte Weg.
- **Unbekannte present-FRN tolerant überspringen:** verworfen — ohne Länge ist sicheres
  Überspringen unmöglich; `DecodeError` (Record verwerfen) ist die ehrliche, sichere Wahl.

## Querverweise

- Quell-Kontrakt (maßgeblich, versioniert): `docs/source-input-contract.md` (→ v1.3.0).
- ADR 0019 (OpenSky-Adapter), ADR 0026 (FLARM-Adapter), ADR 0023 (Quell-Kontrakt),
  ADR 0020 (Live-Tracker), ADR 0010 (Mess-Fusion), ADR 0027 (Provenienz), ADR 0017
  (Feed-Vertrauensgrenze).
- Normative Referenz: EUROCONTROL **SUR.ET1.ST05.2000-STD-04-01** (CAT048 Ed.).
- Cross-Project: `docs/cross-project/todo-for-wayfinder.md`; Issue #35.
