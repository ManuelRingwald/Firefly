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
| FR-TRK-001 | Der Tracker bildet aus dem Plot-Strom bestätigte Tracks (Gating, Assoziation, Filter, Lebenszyklus zusammengeführt). | verifiziert | `firefly-track`: `tracker::*`, `lifecycle::single_target_yields_one_confirmed_track`, `lifecycle::two_crossing_targets_keep_their_identities` |
| FR-TRK-002 | Polarer Plot wird in eine kartesische Messung mit Kovarianz umgerechnet (Converted Measurement via Jacobi-Matrix). | verifiziert | `firefly-track`: `position_matches_geo_conventions`, `elevation_projects_to_ground_range`, `covariance_is_cigar_shaped`, `cross_range_variance_grows_with_range`, `covariance_is_symmetric_and_positive_definite` |
| FR-TRK-003 | Kalman-Filter (Constant-Velocity) schätzt Position + Geschwindigkeit; Prädiktion vergrößert, Update verkleinert die Unsicherheit; glättet besser als die Rohmessung. | verifiziert | `firefly-track`: `predict_moves_position_and_grows_uncertainty`, `update_reduces_uncertainty`, `gain_respects_measurement_precision`, `covariance_stays_valid`, `tracking::filter_smooths_and_recovers_velocity` |
| FR-TRK-004 | Gating: ein Plot ist für einen Track plausibel, wenn seine quadrierte Mahalanobis-Distanz `d²=yᵀS⁻¹y` die χ²-Schwelle (2 DOF, `γ=−2·ln(1−P_G)`) nicht überschreitet; das Tor ist anisotrop (richtungsabhängig). | verifiziert | `firefly-track`: `threshold_matches_chi_squared_2dof`, `plot_on_prediction_is_accepted`, `gate_is_anisotropic`, `mahalanobis_scales_in_sigma` |
| FR-TRK-005 | Datenassoziation: global kostenminimale 1:1-Zuordnung Tracks↔Plots (Ungarische Methode) auf den gegateten Mahalanobis-Kosten; Reste werden als unzugeordnete Tracks/Plots zurückgegeben. | verifiziert | `firefly-track`: `hungarian_beats_greedy`, `hungarian_3x3`, `associate_matches_gated_plots`, `associate_leaves_ungated_plot_unassigned`, `associate_leaves_starved_track_unassigned`, `associate_handles_empty_inputs` |
| FR-TRK-006 | Track-Lebenszyklus: Geburt (tentativ), Bestätigung per M-aus-N, Coasting bei Fehldetektion, Löschung nach zu vielen Fehltreffern (getrennte Schwellen tentativ/bestätigt). | verifiziert | `firefly-track`: `track_is_born_tentative_then_confirmed`, `confirmed_track_coasts_then_dies`, `tentative_track_dies_quickly`, `separated_plots_make_two_tracks`, `lifecycle::*` |
| FR-TRK-007 | Güte-Metriken gegen Ground Truth: Positions-RMSE und Track-Kontinuität (Coverage + ID-Wechsel) als reine, wiederverwendbare Bausteine; gegen ein bekanntes Szenario nachgewiesen. | verifiziert | `firefly-track`: `metrics::rmse_is_root_mean_square`, `metrics::rmse_punishes_outliers`, `metrics::rmse_from_points_uses_euclidean_distance`, `metrics::continuity_counts_coverage_not_gaps_as_switches`, `metrics::continuity_counts_id_change_as_switch`, `metrics::continuity_first_assignment_is_not_a_switch`, `metrics::single_target_quality_meets_thresholds` |
| FR-TRK-008 | Der Tracker liefert den safety-relevanten Track-Status explizit aus (ADR 0008): Coasting-Indikator, Update-Alter (Datenzeit seit letztem Treffer) und Positions-Unsicherheit (1σ-Halbachse der Fehlerellipse aus `P`). Die Entscheidung fällt im Tracker; Adapter/ASD stellen nur dar. | verifiziert | `firefly-track`: `kalman::position_uncertainty_is_semi_major_one_sigma`, `tracker::system_tracks_report_coasting_age_and_uncertainty` |
| FR-IO-001 | Der Tracker-Ausgabestrom wird zu einem neutralen `Frame` (Datenzeit + Sensor + System-Tracks) gebündelt und verlustfrei nach JSON serialisiert/deserialisiert; erster Ausgabe-Adapter (ADR 0009). Die Wire-Form ist web-freundlich (Position in Grad, abgeleitete Geschwindigkeit/Kurs) und vom internen `SystemTrack`-Layout entkoppelt (Ports & Adapters). | verifiziert | `firefly-io`: `frame::wire_form_uses_degrees_and_derived_kinematics`, `frame::frame_round_trips_through_json`, `frame::json_is_self_describing`, `frame::empty_frame_has_no_tracks` |

