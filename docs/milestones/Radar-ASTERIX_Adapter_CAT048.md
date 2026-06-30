# Meilenstein — Radar-ASTERIX-Eingangs-Adapter (`radar_asterix`, CAT048)

> Dritter und **letzter** reservierter Live-Quell-Adapter aus Issue #35 (ADR 0028).
> Erschließt einen **realen Monoradar** über ASTERIX CAT048 — die *klassische*
> Surveillance-Quelle eines ATC-Systems — und macht Firefly damit zum echten
> Multi-Sensor-Tracker (PSR/SSR/Mode S fusioniert mit ADS-B/FLARM).

## Fachlichkeit — *warum*

ADS-B (OpenSky, ADR 0019) und FLARM (OGN, ADR 0026) sind **kooperative,
geodätische** Selbstberichte: das Luftfahrzeug sendet seine eigene WGS84-Position.
Die ursprüngliche Surveillance-Quelle eines Flugsicherungs-Systems ist aber das
**Radar**: es misst **polar** (Entfernung/Azimut) relativ zur Antenne und sendet
seine Ziel-Meldungen als **ASTERIX CAT048** ("Monoradar Target Reports"). Erst
damit ist Firefly das, was sein Name sagt — ein **Radar**-Tracker, der echte
Primär- und Sekundär-Radardaten verarbeitet und mit kooperativen Quellen fusioniert.

Das schließt den Kreis zum Simulator: dessen `radar.rs` erzeugt intern genau
solche Polar-Plots; `radar_asterix` ist der **reale** Eingang derselben Plot-Art.

## Technik — *wie*

Zwei Bausteine (Ports & Adapters, Tracker-Kern format-neutral):

### 1. CAT048-Decoder (`firefly-asterix::cat048`, FR-IO-005)

Neben den CAT062/063/065-Codecs, gemeinsame `fspec`-Mechanik. `decode_target_reports`
dekodiert je Record die **plot-relevanten** Items und **überspringt** alle übrigen
Standard-Items längen-korrekt:

| Item | Bedeutung | genutzt |
|------|-----------|---------|
| I048/010 | SAC/SIC | Sensor-Identität |
| I048/140 | Time-of-Day (1/128 s) | Datenzeit |
| I048/020 | Target Report Descriptor (TYP) | `Detection` → `DetectionKind`/`SourceKind` |
| I048/040 | Measured Position Polar (RHO 1/256 NM, THETA 360/2¹⁶°) | Position |
| I048/070 | Mode-3/A | Identität |
| I048/090 | Flight Level (Mode C, ¼ FL, signed 14-Bit) | Vertikallage |
| I048/220 | Aircraft Address (ICAO 24-Bit) | Identität |
| I048/240 | Aircraft Identification (Callsign) | Identität |
| I048/161 | Track Number | informativ |

Ein vollständiges Pro-FRN-**Format-Modell** (fixed / extended-FX / repetitive /
compound / explicit) erlaubt das längen-korrekte Überspringen aller übrigen Items
(I048/130, /250, /170, /200, /210, /120, …). Ein **unbekannter** FRN → harter
Fehler (kein stiller Fehl-Parse).

**Robustheit (untrusted-Eingangspfad).** Jede Lesung läuft über einen grenzen-
geprüften `Cursor`; ein truncierter/korrupter Record liefert `Cat048DecodeError`
(Datagramm verworfen). **Kein Panic auf Eingabe** — abgesichert durch byte-genaue
Referenz-Vektoren plus Trunkierungs- und Einzelbyte-Mutations-Tests.

**TYP → Provenienz (ADR 0027).** Der Target Report Descriptor klassifiziert die
Detektion; ein SSR+PSR-Dwell bucht im Tracker **PSR und SSR** — echte Radar-
Provenienz statt Heuristik.

### 2. Adapter-Crate (`firefly-radar`, FR-NET-013)

- `RadarConfig` (12-Factor, `FIREFLY_RADAR_*`) inkl. **Radar-Standort**
  `lat`/`lon`/`height_m` — CAT048 ist polar und trägt den Standort nicht.
- `target_report_to_plot`: `DecodedTargetReport` → `firefly_core::Plot`
  (`Measurement::Polar`, `DetectionKind`/`SourceKind` aus TYP, `ModeAC`). Kein
  Plot bei No-Detection / fehlender Position / fehlender Zeit.
- `run` / `datagram_to_plots`: UDP-Listener (Multicast-Join bei Gruppe, sonst
  Unicast-Bind), je Datagramm ein CAT048-Block → Plots.

### 3. Verdrahtung (`firefly-server`, Schritt D)

- `radar_config_from_spec`: `radar_asterix`-Eintrag aus `FIREFLY_SOURCES` →
  `RadarConfig` (`lat`/`lon` Pflicht, `listen`=`group:port`); Kontrakt **v1.3.0**.
- `build_live_tracker_multi`: Radar-Sensoren werden mit ihrem **eigenen
  Standort-Frame** + realem Polar-Fehlermodell registriert (statt des
  geodätischen Platzhalters), sodass Polar-Plots korrekt ins gemeinsame
  Tracking-Frame gehoben werden (FR-TRK-010).
- `spawn_radar_listener_live`: speist Plots in den Live-Tracker; Sensor-Health-
  Monitor (CAT063) und Metrik `firefly_radar_plots_received_total`.
- `representative_config` faltet den Radar-Standort (als Punkt) in die
  Frame-Union und die Scan-Periode in den Ausgabe-Takt.

## Schnittstellen-Wirkung

- **Eingangs-Kontrakt → v1.3.0** (additiv): `radar_asterix` von „reserviert" →
  „unterstützt"; neue Felder `lat`/`lon` (Pflicht), `height_m`?, `listen`?.
- **Ausgabe-Vertrag (CAT062/UDP) unberührt** — kein ICD-Eingriff.
- Schließt die **letzte** offene Quelle aus Wayfinder-Issue #35.

## Verifikation

| Ebene | Test |
|-------|------|
| CAT048-Decode (byte-genau) | `firefly-asterix`: `cat048::reference_block_decodes_all_fields`, `…::flight_level_decodes_negative_via_twos_complement` |
| TYP → Detection | `cat048::target_report_descriptor_typ_maps_to_detection` |
| Längen-korrektes Skippen | `cat048::skips_unused_standard_items_length_correctly`, `…::every_standard_frn_has_a_format` |
| Robustheit (kein Panic) | `cat048::truncations_never_panic`, `cat048::single_byte_mutations_never_panic`, `cat048::wrong_category_is_rejected`, `…::length_mismatch_is_rejected` |
| Plot-Mapping | `firefly-radar`: `plot::detection_classes_map_to_kind_and_source`, `plot::unmeasurable_reports_form_no_plot` |
| Listener (pure core) | `firefly-radar`: `listener::valid_datagram_yields_a_plot`, `…::garbage_datagram_yields_no_plots` |
| Config | `firefly-radar`: `config::*` |
| Verdrahtung | `firefly-server`: `sources::radar_config_maps_site_identity_and_listen`, `…::radar_missing_or_out_of_range_site_is_an_error`, `…::representative_covers_a_radar_only_feed` |

Rückverfolgbarkeit: **FR-IO-005** (CAT048-Decoder), **FR-NET-013** (Radar-Adapter).
ADR **0028**. Kontrakt `docs/source-input-contract.md` v1.3.0.
