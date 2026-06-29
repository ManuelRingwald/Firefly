# ORCH-Source-Input 2b — Live-Verdrahtung von `FIREFLY_SOURCES`

> Zweiter Schritt zum Quell-Eingangs-Kontrakt (ADR 0023): den in 2a gebauten
> Parser wirksam machen — eine orchestrierte Instanz speist ihren Live-Tracker aus
> *N* Adaptern der `FIREFLY_SOURCES`-Liste.

## Fachlicher Hintergrund

2a lieferte die reine Parse-/Mapping-Logik (`firefly-server::sources`). 2b hängt
sie an den Server: im Live-Modus liest Firefly `FIREFLY_SOURCES`, baut je Quelle
einen Adapter und füttert den bestehenden Live-Tracker. Damit ist der
End-to-End-Pfad bereit, sobald Wayfinder (ORCH-5) die Env injiziert.

## Was umgesetzt wurde

### Quell-Auflösung (`sources::resolve_sources` + `representative_config`)
- `resolve_sources(specs, get_env)` → `ResolvedSources`: `adsb_opensky` →
  `OpenSkyConfig` (Cred via `get_env` aufgelöst), `flarm_aprs`/`radar_asterix` →
  `skipped` (reserviert). Ein **fehlerhafter** `adsb_opensky`-Eintrag
  (fehlende/ungültige BBox, malformede Cred) bricht hart ab — eine konfigurierte,
  nicht lauffähige Quelle wird **nicht** still verworfen.
- `representative_config(configs)`: Tracking-Frame-Ursprung + Ausgabe-Takt über N
  Quellen — **Union** aller BBoxen, **min** Poll-Intervall, erste Sensor-ID
  (Platzhalter für den geodätischen ADS-B-Pfad). Leer → Default.

### Verdrahtung (`firefly-server::main::build_live_state`)
- `resolve_live_sources()`: `FIREFLY_SOURCES` gesetzt → parsen + auflösen;
  Parse-/Config-Fehler → **Prozess-Exit** (der Orchestrator sieht den Container
  fallen statt fehl-gequellt laufen). Reservierte Typen → WARN+skip; leere
  Effektiv-Menge → WARN (leerer Himmel). **Vorrang** vor `FIREFLY_OPENSKY_*`;
  ungesetzt → Back-Compat-Fallback auf die einzelne `FIREFLY_OPENSKY_*`-Quelle.
- **Ein Poller je Quelle** in den geteilten `mpsc` (`plots_tx.clone()`); der
  Live-Tracker akzeptiert mehrere Produzenten unverändert.
- `SensorHealthMonitor::new_live` über **alle** Quell-Sensoren (CAT063-Liveness je
  Quelle).
- Tracking-Frame, Referenzpunkt (`live_system_reference_point`) und Output-Takt aus
  `representative_config`; `build_live_tracker` unverändert (single-sensor reicht,
  der geodätische Pfad nutzt das Sensormodell nicht).

## Sicherheits-/Robustheits-Betrachtung

- **Fail-fast bei Fehlkonfig:** unbrauchbare `FIREFLY_SOURCES` → Container-Exit,
  kein stiller Betrieb mit falschen/fehlenden Quellen.
- **Verfügbarkeit vor Vollständigkeit:** reservierte Typen werden geskippt (die
  Instanz dient die Quellen, die sie kann), statt den Start zu blockieren.
- **Credential-Klartext** bleibt in der benannten Env, nie im JSON-Blob; ehrliche
  Grenze (Env zur Laufzeit sichtbar) wie ADR 0023 dokumentiert.
- **Keine CAT062-Ausgabe-Wirkung.**

## Tests

- `sources::resolve_splits_opensky_from_reserved_types`,
  `sources::resolve_propagates_a_bad_adsb_entry`,
  `sources::representative_unions_bboxes_and_takes_min_interval`,
  `sources::representative_of_empty_is_the_default` (neu, 2b) + die 11 Parser-Tests
  aus 2a. Der Live-Startpfad ist zusätzlich durch den bestehenden
  `app_test::websocket_client_receives_frames_in_live_mode` (Fallback-Pfad)
  abgesichert. `cargo test --workspace`, `cargo clippy --all-targets`, `cargo fmt`
  grün; kein `unsafe`.

## Rückverfolgbarkeit

Anforderungs-Register: **FR-NET-011** (jetzt „verifiziert" — 2a + 2b). Env-Doku:
`docs/TECHNICAL.md` §1.5.1, `docs/INSTALLATION.md` §7.

## Nächster Schritt (Wayfinder ORCH-5)

Docker-Backend übersetzt `source_config` → `FIREFLY_SOURCES` + injiziert die
aufgelösten Creds in die benannten Cred-Envs; UI-Zwei-Felder (UX-2). Danach
End-to-End-Abnahme (Quelle konfiguriert → Firefly-Instanz → CAT062 → Lagebild).
FLARM/APRS- und Radar-ASTERIX-Adapter bleiben spätere, je eigene ADRs.
