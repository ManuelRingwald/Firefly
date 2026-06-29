# ORCH-Source-Input 2a — `FIREFLY_SOURCES`-Parser + Mapping

> Erster von zwei Schritten zur Umsetzung des Quell-Eingangs-Kontrakts (ADR 0023):
> die **reine, env-freie** Parse-/Mapping-Logik. Die Verdrahtung in den
> Live-Tracker folgt als Schritt 2b.

## Fachlicher Hintergrund

Wayfinders Auto-Orchestrierung (ADR 0012 dort) fährt eine Firefly-Instanz pro
Feed und sagt ihr über Umgebungsvariablen, **woraus** sie rechnet. Der dafür
ratifizierte Kontrakt (ADR 0023, `docs/source-input-contract.md` v1.0.0) ist eine
JSON-Quell-Liste `FIREFLY_SOURCES`. Schritt 2a baut den Leser dieser Liste —
abgekapselt und vollständig unit-testbar, bevor er an den Server gehängt wird.

## Was umgesetzt wurde (`firefly-server::sources`)

- **Typen** (serde `Deserialize`): `SourceType` (geschlossenes Vokabular
  `adsb_opensky`/`flarm_aprs`/`radar_asterix`, `snake_case`), `BBox` (Feldnamen
  identisch zu Wayfinders `source_config` → Pass-through), `SourceSpec`
  (`type`/`bbox?`/`sac?`/`sic?`/`sensor_id?`/`cred_env?`).
- **`parse_sources(json)`** → `Vec<SourceSpec>` via `serde_json`. Ein
  **unbekannter `type`** (außerhalb des Vokabulars) oder malformes JSON ist ein
  **harter Fehler** (Startkonfig-Fault), nie ein still ignorierter Eintrag.
- **`opensky_config_from_spec(spec, index, get_env)`** → `OpenSkyConfig`: bildet
  einen `adsb_opensky`-Eintrag auf die bestehende Adapter-Konfig ab —
  BBox → Query-Fenster, `sensor_id` (Default wenn fehlend), und die per `cred_env`
  **benannte** Env aufgelöst und am **ersten `:`** in `user:pass` gesplittet (UX-2;
  Passwort darf `:` enthalten, der Basic-Auth-Username nicht). Fehlende/leere
  Cred-Env → Fehler; ohne `cred_env` → anonymer Zugang. `get_env` ist injiziert →
  testbar ohne Prozess-Umgebung.
- **BBox-Validierung:** finit, WGS84-Bereich, `min ≤ max` — eine Konfig, die sonst
  still ein leeres OpenSky-Fenster ergäbe, wird als `InvalidBBox` abgewiesen.
- **Fehlertyp** `SourceError` mit `Display` (klare, indizierte Meldungen je
  Listen-Eintrag) — alle Varianten sind Startkonfig-Faults.

## Sicherheits-/Robustheits-Betrachtung

- **Kein blindes Vertrauen in die Konfig:** unbekannter Typ, fehlende/ungültige
  BBox, fehlende/malformede Credentials werden als Fehler gemeldet statt still
  hingenommen.
- **Credential-Klartext** steht nie im `FIREFLY_SOURCES`-Blob — nur in der per
  `cred_env` benannten Env (Isolation; ehrliche Grenze: Env ist kein Tresor, ADR
  0023 §Konsequenzen).
- **Keine CAT062-Ausgabe-Wirkung;** Eingangs-Kontrakt v1.0.0.

## Tests

`firefly-server::sources` (11 Unit-Tests): Mixed-Liste parsen, unbekannter Typ →
Fehler, malformes JSON → Fehler, BBox/Sensor-Mapping, Default-Sensor-ID,
Cred-Split (inkl. „erster `:`"), fehlende/leere/colon-lose Credentials, fehlende
BBox, invertierte/Bereichs-überschreitende BBox. `cargo test --workspace`,
`cargo clippy`, `cargo fmt` grün.

## Rückverfolgbarkeit

Anforderungs-Register: **FR-NET-011** (Quell-Eingangs-Kontrakt-Parser, Status
„teilweise" — 2b folgt).

## Nächster Schritt (2b)

`build_live_state` aus *N* Adaptern der Liste speisen: je `adsb_opensky`-Spec ein
Poller in den geteilten `mpsc`; reservierte Typen (FLARM/Radar) → WARN+skip;
Sensor-Health-Monitor über alle Sensor-IDs; `FIREFLY_SOURCES` hat Vorrang vor
`FIREFLY_OPENSKY_*`. Danach TECHNICAL/INSTALLATION-Env-Nachzug + Wayfinder ORCH-5.