### Nicht-funktional (NFR)

| ID | Anforderung | Status | Nachweis |
|----|-------------|--------|----------|
| NFR-REPRO-001 | Gleicher Seed/Eingang ⇒ exakt gleicher Ausgang (Determinismus). | umgesetzt | `firefly-sim`: `reproducible_from_seed` |
| NFR-CLOUD-001 | Die Tracker-Kernlogik ist eine reine, deterministische Funktion (Zustand + Plots → Zustand + Tracks); Wanduhr/Netz/Logging bleiben außen. | verifiziert | `firefly-track`: `snapshot::replay_is_deterministic` |
| NFR-CLOUD-002 | Verarbeitung erfolgt nach Datenzeit (`dt`/Zeit wird übergeben), nicht nach Server-Uhr. | verifiziert | `firefly-track`: `snapshot::replay_is_deterministic`; `process_scan(time, …)` |
| NFR-CLOUD-003 | Track-Zustand ist serialisierbar (Snapshot) und damit wiederherstellbar/replizierbar. | verifiziert | `firefly-track`: `snapshot::snapshot_roundtrip_recovers_state`, `snapshot::restored_snapshot_continues_equivalently` |
| NFR-CLOUD-004 | Robustheit gegen schwankende/verzögerte Scan-Intervalle: Tracks werden **nicht allein aufgrund von Zeitverzug** verworfen. Lebenszyklus-Entscheidungen (Coasting/Löschung) richten sich nach Datenzeit und konfigurierbaren Schwellen, nicht nach festen Wanduhr-Fristen. | verifiziert | `firefly-track`: `timing::long_gap_with_data_keeps_track_identity`, `timing::deletion_is_governed_by_miss_count_not_elapsed_time` |
| NFR-OBS-001 | Strukturierte Logs, Metriken und Tracing sind vorhanden. | geplant (M3) | — |
| NFR-OPS-001 | Einfache Vorführbarkeit: Der Tracker/eine Demo lässt sich lokal mit einem einzigen Befehl und ohne Programmierkenntnisse starten (Showcase/Präsentation). | geplant (M3) | — |
| NFR-SAFE-001 | Kein `unsafe`-Code ohne dokumentierte Begründung. | umgesetzt | Clippy/Review-Gate (CLAUDE.md §5) |
| NFR-INT-001 | Tracker-Kern ist format-/transport-neutral; Ausgabe erfolgt über einen neutralen `SystemTrack` + Adapter (Ports & Adapters). | verifiziert | `firefly-track`: `tracker::system_track_position_round_trips_through_wgs84`, `tracker::system_tracks_carry_confirmation_status` |
| NFR-INT-002 | Track-Positionen sind nach WGS84 zurückprojizierbar (geodätische Ausgabe); der Sensor-Frame wird zur Ausgabezeit übergeben (nicht im Zustand gehalten). | verifiziert | `firefly-track`: `tracker::system_track_position_round_trips_through_wgs84`; `firefly-core`: `system_track::ground_speed_is_vector_length`, `system_track::track_angle_follows_compass_convention` |

### Randbedingungen (CON)

| ID | Randbedingung | Quelle |
|----|---------------|--------|
| CON-001 | Zielplattform: Kubernetes, anbieter-neutral (souverän/On-Prem-tauglich). | ADR 0003 |
| CON-002 | Assurance-Orientierung: ED-153 (SWAL) + ED-109A/DO-278A. | ADR 0004 |
| CON-003 | Eingabe-/Austauschformat: ASTERIX (CAT048/021/062). | ADR 0001 |
| CON-004 | Code Englisch, Doku/Erklärung Deutsch. | ADR 0002 |
| CON-005 | Integrationsziel Phoenix WebInnovation (ASD/EFS); Track-Ausgabe als ASTERIX CAT062; Transport/Koordinatenbezug noch offen. | ADR 0006 |

## Statuswerte

`geplant` · `in Arbeit` · `umgesetzt` · `verifiziert` (umgesetzt **und** durch
Nachweis abgesichert) · `verworfen`.
