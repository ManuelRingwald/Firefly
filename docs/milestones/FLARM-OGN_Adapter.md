# Meilenstein — FLARM/OGN-Eingangs-Adapter (`flarm_aprs`)

> Zweiter Live-Quell-Adapter aus Issue #35 (ADR 0026). Erschließt FLARM-getragene
> Luftfahrzeuge über das Open Glider Network (OGN) via APRS-IS als komplementäre
> Surveillance-Quelle neben ADS-B/OpenSky.

## Fachlichkeit — *warum*

Ein realer ASD braucht **Segelflieger, Ultraleicht, Hubschrauber und tieffliegende
Allgemeine Luftfahrt**, die ADS-B (und damit OpenSky) oft **nicht** sieht. Diese
Luftfahrzeuge tragen verbreitet **FLARM** (Kollisionswarngerät). Das **Open Glider
Network** empfängt diese Beacons über ein Empfänger-Netz und re-broadcastet sie
öffentlich über **APRS-IS**. Der Adapter zapft diesen Strom an und speist die
Positionen als Plots in denselben Tracker wie ADS-B/Radar → **fusionierte**
CAT062-Tracks.

## Technik — *wie*

Eigenes Crate `firefly-flarm` (Ports & Adapters, Tracker-Kern format-neutral),
gespiegelt am OpenSky-Adapter (ADR 0019):

| Modul | Aufgabe |
|-------|---------|
| `config.rs` | `FlarmConfig` (12-Factor, `FIREFLY_FLARM_*`); read-only anonym als Default |
| `ogn.rs` | **OGN/APRS-Parser** `parse_position` → `OgnPosition` (robust, kein Panic) |
| `plot.rs` | `position_to_plot` → `firefly_core::Plot` (`Measurement::Geodetic`) |
| `aprsis.rs` | APRS-IS-Client: Area-Filter, Login, Stream-Verarbeitung, Reconnect+Backoff |

**Transport (Unterschied zu OpenSky):** APRS-IS ist ein **dauerhafter TCP-Stream**
(Push), kein REST-Poll. Der Client meldet sich mit einem **Server-Area-Filter**
(`a/latN/lonW/latS/lonE`, aus der bbox) an und liest zeilenweise. Read-only
benötigt nur Passcode `-1` — Firefly **sendet nie**.

**Verdrahtung (Schritt C):**
- `firefly-server::sources::flarm_config_from_spec` bildet einen `flarm_aprs`-Eintrag
  aus `FIREFLY_SOURCES` auf `FlarmConfig` ab (Kontrakt v1.2.0); `cred_env`-Wert
  `callsign:passcode`.
- `main::spawn_flarm_listener_live` startet den Listener und speist Plots in den
  Live-Tracker-Kanal.
- `build_live_tracker_multi` registriert **alle** Quell-Sensoren (OpenSky + FLARM),
  sonst verwirft der Tracker FLARM-Plots (FR-TRK-010).
- Metrik `firefly_flarm_plots_received_total`; FLARM-Sensor im `SensorHealthMonitor`
  (CAT063) mit Nominal-Scan-Periode (5 s, Push-Stream hat kein Poll-Intervall).

## „Mathematik" — Dekodierung

OGN nutzt **unkomprimierte, fest breite** APRS-Positionen (gegen die kanonischen
OGN-Referenzparser verifiziert):

- **Breite** `DDMM.mmH` → `± (DD + MM.mm/60)` Grad (Vorzeichen aus `N`/`S`).
- **Länge** `DDDMM.mmH` → analog (`E`/`W`).
- **DAO** `!Wxy!` verfeinert die Minuten um `x`/`y` **Tausendstel-Minuten** (≈ 1,85 m)
  vor dem Vorzeichen.
- **OGN-`id`-Byte** `STttttaa`: Adresstyp `aa` (low 2 Bit: `0` random, `1` ICAO,
  `2` FLARM, `3` OGN), Aircraft-Typ `tttt`. Die **ICAO-Adresse** wird nur bei
  Adresstyp ICAO gesetzt (FLARM/OGN-IDs sind keine ICAO-24-Adressen — bereitet die
  echte `flarm`-Provenienz, Issue #30, vor).
- **Höhe** `/A=NNNNNN` (Fuß) → Position-Höhe (× 0,3048 m) + `flight_level_ft`.
- **Mess-Kovarianz**: isotrop `R = σ²·I₂`, **σ ≈ 20 m** (APRS-Positionsrundung).

## Sicherheit & Robustheit

Der Parser ist ein **untrusted-Netz-Eingangspfad**: jede Stufe ist
grenz-/längen-geprüft (`str::get`, `checked` Parsing), **kein Panic** auf Eingabe
(Truncation- + Mutations-Fuzz-Tests). Empfänger-Beacons ohne Aircraft-Adresse
werden verworfen. APRS-IS-Daten sind öffentlich und nicht authentifiziert; die
Vertrauensgrenze ist die Netz-/Quellen-Isolation (ADR 0017), wie bei ADS-B.

## Nachweis

`firefly-flarm`: 20 Tests (Parser inkl. echter OGN-Beispielzeilen + Fuzz, Plot,
Config, APRS-IS-Helfer + Stream). `firefly-server`: `sources::flarm_*`,
`representative_covers_a_flarm_only_feed`, `metrics::render_includes_all_metrics`.
Anforderung **FR-NET-012**; adversarische Parser-Prüfung gegen die kanonischen
OGN-Referenzparser ohne Befund.

## Status

- **Schritt A** (ADR 0026) ✅ · **Schritt B** (Crate) ✅ · **Schritt C** (Verdrahtung
  + Kontrakt v1.2.0 + Doku) ✅.
- Schnittstelle: Eingangs-Kontrakt v1.2.0 (additiv); **Ausgabe-Vertrag CAT062
  unverändert**. Cross-Project-Issue #35 (Firefly-Seite für `flarm_aprs` erledigt;
  `radar_asterix` bleibt offen).
