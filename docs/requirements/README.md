# Anforderungs-Register & Rückverfolgbarkeit (Traceability)

> Dieses Verzeichnis ist die Wurzel der **Rückverfolgbarkeit**. Es ist der Kern
> der Zertifizierungs-Fähigkeit (siehe [ADR 0004](../decisions/0004-assurance-und-zertifizierungsfaehigkeit.md)).

## Warum das wichtig ist (fachlich)

Ein Audit nach ED-109A/DO-278A fragt im Kern immer dasselbe: *„Zeig mir für jede
Anforderung, wo sie umgesetzt ist — und wo bewiesen wird, dass sie erfüllt
ist."* Diese durchgehende Kette nennt man **Rückverfolgbarkeit**:

```
Anforderung  →  Design  →  Code  →  Test (Nachweis)
        und zurück (welcher Test deckt welche Anforderung?)
```

Wer das von Anfang an pflegt, hat später kein Drama. Wer es nachrüstet, baut die
Software faktisch neu. Deshalb fangen wir jetzt damit an — schlank, aber echt.

## Wie wir es konkret machen (technisch)

1. **Jede Anforderung bekommt eine stabile ID** in einer von drei Klassen:
   - `FR-…` **Functional Requirement** (was das System *tut*).
   - `NFR-…` **Non-Functional Requirement** (Eigenschaften: Cloud, Performance,
     Security, …).
   - `CON-…` **Constraint** (gesetzte Randbedingung, z. B. Normrahmen).
   IDs werden **nie wiederverwendet**. Wird etwas ungültig, Status → `verworfen`.

2. **Code und Tests verweisen auf die ID** per Tag-Kommentar, z. B.
   `// REQ: FR-GEO-001` an der umsetzenden Stelle bzw. im Test. So findet man per
   Volltextsuche jederzeit alle Berührungspunkte einer Anforderung.

3. **Die Tabelle unten ist die einzige Wahrheit** („single source of truth").
   Spalte *Nachweis* nennt den/die Test(s), die die Anforderung absichern.

Diese Konvention gilt **ab jetzt** für neuen Code. Bestehender M1-Code wird beim
nächsten Anfassen nachgezogen; seine Nachweise stehen bereits in der Tabelle.

## Anforderungs-Register

### Funktional (FR)

| ID | Anforderung | Status | Nachweis (Test) |
|----|-------------|--------|-----------------|
| FR-GEO-001 | Geodätische Umrechnung WGS84↔ECEF↔ENU↔Polar ist verlustfrei (Roundtrip-konsistent). | umgesetzt | `firefly-geo`: `wgs84_ecef_roundtrip`, `enu_geodetic_roundtrip`, `enu_polar_roundtrip` |
| FR-GEO-002 | Azimut wird von Nord im Uhrzeigersinn in [0,2π) geführt. | umgesetzt | `firefly-geo`: `azimuth_conventions` |
| FR-SIM-001 | Ziele folgen kinematischen Legs (Cruise/Turn/Climb/Accelerate) korrekt. | umgesetzt | `firefly-sim`: `cruise_*`, `quarter_turn_*`, `climb_*`, `acceleration_*` |
| FR-SIM-002 | Radar erzeugt Plots nur innerhalb von Reichweite und über dem tiefsten Strahl. | umgesetzt | `firefly-sim`: `target_disappears_after_script_ends` (Coverage), Demo |
| FR-SIM-003 | Erfassungswahrscheinlichkeit (Pd) wird statistisch korrekt angewandt. | umgesetzt | `firefly-sim`: `detection_probability_is_respected` |
| FR-SIM-004 | Messrauschen wird im polaren Sensorframe (Range/Azimut/Elevation) aufgeschlagen. | umgesetzt | Code-Review + `radar.rs`-Doku; quantitativer Test offen |
| FR-SIM-005 | SSR-fähige Ziele liefern kombinierte Plots mit Mode-3/A, Flugfläche, ICAO-Adresse. | umgesetzt | `firefly-sim`: `equipped_target_yields_combined_plots_with_ssr` |
| FR-SIM-006 | Der Plot-Strom ist nach Zeit geordnet. | umgesetzt | `firefly-sim`: `plots_are_time_ordered` |
| FR-TRK-001 | *(M2)* Der Tracker bildet aus Plots bestätigte Tracks (Gating, Assoziation, Filter, Lifecycle). | geplant | — |
| FR-TRK-002 | Polarer Plot wird in eine kartesische Messung mit Kovarianz umgerechnet (Converted Measurement via Jacobi-Matrix). | verifiziert | `firefly-track`: `position_matches_geo_conventions`, `elevation_projects_to_ground_range`, `covariance_is_cigar_shaped`, `cross_range_variance_grows_with_range`, `covariance_is_symmetric_and_positive_definite` |
| FR-TRK-003 | Kalman-Filter (Constant-Velocity) schätzt Position + Geschwindigkeit; Prädiktion vergrößert, Update verkleinert die Unsicherheit; glättet besser als die Rohmessung. | verifiziert | `firefly-track`: `predict_moves_position_and_grows_uncertainty`, `update_reduces_uncertainty`, `gain_respects_measurement_precision`, `covariance_stays_valid`, `tracking::filter_smooths_and_recovers_velocity` |

### Nicht-funktional (NFR)

| ID | Anforderung | Status | Nachweis |
|----|-------------|--------|----------|
| NFR-REPRO-001 | Gleicher Seed/Eingang ⇒ exakt gleicher Ausgang (Determinismus). | umgesetzt | `firefly-sim`: `reproducible_from_seed` |
| NFR-CLOUD-001 | Die Tracker-Kernlogik ist eine reine, deterministische Funktion (Zustand + Plots → Zustand + Tracks); Wanduhr/Netz/Logging bleiben außen. | geplant (M2) | — |
| NFR-CLOUD-002 | Verarbeitung erfolgt nach Datenzeit (ASTERIX Time-of-Day), nicht nach Server-Uhr. | geplant (M2) | — |
| NFR-CLOUD-003 | Track-Zustand ist serialisierbar (Snapshot) und damit wiederherstellbar/replizierbar. | geplant (M2) | — |
| NFR-OBS-001 | Strukturierte Logs, Metriken und Tracing sind vorhanden. | geplant (M3) | — |
| NFR-SAFE-001 | Kein `unsafe`-Code ohne dokumentierte Begründung. | umgesetzt | Clippy/Review-Gate (CLAUDE.md §5) |

### Randbedingungen (CON)

| ID | Randbedingung | Quelle |
|----|---------------|--------|
| CON-001 | Zielplattform: Kubernetes, anbieter-neutral (souverän/On-Prem-tauglich). | ADR 0003 |
| CON-002 | Assurance-Orientierung: ED-153 (SWAL) + ED-109A/DO-278A. | ADR 0004 |
| CON-003 | Eingabe-/Austauschformat: ASTERIX (CAT048/021/062). | ADR 0001 |
| CON-004 | Code Englisch, Doku/Erklärung Deutsch. | ADR 0002 |

## Statuswerte

`geplant` · `in Arbeit` · `umgesetzt` · `verifiziert` (umgesetzt **und** durch
Nachweis abgesichert) · `verworfen`.
