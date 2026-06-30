# ADR 0026 — FLARM/OGN-Eingangs-Adapter (`flarm_aprs` via APRS-IS)

- **Status:** akzeptiert
- **Datum:** 2026-06-30
- **Schnittstellen-relevant:** Eingangs-Kontrakt — `flarm_aprs` wechselt von
  **reserviert → unterstützt**; `docs/source-input-contract.md` wird mit der
  Umsetzung (Schritt C) auf **v1.2.0** gehoben (additiv). Der **Ausgabe**-Vertrag
  (CAT062/UDP) bleibt unberührt.
- **Auslöser:** Wayfinder-Issue [#35](https://github.com/manuelringwald/firefly/issues/35)
  (`from-wayfinder`) — die im Quell-Kontrakt (ADR 0023) reservierten Adapter
  `flarm_aprs` / `radar_asterix` nachliefern. Dieser ADR betrifft `flarm_aprs`.

## Kontext

Firefly kann live tracken (ADR 0020) und über den env-getriebenen Quell-Kontrakt
(ADR 0023) eine Liste heterogener Quellen je Instanz speisen. Heute existiert genau
**ein** Adapter — OpenSky-ADS-B (ADR 0019, OAuth2 in ADR 0024). Er ist die Blaupause:
ein eigenes Crate, das `Vec<Plot>` (`Measurement::Geodetic`, WGS84) erzeugt und über
einen `mpsc`-Kanal (`plots_tx`) in den Tracker speist.

**Fachliche Lücke.** Ein realer ASD braucht **Segelflieger, UL, Hubschrauber und
tieffliegende GA**, die ADS-B/OpenSky oft **nicht** sieht. Diese Luftfahrzeuge tragen
verbreitet **FLARM**; ihre Positionen sind über das **Open Glider Network (OGN)** via
**APRS-IS** öffentlich abgreifbar. `flarm_aprs` erschließt diese komplementäre Quelle
— genau der Fall, für den ADR 0023 das Vokabular reserviert hat. Wayfinder rendert die
Herkunft bereits; der Lückenschluss ist die Firefly-Seite.

**Transport-Unterschied zu OpenSky.** OpenSky ist ein REST-**Poll** (Pull). OGN/APRS-IS
ist ein **dauerhafter TCP-Stream** (Push): der Client meldet sich an, setzt einen
Server-seitigen Geo-Filter und empfängt Pakete zeilenweise, bis die Verbindung
abbricht. Das prägt die Adapter-Struktur.

## Entscheidung

### 1. Transport: APRS-IS-TCP-Stream mit Server-seitigem Area-Filter

Der Adapter hält eine **dauerhafte TCP-Verbindung** zu einem APRS-IS-Server (Default
einer der `*.glidernet.org`-Rotations-Hosts, konfigurierbar). Aus der `bbox` der Quelle
wird der APRS-IS-**Area-Filter** `a/latN/lonW/latS/lonE` gebildet (Server filtert
geografisch, kein Client-seitiges Verwerfen ganz Europas). Pakete werden **zeilenweise**
gelesen. **Reconnect mit exponentiellem Backoff** bei Abbruch (Verfügbarkeit vor
Vollständigkeit, konsistent mit der Fehlertoleranz des OpenSky-Pollers).

### 2. Auth: APRS-IS-Login, read-only standardmäßig anonym

APRS-IS verlangt eine Login-Zeile (`user <CALLSIGN> pass <PASSCODE> …`). Für **reines
Mitlesen** genügt **Passcode `-1`** (read-only, anonym) — Firefly **sendet nie**.
Daher:

- **Ohne `cred_env`:** read-only-Login mit generiertem Pseudo-Callsign + Passcode `-1`.
  Das ist der Normalfall für reine Konsumtion.
- **Mit `cred_env`:** der Wert ist `callsign:passcode` (gleiche **Ein-String-mit-einem-
  Doppelpunkt**-Konvention wie `adsb_opensky`, Split am **ersten** `:`). Nur nötig, wenn
  ein benannter APRS-IS-Account gewünscht ist.

### 3. Parsing: schlanker Eigenbau-OGN-Parser (robust, getestet)

Statt einer schwergewichtigen APRS-Crate ein **fokussierter, eigener Parser** für
**OGN-Positions-Reports** (APRS-Position + OGN-Kommentarfeld). Begründung: minimale
Abhängigkeiten (Charta), und der Decoder ist ein **sicherheits-relevanter
Eingangspfad** — er muss **robust** sein (kein Panic auf Eingabe, längen-/grenzen-
geprüft, fehlerhafte Zeilen verworfen statt abgestürzt) und **byte-/string-genau gegen
echte Paket-Fixtures** getestet werden. Eine breite Fremd-Crate brächte Funktionsumfang,
den wir nicht brauchen, und Angriffsfläche, die wir nicht prüfen.

### 4. FLARM-Identität → Tracker: ICAO nur wenn echt ICAO

OGN-Pakete tragen einen **Adress-Typ** (ICAO / FLARM / OGN-Tracker / random). Der
Adapter setzt `mode_ac.icao_address` **nur**, wenn der Adress-Typ **ICAO** ist (dann
greift der ICAO-basierte Pre-Sort des Trackers, AP9.3). Bei FLARM-/OGN-eigenen IDs
**bleibt die ICAO-Adresse leer** — der Track läuft rein **kinematisch** (PSR-artig). Das
ist korrekt (eine FLARM-ID ist **keine** ICAO-24-Adresse) und bereitet die spätere
echte **`flarm`-Provenienz** (Issue #30) sauber vor, ohne sie vorwegzunehmen.

### 5. Mess-Genauigkeit: isotrope Geodetic-Kovarianz, σ ≈ 20 m

Jeder Report wird zu einem `Plot` mit `Measurement::Geodetic` und **isotroper**
Kovarianz `R = σ²·I₂`, **σ ≈ 20 m** (Default, justierbar). Begründung: FLARM-GPS ist
gut (wenige Meter), aber die **APRS-Positionskodierung ist gerundet** (Basis-Präzision
~18 m, ggf. DAO-Verfeinerung); 20 m ist eine ehrliche, leicht konservative Annahme.

### 6. Plot-Erzeugung & Einspeisung wie OpenSky

Ein gültiger OGN-Positions-Report → **ein `Plot`** (Geodetic, `sensor_id` aus der Spec,
Datenzeit aus dem Paket bzw. Empfangszeit als Fallback). Die Plots laufen in **denselben
`plots_tx`-Kanal** wie die OpenSky-Plots; Tracker-Kern und Ausgabe bleiben unverändert
format-neutral (Ports & Adapters).

### 7. Vokabular & Kontrakt

`flarm_aprs` wird **unterstützt**: Felder `bbox` (Pflicht), `sensor_id` (optional),
`cred_env` (optional). Im Quell-Kontrakt wandert der Typ von „reserviert" auf
„unterstützt"; `source-input-contract.md` → **v1.2.0** (additiv) mit der Umsetzung
(Schritt C).

## Umsetzungs-Häppchen (je für sich testbar, eigener Commit)

- **Schritt A** *(dieser ADR)* — Design ratifizieren. *Kein Code.*
- **Schritt B — Crate `firefly-flarm`:** APRS-IS-Client (Connect, Login, Area-Filter,
  zeilenweise Lesen, Reconnect+Backoff) + OGN-Parser → `Plot`. Robust, voll unit-
  getestet gegen echte Paket-Fixtures. **Keine** Verdrahtung in den Server.
- **Schritt C — Kontrakt + Verdrahtung:** `sources.rs` mappt `flarm_aprs` → `FlarmConfig`
  (raus aus `skipped`, neues `flarm`-Feld in `ResolvedSources`); `main.rs` spawnt den
  Listener (Sensor-Health-Monitor + `firefly_flarm_*`-Metrik); `source-input-contract.md`
  → v1.2.0; Anforderungs-Register/INSTALLATION/TECHNICAL; Cross-Project-Issue #35
  aktualisieren.

## Sicherheit & Robustheit

- **Kein Vertrauen ins Datagramm:** der Parser prüft Längen/Grenzen und verwirft
  fehlerhafte Zeilen, statt zu panicken; Fuzzing des Parsers vorgesehen.
- **Spoofing-Grenze (ehrlich):** APRS-IS-Daten sind **öffentlich und nicht
  authentifiziert** (wie ADS-B, ADR 0019). Der Tracker vertraut den Positionen at face
  value; Schutz ist die Netz-/Quellen-Isolation (ADR 0017) und kinematische
  Plausibilität, nicht Krypto.
- **Verfügbarkeit:** ein toter/abbrechender Feed beendet **nicht** den Server — der
  Listener reconnectet; ein nicht-startender Adapter wird geloggt, die übrigen Quellen
  laufen weiter.

## Konsequenzen

- **Positiv:** komplementäre Abdeckung (Segelflug/UL/tiefe GA), die ADS-B nicht liefert;
  Charta-konform als generischer Ports-&-Adapters-Eingang (Tracker-Kern format-neutral,
  nützt jedem ASD); zweiter realer Live-Adapter nach OpenSky → der `FIREFLY_SOURCES`-
  Mehrquellen-Fall wird erstmals real ausgereizt.
- **Negativ / Grenzen:** APRS-Positionen sind gerundet (begrenzte Genauigkeit, in σ
  berücksichtigt); OGN-Abdeckung ist netz-/empfänger-abhängig (keine garantierte
  Vollständigkeit); FLARM-IDs sind nicht standardisiert ICAO (daher der bewusste
  Identitäts-Sonderfall, #4); öffentliche, ungesicherte Quelle (Spoofing-Grenze oben).

## Alternativen erwogen

- **Schwergewichtige APRS-/OGN-Fremd-Crate:** verworfen — unnötiger Funktionsumfang und
  ungeprüfte Angriffsfläche auf einem sicherheits-relevanten Eingangspfad; ein schlanker,
  getesteter Eigenbau ist analysierbarer (Charta/ADR 0004).
- **Poll statt Stream:** verworfen — OGN/APRS-IS ist ein Push-Stream; Pollen gibt es nicht.
- **ICAO-Adresse für *alle* FLARM-IDs erzwingen:** verworfen — eine FLARM-ID ist keine
  ICAO-24-Adresse; das würde falsche Korrelationen im ICAO-Pre-Sort erzeugen.
- **Sofort Schritt B+C mitbauen:** verworfen — der Adapter ist ein großes Häppchen; erst
  Design ratifizieren, dann Crate, dann Verdrahtung (Charta §2).

## Querverweise

- Quell-Kontrakt (maßgeblich, versioniert): `docs/source-input-contract.md`.
- ADR 0019 (OpenSky-Adapter, Blaupause), ADR 0020 (Live-Tracker-Modus),
  ADR 0023 (Quell-Eingangs-Kontrakt), ADR 0017 (Multicast-/Feed-Vertrauensgrenze).
- Cross-Project: `docs/cross-project/todo-for-wayfinder.md`; Issue #35.
- Vorbereitet: Issue #30 (echte Per-Track-`flarm`-Provenienz, CAT062-ICD) — baut auf der
  Identitäts-Entscheidung (#4) auf.
