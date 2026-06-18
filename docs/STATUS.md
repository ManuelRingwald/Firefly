# Arbeitsstand (Handover-Notiz)

> **Zweck:** Diese Datei ist der schnelle Wiedereinstieg — egal ob am PC oder
> Handy. Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

> 🗺️ **Roadmap:** Arbeitspakete, Findings und empfohlene Reihenfolge stehen in
> `docs/ROADMAP.md` (Stichwort „Roadmap" im Chat zeigt diese Liste).

- **Zuletzt aktualisiert:** 2026-06-18 — **AP9.3 (ICAO-Vorsortierung) abgeschlossen.**
  `fuse_simultaneous_plots` (der gemeinsame Kern beider Pfade: Batch + Async) erhält eine ICAO-Vorsortierstufe
  vor dem JPDA-Schritt. Plots mit bekannter ICAO-Adresse, die zu einem lebenden Track passen, werden direkt
  zugeordnet (β=1, kein Mahalanobis-Gate). Plots ohne Match gehen unverändert in den JPDA-Pool. Die
  gefrorene Referenz (ADR 0011) wird vor der Vorsortierung gebaut → Ghost-Suppression unberührt. Zwei neue
  Tests: `icao_match_bypasses_kinematic_gate` (Plot 111 km außerhalb des Gates → wird trotzdem assoziiert)
  und `icao_no_match_falls_through_to_jpda` (unbekannte ICAO → normaler JPDA-Initiierungspfad). FR-TRK-031.
  Alle Gates grün (`cargo test --workspace`, `clippy`, `fmt`). S4 · Opus 4.8. **Nächster Schritt:
  AP9.6 (`adsb_last_hit_time` auf Track + SystemTrack) oder AP9.5 (I062/290 ES-Age-Subfeld) —
  erst ankündigen, dann bauen.**
- **Vorherige Aktualisierung:** 2026-06-18 — **AP9.1 + AP9.2 (ADS-B-Eingang, Stufe 1) abgeschlossen.**
  `firefly-core::Measurement`-Enum eingeführt (`Polar(Polar)` + `Geodetic { position: Wgs84, sigma_pos_m: f64 }`);
  `Plot::adsb`-Konstruktor; alle sieben Aufrufstellen aktualisiert (Simulator, Player, Tracker-Batch- +
  Async-Pfad, Demo, Tracking-Test). `tracking_measurement` in `firefly-track::measurement` dispatcht
  auf die Enum-Variante: Polar-Pfad unverändert über `convert_plot` + `horizontal_from`; Geodetic-Pfad
  WGS84 → ENU direkt, isotrope Kovarianz `R = σ² · I₂`. 3 neue Unit-Tests (Ursprung/Isotopie,
  Nordrichtung, Sensor-Frame-Invarianz). FR-TRK-030 im Anforderungs-Register. Alle Gates grün
  (`cargo test --workspace`, `clippy`, `fmt`). S3 · Opus 4.8. **Nächster Schritt: AP9.3
  (ICAO-Vorsortierung im Tracker) — erst ankündigen, dann bauen.**
- **Vorherige Aktualisierung:** 2026-06-16 — **Paket #10 / SDPS-005 „Legal
  Recording & Replay" abgeschlossen.** Neues Crate `firefly-recorder` mit zwei
  Binaries: `firefly-record` (Sidecar, tritt Multicast-Gruppe bei, schreibt
  Datagramme mit Unix-ns-Zeitstempel in `.ffrec`-Datei) und `firefly-replay`
  (liest `.ffrec`, sendet Datagramme mit originalem Timing, skalierbar via
  `FIREFLY_REPLAY_SPEED`). Format-Bibliothek `lib.rs` mit Header-/Record-
  Schreib/Lese-API; 6 Unit-Tests (Round-Trip, Bad-Magic, Version-Check, EOF).
  FR-OPS-005 im Anforderungs-Register. Milestone `docs/milestones/SDPS-005_Legal_Recording_Replay.md`.
  ROADMAP aktualisiert. Gates: `cargo test/clippy/fmt` grün. S2 · Sonnet 4.6.
  Nächster Schritt: nächstes Roadmap-Paket nach Abstimmung.
- **Vorherige Aktualisierung:** 2026-06-16 — **Paket #11 / SDPS-006 „Erweiterte
  Observability" abgeschlossen.** SDPS-006a: `firefly_tracks_active` (Gauge) in
  `firefly_server::Metrics`; `firefly_multicast::run` auf generisches
  `run<F: Fn(usize)>` erweitert — `on_scan`-Callback wird nach jedem
  erfolgreichen Send mit `tracks.len()` aufgerufen; `spawn_cat062_multicast`
  in `main.rs` verdrahtet den Callback auf `metrics_scan.tracks_active.store()`.
  Alle 3 betroffenen Test-Calls (`sender.rs`, `receiver.rs`) mit no-op `|_| {}`
  ergänzt. `render()` exponiert `firefly_tracks_active` als Prometheus-Gauge.
  SDPS-006b: `monitoring/grafana/dashboard.json` — importierbares Grafana-
  Dashboard (Schema 38) mit 5 Panels: Tracks-Active-Stat/Zeitreihe,
  CAT062-Scan-Rate + CAT065-Heartbeat-Rate, WS-Clients-Stat,
  Sendefehler-Stat. Hinweis: Plots/s erst mit SDPS-001 sinnvoll.
  NFR-OBS-002 im Register. Milestone `docs/milestones/SDPS-006_Erweiterte_Observability.md`.
  Gates: `cargo test --workspace` grün, `cargo clippy` sauber, `cargo fmt` sauber.
  S2 · Sonnet 4.6. Nächster Schritt: nächstes Roadmap-Paket nach Abstimmung.
- **Vorherige Aktualisierung:** 2026-06-15 — Paket #3 „CAT065 Heartbeat" —
  **Firefly-Seite (Sender) fertig.** Neues Modul `firefly-asterix::cat065`:
  `Cat065Encoder` kodiert eine periodische SDPS-Status-Meldung (I065/000=1) mit
  I065/010 (SAC/SIC), I065/015 (Service-ID), I065/030 (Time of Day, 1/128 s),
  I065/040 (NOGO operationell/degradiert); `decode_status_block` als Umkehrung;
  byte-genauer Referenz-Dump-Test. `firefly-multicast`: `run_heartbeat`
  (wall-clock-getakteter, entkoppelter Sende-Task, Default 1 s) + Config
  (`FIREFLY_CAT065_ENABLED`/`_PERIOD`/`_SERVICE_ID`). `firefly-server`:
  `spawn_cat065_heartbeat` (eigener Socket, stempelt UTC-Tageszeit), Metrik
  `firefly_cat065_heartbeats_sent_total`. **Gleiche Multicast-Gruppe wie
  CAT062**, Dispatch am CAT-Oktett (Architektur-Entscheidung des
  Projektverantwortlichen). Doku: **ADR 0018**, ICD → **2.3.0** (additiv, §8),
  FR-IO-006 + FR-NET-003 im Register. Alle Gates grün (`cargo test/clippy/fmt`).
  **Wayfinder-Seite ebenfalls fertig** (CAT065-Decoder `pkg/cat065`,
  Receiver-Dispatch am CAT-Oktett, `pkg/health`-Staleness-Tracker,
  Frontend-Banner, `/ready`/`/metrics`-Integration) — **Paket #3 beidseitig
  abgeschlossen**, ROADMAP auf „erledigt". Nächster Schritt: nächstes
  Roadmap-Paket nach Abstimmung (z. B. #4 Konfigurierbarer
  System-Referenzpunkt).
- **Vorherige Aktualisierung:** 2026-06-15 — Paket #2 „Observability-Grundgerüst"
  **abgeschlossen** mit Häppchen 2.3: gemeinsamer `/metrics`-Endpoint
  (Prometheus-Textformat). Firefly-Teil: neues Modul
  `firefly-server::metrics` (`Metrics`-Struct mit Atomics,
  `ConnectedClientGuard`, `render()`); `/metrics`-Route im axum-Router.
  Exponiert: `firefly_scene_frames_total` (Gauge), `firefly_ws_clients_connected`
  (Gauge, via `ConnectedClientGuard` in `pump_frames`),
  `firefly_ws_clients_total` (Counter), `firefly_cat062_scans_sent_total` /
  `firefly_cat062_send_errors_total` (Counter, aus `spawn_cat062_multicast`
  nach `firefly_multicast::run`). NFR-OBS-001 aktualisiert (Metrik-Endpunkt
  nicht mehr offen). Neue Tests: `metrics::render_includes_all_metrics`,
  `metrics::connected_client_guard_tracks_lifetime`,
  `app::metrics_endpoint_exposes_frame_count`. Alle Gates grün
  (`cargo test/clippy/fmt`). Wayfinder-Teil (Paket #2.3, NFR-OBS-002):
  `pkg/metrics` (Prometheus-Rendering), `/metrics` auf Port `:8080` neben
  `/health`/`/ready` — Block-/Track-Zahlen, CAT062-Decode-Fehler
  (`Receiver.DecodeErrorCount`), aktuelle Track-Zahl, WS-Client-Count/Evictions
  (`Broadcaster.EvictedCount`). **Paket #2 vollständig erledigt.** Nächster
  Schritt: nächstes Roadmap-Paket nach Abstimmung mit dem
  Projektverantwortlichen (z. B. AP5/AP6 CAT065-Heartbeat oder
  Konfigurierbarer System-Referenzpunkt).
- **Vorherige Aktualisierung:** 2026-06-15 — Paket #2 „Observability-Grundgerüst",
  Häppchen 2.2: `tracing`-Instrumentierung in `firefly-multicast` (Wayfinders
  2.1 war bereits erledigt). Neue Abhängigkeit `tracing = "0.1"` (wie
  `firefly-server`). Sender (`lib.rs::run`): `tracing::debug!` pro gesendetem
  Scan (Zeit, Bytes, Track-Zahl, Ziel), `tracing::error!` bei Sendefehler vor
  Rückgabe des `io::Error`. Empfänger (`receiver.rs::run`): `tracing::debug!`
  pro empfangenem Block (Record-Zahl), `tracing::warn!` bei Socket-/Decode-
  Fehler vor Rückgabe des `ReceiveError`. `firefly-asterix` unverändert
  (Encoder ist infallibel, Decode-Fehler bereits typisiert). NFR-OBS-001
  ergänzt. Alle Gates grün (`cargo test/clippy/fmt`). Nächster Schritt:
  Häppchen 2.3 — gemeinsamer `/metrics`-Endpoint (Prometheus), nach
  Abstimmung mit dem Projektverantwortlichen.
- **Vorherige Aktualisierung:** 2026-06-15 — Paket #1 „Multicast-Feed-Sicherheit",
  Häppchen 1.1: **ADR 0017 „Vertrauensgrenze des CAT062-Multicast-Feeds"**
  erstellt (`docs/decisions/0017-multicast-feed-vertrauensgrenze.md`).
  Entscheidung: Vertrauensgrenze liegt auf der **Netzwerk-Schicht** (dediziertes
  isoliertes Segment/VLAN für Firefly-Sender + autorisierte ASD-Empfänger), nicht
  im CAT062-Anwendungsprotokoll — kein anwendungsseitiges Signieren/Verschlüsseln
  von CAT062 (würde ADR 0006 brechen). TTL=1 (`MulticastConfig`-Default) bleibt
  zusätzliche, aber nicht hinreichende Maßnahme. Diskutiert auch das durch ADR
  0016 neu entstandene Risiko eines gefälschten TSE-Bits (Track-Löschung durch
  Injection) — Schutz ist identisch mit allgemeinem Injektions-Schutz, keine
  TSE-spezifische Zusatzmaßnahme. Neue Anforderung **NFR-SEC-001** im Register
  (Status: dokumentiert, Umsetzung ist Deployment-Sache). Reine Doku, kein
  Code-Diff. Nächster Schritt: Häppchen 1.2 — Wayfinder-seitiges ADR-Pendant
  (Empfangspfad-Vertrauensgrenze + Browser-Rand-Entscheidung TLS/Auth).
- **Vorherige Aktualisierung:** 2026-06-15 (Branch `claude/tse-i062-080`, nach
  `main` gemergt — PRs #16 (Firefly) / #8 (Wayfinder):
  **TSE — CAT062 Track-Ende-Signalisierung über I062/080, ICD 2.2.0, additiv,
  ADR 0016.** AP7/AP8 (Callsign) waren bereits zuvor nach `main` gemergt — PRs
  #15 (Firefly) / #7 (Wayfinder).) Firefly-Teil **T1–T4 erledigt**: (T1) ADR 0016; (T2)
  `SystemTrack.ended: bool`, Lösch-Ereignis an beiden Löschstellen
  (`process_scan` + `process_plots`) via `delete_and_buffer_ended` eingefangen
  (voller letzter Zustand), `Tracker::take_ended_tracks()` draint für die
  Ausgabe — FR-TRK-029; (T3) `Player::periodic_snapshots` hängt Ende-Records
  einmalig an den Tick an (CAT062/Multicast-Pfad; JSON/`periodic_frames`
  unberührt); (T4) Encoder/Decoder setzen/lesen das **TSE-Bit (I062/080
  Oktett 2, Bit 7, `0x40`)**, ICD → 2.2.0, byte-genaue Tests. Additiv: kein
  FSPEC-Wachstum, Referenz-Dump unverändert. Ehrliche Grenze: im async-Pfad
  treibt Plot-Verkehr die Löschung — bei komplett stillem Feed (noch) kein TSE.
  Alle Gates grün (`cargo test/clippy/fmt`). Wayfinder-Teil **T5** (Decoder
  liest TSE und entfernt den Track sofort) ist ebenfalls erledigt — **TSE
  beidseitig abgeschlossen.** Nächster Schritt: siehe Arbeitspakete-Übersicht
  (Betriebs-Härtung / Multicast-Feed-Sicherheit / Observability).
- **Vorherige Aktualisierung:** 2026-06-15 (Branch `claude/callsign-i062-245`:
  **AP7 — CAT062 Target Identification I062/245 (Callsign), ICD 2.1.0,
  additiv.**) Neuer Typ `Callsign([u8; 8])` (`firefly-core`), durchgereicht von
  `ModeAC.callsign` über `Track::update_identity` (sticky, wie `mode_3a`/
  `flight_level_ft`) bis `SystemTrack.callsign` (FR-TRK-028). Encoder kodiert
  I062/245 (FRN 10, 7 Oktette: STI/spare + 8 × 6-Bit-IA-5) nur wenn vorhanden;
  FRN 10 liegt im bereits vorhandenen 2. FSPEC-Oktett → **additiv, kein
  Breaking Change** (ICD 1.x/2.0.0-Decoder bleiben gültig). Decoder
  (`decode_target_identification`) robust gegen Fremd-Codes (defensiv →
  Leerzeichen). Tests: `target_identification_packs_eight_six_bit_ia5_codes`,
  `decode_recovers_callsign_when_present`; Referenz-Dump-Test unverändert grün
  (empirischer Beleg für additiv). Alle 9 Frankfurt-Szene-Targets tragen jetzt
  Callsigns (`firefly-server::scene`). Doku: ICD → 2.1.0, FR-TRK-028 ergänzt,
  Milestone `M3X-cat062-encoder.md` (Nachtrag AP7). Alle Gates grün
  (`cargo test --workspace`, `clippy`, `fmt`). **Nächster Schritt: AP8
  (Wayfinder-Decoder für I062/245 nachziehen).**
- **Vorherige Aktualisierung:** 2026-06-15 (Branch `claude/callsign-i062-245`:
  Doku-Vorbereitung fürs Testen — `README.md`/`DOCKER.md` um einen Abschnitt
  „Zusammen mit Wayfinder testen (End-to-End-ASD)" ergänzt:
  `FIREFLY_CAT062_ENABLED=true` aktiviert den CAT062-Multicast-Feed, Hinweis
  auf `network_mode: host` (Multicast traversiert Docker-Bridge nicht).
  Wayfinder erhält im Gegenzug README/Dockerfile/docker-compose/DOCKER.md.)
- **Frühere Aktualisierung:** 2026-06-14 (Branch `claude/serene-heisenberg-xq4rla`:
  **AP1 — CAT062 Vertikallage I062/136 + UAP-Standardtreue, ADR 0015.**)
  Neues optionales Item **I062/136** (Measured Flight Level, FRN 17, signed
  i16, LSB 1/4 FL = 25 ft) als **Pass-through** der zuletzt gemessenen
  Mode-C-Höhe (`SystemTrack.flight_level_ft`, sticky wie Identität, kein
  vertikaler Filter; FR-TRK-027). Zugleich **I062/500 von FRN 16 → FRN 27**
  (echter EUROCONTROL-UAP-Slot; FRN 16 = I062/295 reserviert) → die Firefly-UAP
  ist jetzt ein **konformes Subset** der echten CAT062-UAP, lesbar von einem
  konformen Fremd-Decoder. **Breaking Wire-Change** (FSPEC 3→4 Oktette):
  **ICD → 2.0.0**, ADR 0015, Referenz-Dump neu berechnet
  (`[0x9F,0x0F,0x01,0x04]`, LEN 40). Encoder **und** Decoder in
  `firefly-asterix` umgesetzt; live gegen den Demo-Stream verifiziert (FRN 17 +
  27 gesetzt, FL374 dekodiert). **Wayfinder-Decoder muss in lockstep nachziehen
  (AP2)** — Cross-Project-Issue `from-firefly` offen. Alle Gates grün.
  ---
  **Frühere Arbeit dieser Sitzung (bereits in `main`): ADR 0013 vollständig
  umgesetzt (13.1–13.7 inkl. 13.5d).**
  `Tracker::process_plot` (async Pro-Plot-Verarbeitung) additiv,
  zeit-kontinuierlicher Lebenszyklus, `Tracker::snapshot_at(t)` (read-only
  Zeit-Projektion), der **periodische Ausgabetakt** im Player
  (`periodic_snapshots`/`periodic_frames`), **13.5a: gemeinsame Assoziation
  über nahezu gleichzeitige Plots** (`process_plots` + Simultaneitäts-Fenster +
  geteilter `fuse_simultaneous_plots`, FR-TRK-025), **13.5c: Kadenz-Boden im
  async-Lösch-Lebenszyklus** (FR-TRK-026), **13.6: azimut-abhängige
  Pro-Plot-Zeitstempel im Simulator** — Messung wird jetzt am eigenen
  `plot_time` (nicht am `scan_start`) neu ausgewertet, was den vermeintlichen
  13.5b-Kreuzungs-Tausch als Simulator-Bug auflöste (13.5b entfällt) —,
  **13.7: Frankfurt/Demo-Szene + Player auf den periodischen Ausgabetakt
  umgestellt** und **13.5d: Lösch-Kadenz-Boden auf konfigurierte
  `SensorModel::scan_period` umgestellt** (Option B, ARTAS-Sensor-
  Deklarations-Stil) statt der durch 13.6 verfälschten Online-Schätzung —
  `process_plots`/`should_delete_continuous` only, Batch-Pfad unverändert.
  `process_scan`/Batch verhaltensgleich, alle Gates grün.
  **Frankfurt Track-IDs 22 → 10 → 8 (Ziel erreicht).** Beide zuvor
  `#[ignore]`-markierten Frankfurt-Tests
  (`frankfurt_scene_keeps_one_identity_per_aircraft`,
  `frankfurt_crossing_pair_keeps_identity_through_the_crossing`) sind wieder
  grün; das Kreuzungs-Paar trägt jetzt die IDs 5/4 (statt 1/2 — Track-1 ist
  durchgehend `arrival_north`, kein Geister-Artefakt mehr).
- **Branch:** `claude/serene-heisenberg-xq4rla` wird per PR nach `main`
  gemergt (ADR 0013 13.1–13.7 inkl. 13.5d). Danach enthält `main` den
  vollständigen Stand (M1–M6 + Produktions-Phase bis ADR 0013); alle anderen
  Branches sind entweder bereits remote gelöscht oder können nach diesem
  Merge gelöscht werden — `main` ist die einzige verbleibende Branch.

> 🔁 **ADR 0014 (Pivot Produktion, Wayfinder konsumiert CAT062/UDP) — akzeptiert.**
> `CLAUDE.md` ist auf Produktionsbetrieb umgestellt (Modell-Angabe pro Schritt
> jetzt Pflicht). Cross-Project-Status: Issues **#6, #8, #10** (Pub/Sub-Fanout,
> Typ-Diskriminator, Schema-Versionierung) sind **geschlossen** — durch die
> CAT062-Architektur gegenstandslos. **#7** (Auth) ist **transformiert** auf
> Netz-Isolation des Multicast-Pfads + Wayfinder-Browser-Rand. **#9** (UTC
> Time-of-Day in I062/070) ist **erledigt und geschlossen** — Wayfinder hat
> M1 (CAT062-Pipeline + Live-Karte) abgeschlossen und den Vertrag gegen den
> Referenz-Dump verifiziert (`TestReferenceVector`), keine neuen
> Schnittstellen-Probleme. Das ADR-0013-Vorhaben (siehe nächster Absatz)
> bleibt der fachlich nächste Schritt.

> 🔭 **ADR 0013 (asynchrone Pro-Plot-Verarbeitung) — Umsetzung abgeschlossen.**
> Die Architektur-Entscheidung ist **angenommen** (`docs/decisions/0013-…md`).
> **13.1 + 13.2 sind umgesetzt:** `Tracker::process_plot` verarbeitet einen
> einzelnen Plot zu seiner eigenen Datenzeit (prädizieren → gegen Live-Schätzung
> gaten/assoziieren → updaten/initiieren → **zeit-kontinuierliche**
> Bestätigung/Löschung nach der *eigenen* Revisit-Kadenz jedes Tracks,
> `expected_revisit`/`should_delete_continuous`, ohne global geschätzte
> Feed-Kadenz), **additiv** neben `process_scan` (**Ansatz B**, mit dem
> Verantwortlichen abgestimmt). Grund für additiv statt sofortiger dünner
> Schleife: die Same-Time-Batch-Semantik (frozen reference + Joint-Association,
> ADR 0011) ist für die heutigen Tests tragend, solange der Simulator
> gleichzeitige Plots liefert (bis 13.5). FR-TRK-022/023, Tests
> `tracker::process_plot_*`, `track::expected_revisit_*`.
> **13.3 + 13.4 sind umgesetzt:** `Tracker::snapshot_at(t)` projiziert read-only
> **alle** Tracks auf eine gemeinsame Ausgabezeit `t`; darauf bauen
> `Player::periodic_snapshots`/`periodic_frames` den **festen Ausgabe-Herzschlag**
> (`t_out`, Default = kleinste Sensorperiode) — entkoppelt vom unregelmäßigen
> Eingang. FR-TRK-024 / FR-IO-005, Tests `tracker::snapshot_at_*`,
> `firefly-player::periodic_*`.
> **13.5 wurde re-dekomponiert (nach Befund).** Ein erster kombinierter Versuch
> (azimut-Zeiten + Frankfurt-Cutover) zeigte eine Architektur-Regression: naiver
> Pro-Plot-Pfad → Frankfurt **40 statt 8 IDs** + Kreuzungs-Identitäts-Tausch
> (Ursachen: Lösch-Churn ohne Kadenz-Boden **und** verlorene Geister-/Joint-JPDA-
> Logik bei *gleichzeitigen* Plots). Mit dem Verantwortlichen abgestimmt: **volle
> Async** (diese ADR-Option), weil reale SDPS (ARTAS & Co.) genau so arbeiten und
> der Batch-Pfad an der unrealistischen „alle Plots gleichzeitig"-Annahme hängt.
> **13.5a ist umgesetzt:** Simultaneitäts-Fenster (`SIMULTANEITY_WINDOW`, 0,5 s),
> `Tracker::process_plots` fasst koinzidente Plots zu *einer* Mess-Gelegenheit
> zusammen und assoziiert sie gemeinsam gegen eine eingefrorene Referenz
> (ADR-0011-Geisterunterdrückung + JPDA-Exklusivität, wie ein Scan); der Kern
> `fuse_simultaneous_plots` ist aus `process_scan` extrahiert und geteilt
> (`process_scan` verhaltensgleich, `process_plot` = Ein-Plot-Bequemlichkeit).
> FR-TRK-025, Tests `tracker::process_plots_*`.
> **13.5c ist umgesetzt:** Kadenz-Boden im async-Lösch-Lebenszyklus —
> `should_delete_continuous` löscht erst bei `budget · max(eigene Revisit,
> langsamste Sensorperiode)`, `process_plots` schätzt die Sensor-Perioden. Damit
> wird ein nur noch langsam (z. B. 12 s) abgedeckter Track nicht in der Lücke
> weggelöscht und neu geboren. FR-TRK-026, Test
> `tracker::process_plots_cadence_floor_survives_a_slow_sensor_gap`. **Messung
> (13.5a+13.5c im Player-Pfad): Frankfurt 40 → 22 IDs.**
> **13.5b entfällt** (Untersuchung ergab den 13.6-Simulator-Bug, kein
> Assoziationsproblem). **13.6 ist umgesetzt:** azimut-abhängige
> Pro-Plot-Zeitstempel im Simulator, Messung am eigenen `plot_time` neu
> ausgewertet; Frankfurt 22 → 10 IDs. **13.7 ist umgesetzt:** Frankfurt/Demo +
> Player auf `periodic_frames`/`periodic_snapshots` umgestellt.
> **13.5d ist umgesetzt:** der Kadenz-Boden in `should_delete_continuous`
> verwendet jetzt das **konfigurierte** `SensorModel::scan_period` (Maximum
> über alle Sensoren) statt der durch 13.6 verfälschten Online-Schätzung aus
> 13.5c (Option B, ARTAS-Sensor-Deklarations-Stil — abgestimmt mit dem
> Verantwortlichen). FR-TRK-026 aktualisiert, Test
> `tracker::process_plots_cadence_floor_survives_a_slow_sensor_gap` jetzt mit
> konfigurierten Perioden (2 s/12 s). **Messung: Frankfurt 22 → 10 → 8 IDs**
> (Ziel erreicht) — beide zuvor `#[ignore]`-markierten Frankfurt-Tests sind
> wieder grün. Damit ist ADR 0013 (13.1–13.7) vollständig umgesetzt. Details im
> Abschnitt *„Umsetzungsstand / Wiedereinstieg"* der ADR 0013. Nächster
> fachlicher Schritt: Betriebs-Härtung (§7 Charter) oder
> Multicast-Feed-Sicherheit — neuer Schritt wie immer erst abstimmen, dann
> bauen (CLAUDE.md §2).

- **Diese Sitzung (Aufräumen + Merge):**
  - Offener Branch endete mit unfertigem ADR-0013-WIP, der ein Qualitäts-Gate verletzte
    (`cargo test` rot). WIP per `git revert` zurückgenommen → **alle 32 Suites grün**,
    Clippy sauber, `cargo fmt` ok.
  - ADR 0013 von „in Entscheidung" auf **„akzeptiert — Umsetzung ausstehend"** gesetzt
    und um einen ausführlichen **Wiedereinstiegs-Abschnitt** (Foundation, Grund der
    Rücknahme, Häppchen-Plan 13.1–13.7, Anker-Commits) ergänzt.
  - Branch nach `main` gemergt und gepusht.
- **Abgeschlossen — M6.5 ✅ (Nachträge, gemergt zu `main`):**
  - **Server-seitige Roh-Plot-Geolokation:** `Player::frames()` rechnet jeden Plot über
    `Polar::to_enu()` + `LocalFrame::enu_to_geodetic()` (sensorbezogen, `TrackerConfig.sensors`)
    nach WGS84 um; neue `Tracker::config()`-Zugriffsmethode, `FramePlot` jetzt aus `firefly-io`
    re-exportiert. Das „raw plots"-Overlay zeigt jetzt echte Positionen statt einer leeren Liste.
  - **Coasting-Anzeige entflackert:** Bei mehreren Radaren mit `scan_offset` (ADR 0012) wurde der
    rohe Pro-Scan-`coasting`-Status sichtbar geflackert (Blau↔Orange), obwohl der Track insgesamt
    aktuell war. Frontend zeigt „coasting" jetzt erst, wenn `update_age_s` über
    `COAST_DISPLAY_THRESHOLD_S = 5.0` (eine Scan-Periode) liegt.
  - Doku: `docs/milestones/M6-showcase.md` (M6.3-Abschnitt ergänzt, „Ausblick" bereinigt).
  - **Verzug-Demo holt auf (Dead-Reckoning + Snap):** Server taktet jetzt **absolut**
    (`pacing::due_at`), sodass nach dem 5-s-Verzug die aufgelaufenen Frames durchlaufen und
    das Bild zur Gegenwart aufholt. Das Frontend extrapoliert die Tracks während der Lücke aus
    ihrer Geschwindigkeit und schnappt beim nächsten echten Frame ein. Die `delay_triggered`-
    Nachricht trägt jetzt `speed`. Track-Strom unangetastet (NFR-CLOUD-004).
  - **History-Trail (Kometenschweif):** Roh-Plots und vergangene Track-Positionen bleiben als
    verblassende Spur (Frontend), behebt auch das kurze Aufblitzen der Plots.
  - **Realistische, gemischte Scan-Perioden:** Frankfurt-Radare 4 s (Anflug) / 10 s / 12 s
    (En-Route) statt einheitlich 4 s; 8 stabile IDs bleiben (ADR 0012).
  - Glossar: Dead-Reckoning, History-Trail/Kometenschweif ergänzt.
  - **Alle Tests grün (32 Suites), Clippy sauber, `cargo fmt` ok, JS-Syntax geprüft.**
- **Live-Debugging (diese Sitzung, direkt auf `main`):** Beim ersten Start über Docker blieb die
  Karte leer — behoben: ungültige Font-Awesome-Glyphs-URL entfernt, Track-Label-Layer komplett auf
  HTML-Marker (`maplibregl.Marker`) umgestellt (kein externer Glyphs-Server mehr nötig).
- **PR:** Entwicklung abgeschlossen — PR erstellt und zu `main` gemergt (55 Commits,
  Meilensteine M1–M6 vollständig).

---

## 1. Wo wir gerade stehen

- **M1 (Simulator) ist fertig** und gepusht: Workspace + drei Crates
  (`firefly-geo`, `firefly-core`, `firefly-sim`).
- **M2 läuft:** Häppchen **2.1–2.8 erledigt (M2 abgeschlossen)** — Crate `firefly-track` mit
  Converted-Measurement, Kalman-Filter (CV, Joseph-Form), Gating (Mahalanobis/χ²),
  Datenassoziation (GNN/Ungarische Methode) und **Track-Lebenszyklus** (`Tracker`,
  Pro-Scan-Orchestrierung: Geburt/Bestätigung/Coasting/Löschung). Der
  Single-Radar-Tracker steht — inkl. End-to-End-Test mit zwei kreuzenden Zielen.
  **2.6**: serialisierbarer Zustand mit Snapshot/Replay (serde, ADR 0007).
  **2.7**: neutraler `SystemTrack`-Output in WGS84 (`firefly-core`) + Projektion
  `Tracker::system_tracks(&LocalFrame)` — der ASD-Port Richtung CAT062.
  **2.8**: Güte-Metriken (`Rmse`, `TrackContinuity`) gegen Ground Truth; E2E-Test
  mit Positions-RMSE < 40 m, 0 ID-Wechsel, Coverage > 90 %.
  **Nachtrag (ADR 0008, FR-TRK-008):** der `SystemTrack` trägt jetzt den
  safety-relevanten Status — `coasting`, `update_age`, `position_uncertainty`
  (1σ-Halbachse aus `P`) → bereitet CAT062 I062/080, /290, /500 vor.
  **Härtung (NFR-CLOUD-004):** `tests/timing.rs` beweist — lange Scan-Lücke mit
  Daten erhält Identität; Löschung nach Fehltreffer-*Anzahl*, nicht nach Zeit.
  Externe Abhängigkeiten `nalgebra` (ADR 0005), `serde` (ADR 0007).
- Die **Arbeitsregeln** stehen (`CLAUDE.md`): *erst erklären, dann bauen*;
  keine unerklärten Begriffe; Doku ist Teil der Leistung.
- **M3 läuft:** Häppchen **3.0–3.4 erledigt.**
  **3.0**: Architektur-Entscheidung (ADR 0009): Async-Server **Tokio + axum**,
  Transport **WebSocket**, erster Ausgabe-Adapter **JSON**, Karten-Frontend
  **MapLibre GL** (GPU-Vektorkarte, anbieter-neutral, mit Blick auf M4).
  **3.1**: neue Crate **`firefly-io`** mit dem neutralen `Frame` (Datenzeit +
  Sensor + `SystemTrack[]`) und `FrameTrack`; web-freundliche Drahtform (Position
  in **Grad**, abgeleitete Geschwindigkeit/Kurs), verlustfreier JSON-Roundtrip
  (FR-IO-001).
  **3.2**: neue Crate **`firefly-player`** — `Player::new(&scenario, config)` +
  `.frames()` führt das Szenario (M1) durch den Tracker (M2) und erzeugt den
  **Frame-Strom** (ein `Frame` je Scan-Zeit) über `firefly-io` (FR-IO-002). Rein
  und deterministisch.
  **3.3**: neue Crate **`firefly-server`** (axum/Tokio) streamt den Frame-Strom
  über **WebSocket** (`/ws`) an den Browser, getaktet nach Datenzeit am
  Ausgabe-Rand (Tempo-Faktor `FIREFLY_SPEED`); dazu Health-/Readiness-Probes,
  12-Factor-Config (`FIREFLY_PORT`), geordnetes Shutdown (SIGTERM) und
  strukturierte `tracing`-Logs (FR-NET-001, NFR-OBS-001). Start mit **einem
  Befehl**: `cargo run -p firefly-server` → `http://localhost:8080`.
  **3.4**: **MapLibre-Frontend** (`crates/firefly-server/static/index.html`, ins
  Binary eingebettet) — 2D-Karte (Stil `demotiles.maplibre.org`), die den
  `/ws`-Strom konsumiert und je Frame die Tracks zeichnet. Safety-Status sichtbar
  (ADR 0008): Farbe nach confirmed/tentativ/coasting, Unsicherheits-Ring
  (gestrichelt beim Coasting), Geschwindigkeitsvektor (FR-UI-001).
  **Bugfix (nach Sichtprüfung im Browser):** Die Track-IDs zählten hoch (statt
  zwei stabile Tracks gab es bis zu #21). Zwei Ursachen — beide behoben:
  *(1)* Die umgerechnete Mess-Kovarianz ignorierte das **Höhenwinkel-Rauschen**;
  bei 10–11 km Höhe streut das die Bodenentfernung um ~175 m, das Gate war viel
  zu eng → Plot fiel raus → Dublette. Jetzt fließt der Term
  `σ_ρ² = (cos φ·σ_r)² + (r·sin φ·σ_φ)²` ein (FR-TRK-002, `from_polar_deg`).
  *(2)* Das **Prozessrauschen** des CV-Filters war auf Geradeausflug getunt und
  zu klein für die 1°/s-Kurve → das kurvende Ziel zerbrach; Demo nutzt jetzt
  `accel_psd ≈ 60` (passt zum Manöver). Ergebnis: je Flugzeug **eine** stabile
  ID (Regressionstests `identity::*`, `scene::demo_scene_keeps_one_identity_*`).
  **3.5**: „Verzug"-Auslöser (NFR-OPS-001/NFR-CLOUD-004) — Knopf „Verzug
  simulieren (5 s)" im Frontend schickt `"delay"` über `/ws`; `pump_frames`
  bestätigt mit `delay_triggered` und pausiert die Zustellung um 5 s
  (`DELAY_TRIGGER_PAUSE`), rein an der Auslieferungs-Kante — der Frame-Strom
  selbst bleibt unverändert. Test
  `websocket::delay_trigger_pauses_delivery_without_corrupting_the_stream`.
  **M3 ist damit abgeschlossen** (`docs/milestones/M3-live-picture.md`).
- **Häppchen B (ADR 0006-Nachtrag) erledigt:** Transport- und
  Koordinatenfrage geklärt — **UDP-Multicast** + **System-Stereografisch**
  (CAT062 I062/100). Noch nicht umgesetzt; als Zielbild in ADR 0006
  festgehalten.
- **M4 ist abgeschlossen** (`docs/milestones/M4-multi-radar-fusion.md`):
  **4.1**: `Track` (firefly-track) merkt sich die SSR-Identität (Mode-3/A,
  ICAO-Adresse) aus zugeordneten Plots (`Track::update_identity`, sticky), und
  `SystemTrack` (firefly-core) führt sie als `mode_3a: Option<u16>` /
  `icao_address: Option<u32>` mit (FR-TRK-009).
  **4.0**: Architektur-Entscheidung **ADR 0010** — zentrale **Mess-Fusion**
  (Option A): ein Tracker, gemeinsamer Tracking-Frame, Plot-Umrechnung in
  diesen Frame (Position + Kovarianz), Pro-Sensor-Rauschmodell. Begründung:
  Präzision (Rohmessungen) bei gleicher Cloud-Tauglichkeit; Synergie mit dem
  System-Referenzpunkt der CAT062-Ausgabe (ADR 0006).
  **4.A.1**: `firefly-geo` bekommt die Frame-zu-Frame-Transformation für
  Position **und** Kovarianz (FR-GEO-003).
  **4.A.2**: `firefly-track` auf Multi-Sensor umgestellt — gemeinsamer
  `tracking_frame`, `BTreeMap<SensorId, SensorModel>`, sequenzielle
  Sensor-Verarbeitung löst das Geister-Problem (FR-TRK-010).
  **4.A.3**: Ende-zu-Ende-Test (zwei Radare, ein Flugzeug → ein Track,
  `firefly-player/tests/multi_radar.rs`).
  **4.A.4**: `SystemTrack.contributing_sensors` — welche Sensoren im letzten
  Scan beigetragen haben (nicht sticky, leer beim Coasten).
  **4.2**: CAT062-Identitätsfelder I062/060 (Mode 3/A) und I062/380/ADR
  (Mode-S-Adresse) — nur bei vorhandener Identität, automatisch im FSPEC.
- **Häppchen C.1 (ADR-0006-Nachtrag) erledigt:** `firefly-geo` bekommt
  `StereographicProjection` (FR-GEO-004) — konforme WGS84 ↔ System-
  Stereografisch-Projektion (konforme Breite + Gaußsche Hilfskugel +
  sphärische schiefachsige Stereografie, EUROCONTROL/ARTAS-Art).
- **Häppchen C.2 (ADR-0006-Nachtrag) erledigt:** `firefly-asterix` kodiert
  I062/100 (System-Stereografische Position, X/Y, FRN 6) zusätzlich zu
  I062/105 — Projektion über `StereographicProjection`, Referenzpunkt als
  Konstruktorparameter von `Cat062Encoder::new`.
- **Häppchen C.3 (ADR-0006-Nachtrag) erledigt:** neue Crate `firefly-multicast`
  versendet CAT062-Blöcke per UDP-Multicast (FR-NET-002), nach Datenzeit
  getaktet; 12-Factor-Config (`FIREFLY_CAT062_*`), per Default aus.
  `Player::scans()` als geteilte deterministische Basis beider Adapter; im
  Server-`main` als optionaler Task verdrahtet. **ADR 0006 (Transport &
  Koordinatenbezug) ist damit vollständig umgesetzt.**
- **Häppchen D.1 erledigt:** `firefly-asterix::decode_data_block()` dekodiert
  einen CAT062-Block zurück in `DecodedRecord`s (FR-IO-004) — FSPEC-Parsing
  (`fspec::parse`) + alle bisherigen Items außer der Rückprojektion von
  I062/100. Roundtrip-Tests gegen den eigenen Encoder.
- **Häppchen D.2 erledigt:** `unproject_cartesian_position` projiziert
  I062/100 (X/Y in Metern) über `StereographicProjection::unproject` zurück
  nach WGS84; stimmt mit der unabhängig kodierten I062/105-Position auf
  unter 1 m überein.
- **Häppchen D.3 erledigt:** `firefly-multicast::receiver` — `receiver_socket`
  tritt der Multicast-Gruppe bei, `recv_records`/`run` empfangen und
  dekodieren CAT062-Datagramme. Ende-zu-Ende-Test: Sender → Socket →
  Empfänger → Decoder liefert die ursprünglichen Track-Daten zurück.
- **M5 ist abgeschlossen** (`docs/milestones/M5-imm.md`): Häppchen
  **M5.1 – M5.4 (IMM)** —
  `firefly-track::motion` mit `MotionModel` (CV + Coordinated-Turn);
  `LinearKalman::predict_with` (FR-TRK-011). `firefly-track::imm` mit `Imm`:
  Filter-Bank + Markov-Übergangsmatrix, IMM-Mischung (FR-TRK-012) und der
  vollständige IMM-Zyklus mit Likelihood-Gewichtung (FR-TRK-013). Im Tracker
  wirksam (FR-TRK-014): jeder Track trägt eine IMM-Bank, Prädiktion/Update über
  `Imm::predict`/`Imm::update`, Gating über die kombinierte Schätzung.
  **M5.5 – M5.9 (JPDA)** — `firefly-track::pda` berechnet
  Assoziationswahrscheinlichkeiten `β` unter einem Clutter-Modell
  (FR-TRK-015); `LinearKalman::update_pda`/`Imm::update_pda` falten alle
  gegateten Plots gewichtet ein (FR-TRK-016/017); `firefly-track::jpda` löst
  die Exklusivität bei überlappenden Toren über Clustering + Ereignis-
  Aufzählung (FR-TRK-018); im Tracker ersetzt JPDA die harte GNN-Zuordnung je
  Sensor (FR-TRK-019), `TrackerConfig` trägt ein `ClutterModel`. E2E-Test:
  zwei eng benachbarte parallele Ziele bleiben zwei unterscheidbare,
  bestätigte Tracks („Track-Koaleszenz" als dokumentiertes JPDA-Merkmal).
- Qualität: **Alle Tests grün** (Workspace), Clippy sauber, `cargo fmt` ok. Sichtprüfung des
  Frontends im Browser ist ein manueller Schritt.
- **Dokumentation** aufgebaut: Glossar, M1-/M2-Erklärungen, ADRs 0001–0009,
  Anforderungs-Register mit Rückverfolgbarkeit.

## 2. Gesetzte Entscheidungen (Fundament, nicht mehr offen)

| Thema | Entscheidung | Quelle |
|-------|--------------|--------|
| Engine-Sprache | **Rust** (Frontend später JS) | ADR 0001 |
| Datenformat | **ASTERIX** (CAT048/021/062) | ADR 0001 |
| Erster Umfang | Simulator (M1) + Single-Radar-Tracker (M2) | ADR 0001 |
| Darstellung | **2D-Karte** | ADR 0001 |
| Sprache | Code Englisch, Doku/Chat Deutsch | ADR 0002 |
| Architektur | **Cloud-nativ**, Kubernetes, anbieter-neutral | ADR 0003 |
| Assurance | **Zertifizierungs-fähig**, ED-153 + ED-109A/DO-278A | ADR 0004 |
| Integration | Andocken an **Phoenix ASD**; Ausgabe **ASTERIX CAT062**; Kern neutral via Ports & Adapters | ADR 0006 |

## 3. Nächster Schritt (hier geht es weiter!)

✅ **M2 ist abgeschlossen** (inkl. Nachtrag: safety-relevanter `SystemTrack`-Status,
ADR 0008). Der Single-Radar-Tracker steht vollständig: Messung → Filter → Gate →
Zuordnung → Lebenszyklus → Snapshot/Replay → neutraler WGS84-Output mit
Safety-Status → Güte-Metriken.

✅ **Timing-Härtung (NFR-CLOUD-004) erledigt** — `tests/timing.rs` beweist beide
Eigenschaften. Damit ist M2 inkl. aller Veredelungen abgeschlossen.

✅ **M3 Häppchen 3.0–3.5 erledigt — M3 ist abgeschlossen.** ADR 0009 steht;
`firefly-io` → `Frame` als JSON (FR-IO-001); `firefly-player` →
deterministischer Frame-Strom (FR-IO-002); `firefly-server` streamt ihn live
über WebSocket (FR-NET-001); das **MapLibre-Frontend** zeigt die Tracks auf
einer 2D-Karte mit sichtbarem Safety-Status (FR-UI-001); ein „Verzug"-Knopf
macht die Timing-Robustheit (NFR-CLOUD-004) erlebbar (NFR-OPS-001). Komplette
Kette steht: ein Befehl → Live-Lagebild im Browser.
Meilenstein-Doku: `docs/milestones/M3-live-picture.md`.

✅ **Häppchen 3.X — CAT062-Encoder-Adapter abgeschlossen** (binäre
ASTERIX-Ausgabe neben JSON, ADR 0006):
- [x] **3.X.1** Crate `firefly-asterix`, Framing (CAT/LEN) + FSPEC/UAP-Mechanik
  + I062/010, /070, /040 (geometrie-frei). *S3 · Sonnet · Effort mittel*
- [x] **3.X.2** I062/105 (Position WGS84) + I062/185 (Geschwindigkeit kart.) mit
  Skalierungsfaktoren + Zweierkomplement. *S4 · Opus 4.8 · Effort hoch*
- [x] **3.X.3** I062/080 (Track-Status, variable Länge mit FX), I062/290, /500
  (Alter/Unsicherheit) — gegen EUROCONTROL SUR.ET1.ST05.2000-STD-09-01 Ed. 1.10
  verifiziert. *S4 · Opus 4.8 · Effort hoch*
- [x] **3.X.4** Adapter-Abschluss: Entscheidung gegen `Frame → CAT062`
  (ADR 0006-Nachtrag), Meilenstein-Doku `M3X-cat062-encoder.md`.
  *S3 · Sonnet · Effort mittel*

✅ **Häppchen B (ADR 0006-Nachtrag) erledigt:** Transport- und
Koordinatenfrage geklärt — **UDP-Multicast** + **System-Stereografisch**
(CAT062 I062/100). Noch nicht umgesetzt; Folge-Häppchen (Projektion +
I062/100-Encoder, Multicast-Versand) sind als Zielbild in ADR 0006
festgehalten und werden voraussichtlich im Umfeld von M4 eingeplant.

✅ **M4 ist abgeschlossen** (Häppchen 4.0 + 4.1 + 4.A.1–4.A.4 + 4.2,
Meilenstein-Doku `docs/milestones/M4-multi-radar-fusion.md`): SSR-Identität
durchgereicht (FR-TRK-009); Architektur entschieden — **zentrale Mess-Fusion**
(ADR 0010); Frame-Transform, Multi-Sensor-Tracker, E2E-Fusionstest,
Sensor-Provenienz und CAT062-Identitätskodierung alle umgesetzt.

✅ **Häppchen C.1 (ADR-0006-Nachtrag) erledigt:** `StereographicProjection` in
`firefly-geo` (FR-GEO-004) — konforme WGS84 ↔ System-Stereografisch-Projektion,
Referenzpunkt frei wählbar (für CAT062 später: `tracking_frame`-Ursprung).

✅ **Häppchen C.2 (ADR-0006-Nachtrag) erledigt:** I062/100-Encoder in
`firefly-asterix` — `Cat062Encoder::new(source, system_reference_point)`
projiziert jede Track-Position über `StereographicProjection` und kodiert
X/Y zusätzlich zu I062/105 (FRN 6, FSPEC-Bit 0x04 in Oktett 1).

✅ **Häppchen C.3 (ADR-0006-Nachtrag) erledigt:** neue Crate `firefly-multicast`
— `MulticastConfig` (12-Factor, `FIREFLY_CAT062_*`, Default aus),
`sender_socket()` + `run()` versenden je Scan einen CAT062-Block per
UDP-Multicast, nach Datenzeit getaktet (FR-NET-002). `Player::scans()` als
geteilte deterministische Basis; im Server-`main` als optionaler Task
verdrahtet. **Damit ist ADR 0006 (ASD-Integration, Transport &
Koordinatenbezug) vollständig umgesetzt.**

✅ **Häppchen D.1 erledigt:** CAT062-Decoder (`decode_data_block`,
`fspec::parse`, FR-IO-004) — Umkehrung des Encoders für FSPEC + alle Items
außer der I062/100-Rückprojektion.

✅ **Häppchen D.2 erledigt:** `unproject_cartesian_position` projiziert
I062/100 (X/Y in Metern) über `StereographicProjection::unproject`
(FR-GEO-004) zurück nach WGS84; stimmt mit der unabhängig kodierten
I062/105-Position auf unter 1 m überein.

✅ **Häppchen D.3 erledigt:** `firefly-multicast::receiver` —
`receiver_socket` bindet und tritt der Multicast-Gruppe bei, `recv_records`/
`run` empfangen und dekodieren CAT062-Datagramme. Ende-zu-Ende-Test:
Sender → Socket → Empfänger → Decoder liefert Kinematik, Status, Identität
und I062/100-Position originalgetreu zurück. **Häppchen D ist damit
abgeschlossen.**

✅ **Häppchen M5.1 erledigt:** zweites Bewegungsmodell. Neues Modul
`firefly-track::motion` mit `MotionModel` (`ConstantVelocity`,
`CoordinatedTurn { rate }`). Die CT-Übergangsmatrix dreht den
Geschwindigkeitsvektor um `ω·dt` und integriert den Bogen in die Position;
`ω → 0` ergibt exakt CV. `LinearKalman::predict_with(&MotionModel, …)` als Haken
für mehrere parallele Modelle (FR-TRK-011).

✅ **Häppchen M5.2 erledigt:** IMM-Mischungs-Schritt. `firefly-track::imm`
mit der Struktur `Imm` (Filter-Bank + Modellwahrscheinlichkeiten +
Markov-Übergangsmatrix). Implementiert die Interaktions-/Mischungs-Stufe:
`predicted_model_probabilities`, `mixing_probabilities`,
`mixed_initial_conditions` (gemischter Mittelwert + Kovarianz mit
Spread-of-the-Means-Term) — FR-TRK-012.

✅ **Häppchen M5.3 erledigt:** vollständiger IMM-Zyklus. `Imm::step(dt, Q,
Option<&measurement>)` — modellbedingtes Filtern aus der gemischten
Anfangsbedingung, Gauß-Likelihood je Modell
(`LinearKalman::measurement_likelihood`), Modellwahrscheinlichkeits-Update
`μ_j ∝ c_j·Λ_j` (Coasting → Markov-Prädiktion) und die kombinierte Schätzung
`combined_estimate`. Konvergenz nachgewiesen: CV gewinnt auf der Geraden, CT in
der Kurve (FR-TRK-013).

✅ **Häppchen M5.4 erledigt:** IMM im Tracker. Jeder `Track` trägt eine
IMM-Bank statt eines einzelnen `LinearKalman`; `ImmConfig` in `TrackerConfig`,
Prädiktion/Update über `Imm::predict`/`Imm::update`, Gating über
`Track::estimate`. Geradeausflug-Verhalten erhalten; kurvendes Ziel → passendes
CT-Modell dominiert (FR-TRK-014). **IMM-Teil von M5 abgeschlossen.**

✅ **Häppchen M5.5 erledigt:** PDA-Assoziationswahrscheinlichkeiten `β`. Neues
Modul `firefly-track::pda` mit `ClutterModel` und
`association_probabilities` — `β_0` (kein Treffer) und `β_j` je gegatetem
Plot, aus Likelihood vs. Clutter-Term `b` (FR-TRK-015).

✅ **Häppchen M5.6 erledigt:** PDA-gewichtetes Kalman-Update.
`LinearKalman::update_pda` faltet alle Hypothesen (kein Treffer + je Plot)
gewichtet ein, Spread-of-the-Means wie beim IMM-Mixing (FR-TRK-016).

✅ **Häppchen M5.7 erledigt:** PDA-gewichtetes IMM-Update. `Imm::update_pda`
überträgt M5.6 auf die ganze Modell-Bank (Zweig 0 = Bank nach `predict`,
Zweig `1+j` = `Imm::update(measurements[j])` auf Kopie der Bank), mischt
Modellzustände und -wahrscheinlichkeiten `β`-gewichtet (FR-TRK-017).

✅ **Häppchen M5.8 erledigt:** JPDA-Kern. Neues Modul `firefly-track::jpda`
mit `joint_association_probabilities` — Clustering (Union-Find) +
erschöpfende Aufzählung zulässiger gemeinsamer Zuordnungen je Cluster, daraus
marginalisierte `β_ij` mit Exklusivität (FR-TRK-018).

✅ **Häppchen M5.9 erledigt:** JPDA im Tracker. Die harte GNN-Zuordnung ist je
Sensor durch JPDA ersetzt; `Imm::update_pda` faltet alle gegateten Plots eines
Tracks gewichtet ein, der Plot mit größtem `β` liefert die Identität;
`TrackerConfig` trägt ein `ClutterModel` (~1 Falschplot/10 km², getunt gegen
RMSE-Regression). E2E-Test: zwei eng benachbarte parallele Ziele bleiben zwei
unterscheidbare, bestätigte Tracks — „Track-Koaleszenz" als dokumentiertes
JPDA-Merkmal (FR-TRK-019). **M5 (IMM + JPDA) ist damit vollständig
abgeschlossen.**

✅ **M6.1 — Frankfurt-Showcase-Szene erledigt** (`docs/milestones/M6-showcase.md`):
drei Radare, acht Flugzeuge (JPDA-Nahpaar, IMM-Manöver, SSR/primary-only,
Warteschleife, Multi-Radar-Überlappung), acht stabile Track-IDs über 240 s,
`FIREFLY_SCENE=frankfurt` zur Szenenauswahl (12-Factor).

✅ **M6 — Frontend-Showcase + Container ist abgeschlossen:**
- ✅ **M6.2** OpenStreetMap-Hintergrundkarte + Airspace-Overlay (GeoJSON, Layer-Toggle).
- ✅ **M6.3** Roh-Plot-Transparenz-Ebene (zeigt Radar-Plots vor Tracker-Verarbeitung).
- ✅ **M6.4** Docker-Containerisierung (Multi-Stage-Build, docker-compose, DOCKER.md).
- ✅ **M6.5** Nachträge: Server-seitige polar→WGS84-Konvertierung für `plots`,
  entflackerte Coasting-Anzeige, Verzug-Aufholen mit Dead-Reckoning,
  History-Trail (Kometenschweif) und realistische gemischte Scan-Perioden.

✅ **PROJEKT ABGESCHLOSSEN**

Alle Meilensteine (M1–M6) sind implementiert und zu `main` gemergt. Der Radar-Tracker steht end-to-end:
- **M1:** Simulator mit realistischen Szenen (Frankfurt: 3 Radare, 8 Flugzeuge)
- **M2:** Single-Radar-Tracker (Kalman, GNN, Gating, Lebenszyklus)
- **M3:** Live-Lagebild (WebSocket, MapLibre, CAT062-Kodierung)
- **M4:** Multi-Radar-Fusion (zentrale Mess-Fusion, SSR-Identität)
- **M5:** Manöver + dichter Verkehr (IMM, JPDA)
- **M6:** Showcase + Cloud (Docker, realistische Szenen, Dead-Reckoning, History-Trail)

Alle Qualitäts-Gates erfüllt: Tests ✅, Clippy ✅, Doku ✅, Cloud-native ✅, Zertifizierungsfähig ✅.

➡️ **Aktiver nächster Schritt — ADR 0013 umsetzen** (angenommen, Umsetzung
ausstehend): asynchrone Pro-Plot-Verarbeitung + periodischer Ausgabetakt. Der
vollständige Häppchen-Plan (13.1 `process_plot` → … → 13.7 CAT062 aus Snapshot)
und die Wiedereinstiegs-Anker stehen im Abschnitt *„Umsetzungsstand /
Wiedereinstieg"* von `docs/decisions/0013-…md`. Foundation-Commit `6a58a03`
(zurückgenommen via `0959059`).

➡️ **Weitere mögliche Fortsetzungen** (offene Punkte aus Abschnitt 5):
1. Live-OpenAIP-API-Integration statt statische Airspaces-GeoJSON.
2. Sensor-Registrierung / Bias-Korrektur (M4-Nachtrag, S5).
3. FHA / Hazard-Analyse (Sicherheit, S4).
4. Coverage-Werkzeug (Visualisierung, S3).
5. Out-of-Order-Eingang (Robustheit, S3).

### M5-Plan in Häppchen (abgeschlossen)

- [x] **M5.1** Zweites Bewegungsmodell: Coordinated-Turn-Übergangsmatrix neben CV, gemeinsamer 4-D-Zustand (FR-TRK-011) — *S4 · Opus 4.8 · Effort hoch*
- [x] **M5.2** IMM-Grundgerüst: Bank + Markov-Mischung der Modellzustände (FR-TRK-012) — *S5 · Opus 4.8 · Effort max*
- [x] **M5.3** Modellbedingtes Filtern + Modellwahrscheinlichkeits-Update (Likelihood je Modell) + kombinierte Schätzung (FR-TRK-013) — *S5 · Opus 4.8 · Effort max*
- [x] **M5.4** IMM in den `Tracker` einhängen (ersetzt den einzelnen `LinearKalman` je Track, FR-TRK-014) — *S4 · Opus 4.8 · Effort hoch*
- [x] **M5.5** PDA-Assoziationswahrscheinlichkeiten `β` + Clutter-Modell (FR-TRK-015) — *S4 · Opus 4.8 · Effort hoch*
- [x] **M5.6** PDA-gewichtetes Kalman-Update (FR-TRK-016) — *S4 · Opus 4.8 · Effort hoch*
- [x] **M5.7** PDA-gewichtetes IMM-Update (FR-TRK-017) — *S5 · Opus 4.8 · Effort max*
- [x] **M5.8** JPDA-Kern: Clustering + Ereignis-Aufzählung + Marginalisierung (FR-TRK-018) — *S5 · Opus 4.8 · Effort max*
- [x] **M5.9** JPDA im `Tracker` einhängen, ersetzt GNN je Sensor (FR-TRK-019) — *S4 · Opus 4.8 · Effort hoch*

### M4-Plan in Häppchen (Option A, ADR 0010)

- [x] **4.1** SSR-Identität bis zum `SystemTrack` (FR-TRK-009) — *S3 · Sonnet*
- [x] **4.0** Architektur-Entscheidung: zentrale Mess-Fusion (ADR 0010) — *S4 · Opus 4.8*
- [x] **4.A.1** `firefly-geo`: Frame-zu-Frame-Transformation (Position + Kovarianz, FR-GEO-003) — *S4 · Opus 4.8 · Effort hoch*
- [x] **4.A.2** `firefly-track` auf Multi-Sensor: gemeinsamer Tracking-Frame, Plot-Umrechnung + sequenzielle Fusion, Pro-Sensor-Rauschmodell (FR-TRK-010) — *S4–S5 · Opus 4.8 · Effort hoch*
- [x] **4.A.3** Multi-Radar-Szenario (zwei überlappende Radare) + E2E-Test: ein Flugzeug → **ein** Track (FR-TRK-010) — *S4 · Opus 4.8 · Effort hoch*
- [x] **4.A.4** Sensor-Provenienz im `SystemTrack` (welche Sensoren tragen bei, FR-TRK-010) — *S3 · Sonnet · Effort mittel*
- [x] **4.2** CAT062-Identitätsfelder kodieren (I062/060, I062/380/ADR; `firefly-asterix`, FR-IO-003/FR-TRK-009) — *S3–S4 · Opus 4.8 · Effort mittel–hoch*
- [ ] *(später)* Sensor-Registrierung / Bias-Korrektur — *S5 · Fable 5 / Opus 4.8*

Offen/optional: Sichtprüfung des Frontends (inkl. „Verzug"-Knopf) im Browser
durch den Projektverantwortlichen.

Erst Erklärung → Rückfragen/Go → dann kleine, testbare Umsetzung.

### Häppchen C — ADR-0006-Transport (System-Stereografisch + UDP-Multicast)

- [x] **C.1** `firefly-geo`: konforme WGS84 ↔ System-Stereografisch-Projektion
  (`StereographicProjection`, FR-GEO-004) — *S4 · Opus 4.8 · Effort hoch*
- [x] **C.2** I062/100-Encoder (`firefly-asterix`, X/Y 24-Bit Zweierkomplement,
  LSB 0,5 m, zusätzlich zu I062/105) — *S2–S3 · Sonnet · Effort niedrig–mittel*
- [x] **C.3** UDP-Multicast-Versand-Adapter (`firefly-multicast`, FR-NET-002;
  `Player::scans()` als geteilte Basis; im Server-`main` verdrahtet) — *S4 · Opus 4.8 · Effort hoch*

### Häppchen D — CAT062-Multicast-Empfänger/Recorder

- [x] **D.1** CAT062-Decoder (`decode_data_block`, `fspec::parse`, FR-IO-004) —
  FSPEC + alle Items außer I062/100-Rückprojektion — *S3 · Sonnet · Effort mittel*
- [x] **D.2** I062/100 → WGS84 zurückprojizieren (`StereographicProjection::unproject`,
  FR-GEO-004), Vergleich mit I062/105 auf unter 1 m — *S2 · Sonnet · Effort niedrig–mittel*
- [x] **D.3** Echter Multicast-Empfänger (Socket tritt Gruppe bei, empfängt,
  dekodiert; Loopback-Integrationstest analog C.3) — *S3 · Sonnet · Effort mittel*

## 4. M3-Plan in Häppchen (mit Komplexität / Modell)

- [x] **3.0** Architektur-Entscheidung (ADR 0009: Tokio/axum, WebSocket, JSON, MapLibre) — *S2 · Sonnet · Effort niedrig*
- [x] **3.1** JSON-Ausgabe-Adapter (`Frame` = Zeit + Sensor + `SystemTrack[]`, `serde_json`; Crate `firefly-io`, FR-IO-001) — *S2–S3 · Sonnet · Effort niedrig–mittel*
- [x] **3.2** „Player": Szenario → Tracker → Frame-Strom (reine Logik, Tempo getrennt vom Kern; Crate `firefly-player`, FR-IO-002) — *S3 · Sonnet · Effort mittel*
- [x] **3.3** WebSocket-Server (axum/tokio, Health/Readiness, 12-Factor, Shutdown, Logs/NFR-OBS-001; Crate `firefly-server`, FR-NET-001) — *S4 · Opus 4.8 / Fable 5 · Effort hoch*
- [x] **3.4** Frontend 2D-Karte mit Live-Tracks (MapLibre; coasting/Status farbig, Unsicherheits-Ring, Geschwindigkeitsvektor; `static/index.html`, FR-UI-001) — *S3 · Sonnet · Effort mittel*
- [x] **3.5** Demo-Erlebnis (ein Befehl, „Verzug"-Auslöser zeigt Timing-Robustheit) — *S3 · Sonnet · Effort mittel*
- [x] **3.X** CAT062-Encoder-Adapter (3.X.1–3.X.4, `firefly-asterix`, FR-IO-003) — *S4 · Opus 4.8 / Fable 5 · Effort hoch*

## 4b. M2-Plan in Häppchen (abgeschlossen)

- [x] **2.1** Converted Measurement (Plot → kartesisch + Kovarianz) — *S3 · Sonnet*
- [x] **2.2** Kalman-Filter (Constant-Velocity, Predict/Update) — *S4 · Opus*
- [x] **2.3** Gating (Mahalanobis-/χ²-Validierungsregion) — *S3 · Sonnet*
- [x] **2.4** Datenassoziation GNN (Ungarische Methode) — *S4 · Opus*
- [x] **2.5** Track-Lebenszyklus (M-aus-N, Bestätigung, Coasting, Löschung) — *S4 · Opus*
- [x] **2.6** Serialisierbarer Zustand (Snapshot/Replay) — *S3 · Sonnet · Effort mittel*
- [x] **2.7** Neutraler `SystemTrack`-Output in WGS84 (ASD-Port → CAT062) — *S3 · Sonnet · Effort mittel*
- [x] **2.8** Güte-Metriken gegen Ground Truth (RMSE, Track-Kontinuität) — *S3 · Sonnet · Effort mittel*
- [x] **Nachtrag** Safety-Status auf `SystemTrack` (ADR 0008, FR-TRK-008) — *S3 · Sonnet · Effort mittel*
- [x] **Härtung** Timing-Robustheit (NFR-CLOUD-004) — *S3 · Sonnet · Effort mittel*

Jeder Haken wird erst gesetzt, wenn die Qualitäts-Gates (CLAUDE.md §5) erfüllt
sind und die Anforderung im Register rückverfolgbar steht.

### Komplexität künftiger Meilensteine (grobe Orientierung, inkl. Effort)

- **M1.5** ASTERIX CAT048-Codec — *S3 · Sonnet · Effort hoch* (viel Code, aber
  bit-genau und fehleranfällig).
- **M3** WebSocket-Server/Cloud-Anbindung — *S4 · Opus 4.8 / Fable 5 · Effort hoch*;
  Map-Frontend (JS) — *S3 · Sonnet · Effort mittel*; CAT062-Encoder + Transport-
  Adapter — *S4 · Opus 4.8 / Fable 5 · Effort hoch*.
- **M4** Multi-Radar-Fusion + SSR/ADS-B-Korrelation — *S5 · Fable 5 / Opus 4.8 · Effort hoch–max*.
- **M5** IMM / JPDA — *S5 · Fable 5 / Opus 4.8 · Effort max*.
- Reine Doku-/Nachbereitungs-Schritte — *S1–S2 · Haiku · Effort niedrig*.

## 5. Offene Punkte / später entscheiden

- **CAT062 I062/290 / I062/500 gegen Spezifikation geprüft (erledigt):** Anhand
  des vom Projektverantwortlichen bereitgestellten Auszugs aus
  SUR.ET1.ST05.2000-STD-09-01 (Ed. 1.10) verifiziert: Primary-Subfield-Bit für
  PSR-Alter ist Bit 15 (= `0x40`), LSB ¼ s, ein Oktett — passt. Primary-
  Subfield-Bit für APC ist Bit 16 (= `0x80`), Subfeld = X- und Y-Komponente je
  16 Bit, LSB ½ m, vorzeichenlos — passt. Unsere Encoder-Konstanten und der
  Referenz-Dump (`0x40, 0x08` bzw. `0x80, 0x00, 0xC8, 0x00, 0xC8`) stimmen
  exakt. Das Mapping „update_age → PSR-Alter" war eine Single-Sensor-
  Vereinfachung; mit 4.A.4 trägt der `SystemTrack` jetzt `contributing_sensors`
  (welche Sensoren im letzten Scan beigetragen haben). Die CAT062-Kodierung
  dieser Mehr-Sensor-Information ist Teil von 4.2.
- **ASD-Integration (ADR 0006) vollständig umgesetzt (Häppchen C.1–C.3 und
  D.1–D.3):** Transport = **UDP-Multicast** (`firefly-multicast`, FR-NET-002),
  Koordinatenbezug = **System-Stereografisch** (`StereographicProjection`
  FR-GEO-004 → CAT062 I062/100, FR-IO-003) **zusätzlich** zu I062/105. Der
  Tracker-Kern bleibt WGS84-neutral (`SystemTrack`); Projektion und Transport
  sind reine Adapter-Aufgaben. Die **Empfänger-/Recorder-Seite** (Häppchen D)
  ist mit Decoder (D.1), I062/100-Rückprojektion (D.2) und echtem
  Multicast-Empfänger (D.3) abgeschlossen — ADR 0006 ist Ende-zu-Ende bewiesen
  (Sender → Draht → Empfänger → Decoder). Weiterhin offen: konfigurierbarer
  System-Referenzpunkt jenseits des Demo-Ursprungs.
- **Message-Bus-Technologie** (z. B. NATS/Kafka) — erst relevant ab M3, dann ADR.
- **Coverage-Werkzeug** (z. B. `cargo llvm-cov`) — einführen, sobald V&V-Nachweise
  greifbar werden.
- **Sicherheitsanalyse (FHA/Hazards)** — sinnvoll, sobald Tracker-Funktionen
  stehen, gegen die man Gefährdungen bewerten kann.
- **Out-of-order-Daten (Eingangs-Adapter, M3/M4):** Wenn ein *sehr alter* Plot
  *nach* neueren ankommt, kann man nicht sinnvoll rückwärts vorhersagen. Standard:
  am Eingang nach Datenzeit ordnen, kleines Zeitfenster puffern, zu Spätes
  *verwerfen* (nur den Plot, nicht den Track). Bewusst **kein** „Daten alt → Reset".
- **Frische-/Staleness-Anzeige (Ausgabe-Rand, M3):** Aus `SystemTrack.update_age`
  am Anzeige-Rand eine *weiche* Frische-Markierung ableiten — nie zustands-
  zerstörend (ADR 0008). Die Entscheidung selbst liegt schon im Tracker.
- **Vorführbarkeit (NFR-OPS-001):** Ein-Befehl-Demo ohne Programmierkenntnisse
  für Präsentationen — Umsetzung mit dem Frontend in M3.
- **GNN-Assoziationskosten (erledigt durch JPDA):** Die alte Notiz betraf die
  harte 1:1-Zuordnung (Ungarische Methode, reine `d²`-Kosten) bei dichtem
  Verkehr/überlappenden Gates. Mit M5.5–M5.9 ist die harte Zuordnung durch
  **JPDA** ersetzt (FR-TRK-018/019), das genau dieses Problem über
  Assoziationswahrscheinlichkeiten `β` und Exklusivität löst — kein separater
  Kostenterm mehr nötig.
- **Manöver-Handling (erledigt durch IMM):** Ein einzelnes `Q` deckt nur einen
  Manöver-Bereich ab. M5.1–M5.4 lösen das mit **IMM** (mehrere Bewegungsmodelle
  parallel, FR-TRK-011–014).
- **Höhen-Projektionsfehler bei `horizontal_from` (M6.1-Befund — BEHOBEN):**
  Trat ein hoch fliegendes Ziel *mitten im Flug* in die Reichweite eines
  zweiten Radars ein, während sein Track vom ersten Radar bereits eng
  eingerastet war, lag die erste Messung des zweiten Radars knapp außerhalb des
  engen Tores — `horizontal_from` projizierte die Bodenmessung im *Quellrahmen*
  (`up=0`) entlang der je Standort verschiedenen lokalen „Oben"-Richtung
  (wenige zehn bis ~100 m Versatz bei 10 km Höhe) → „Geister"-Spur. **Behoben:**
  `horizontal_from(source, z, height, r)` hebt jetzt den vollständigen
  3D-Punkt in den Tracking-Frame und projiziert erst dort auf den Boden →
  sensor-unabhängig (FR-GEO-003, Regressionstest
  `airborne_target_maps_to_one_point_from_two_sensors`). Frankfurt läuft damit
  wieder mit realistischen, überlappenden Reichweiten (100 km) und einem echten
  Nord-Handover auf 8 km Höhe — stabil bei acht Tracks.
- **Multi-Radar-Geister-Tracks bei Gate 0,99 (M6.1-Befund — BEHOBEN, ADR 0011):**
  Bei Gate `0,99` entstanden zwei „Geister"-Spuren. Diagnose: **kein**
  IMM-Manöver, sondern Fusions-Artefakte — (a) sequenzielle Tor-Verengung
  (Sensor A aktualisiert → Tor enger → Sensor Bs Plot fällt heraus → Duplikat)
  und (b) ein einzelner 3σ-Ausreißer-Plot, der sofort einen Track gebärt.
  **Behoben:** eine zu Scan-Beginn eingefrorene Fusions-Referenz (alle Sensoren
  gaten gegen die Prädiktion, keine Zwischen-Verengung) + ein getrenntes,
  weiteres Initiierungs-Sperr-Tor (`init_gate`, Default `0,9999`). FR-TRK-020,
  Test `tracker::outlier_plot_does_not_spawn_a_ghost`. Frankfurt läuft damit
  wieder mit dem Standard-Tor `0,99` und acht Tracks.
- **Instabilität bei asynchronen Radar-Scans (`scan_offset`, M6.1-Befund —
  BEHOBEN, ADR 0012):** In der dichten 8-Flugzeug/3-Radar-Szene führte ein
  Scan-Versatz zu massiver Track-ID-Instabilität (50–90 statt 8 IDs).
  **Ursache:** Der Lebenszyklus buchte Treffer/Fehltreffer **pro
  `process_scan`-Aufruf**; `Player::scans` gruppiert Plots nach exakt gleicher
  Zeit, also trug mit Versatz jeder Aufruf nur **einen** Sensor bei — ein
  Flugzeug, das gerade nur Radar B sieht, kassierte beim Offset-Scan von Radar A
  einen falschen „Miss" → Löschung → Respawn. **Behoben:** der **adaptive
  Lebenszyklus** (FR-TRK-021) zählt Bestätigung/Löschung in
  `coast_reference = max(revisit_interval, cadence)` Sekunden statt in
  Scan-Aufrufen — `revisit_interval` (EWMA der Treffer-Zeitlücken je Track) und
  `cadence` (geschätzte Feed-Taktung) wachsen auf die wahre Sensor-Periode, ein
  einzelner kurzer Versatz zwischen zwei Radaren zählt nicht als verpasste
  Wiederkehr. Frankfurt läuft jetzt dauerhaft mit `scan_offset = 0/1.3/2.6 s`
  und genau acht Track-IDs (`scene::frankfurt_scene_keeps_one_identity_per_aircraft`).

## 6. So steige ich wieder ein (Kurzbefehle)

```bash
cargo test --workspace                     # alles grün?
cargo run --example demo -p firefly-sim    # M1-Simulator live sehen
```

Doku-Einstieg: `docs/README.md` → Glossar, Meilensteine, ADRs, Requirements.
