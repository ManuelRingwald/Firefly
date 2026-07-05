# Quell-Eingangs-Kontrakt (`FIREFLY_SOURCES`, env-getrieben)

> **Was das ist.** Die **maßgebliche, versionierte** Beschreibung, wie ein
> Orchestrator (Wayfinder, ADR 0012 dort) einer **Firefly-Instanz** per
> Umgebungsvariablen mitgibt, **woraus** sie ihre Tracks rechnet — die Liste der
> Live-Quellen samt Geo-Ausschnitt und Credentials.
>
> Dies ist der **Eingangs**-Kontrakt (Orchestrator → Firefly). Er ist getrennt vom
> **Ausgabe**-Vertrag (CAT062/UDP, `docs/ICD-CAT062.md`) und berührt diesen nicht.
> Kein gemeinsamer Code — beide Seiten implementieren den Vertrag unabhängig.
>
> **Eigentümerschaft & Änderungsprozess:** Dieses Dokument lebt im Firefly-Repo,
> da Firefly der **Leser** dieser Envs ist. Jede Änderung:
> 1. per ADR in Firefly begründen,
> 2. Version unten erhöhen + Changelog-Eintrag,
> 3. Wayfinder informieren (Issue `from-firefly`, referenziert in
>    `docs/cross-project/todo-for-wayfinder.md`).

---

## Version

**1.5.0** (2026-07-05) — Neuer Quell-Typ **`adsb_aggregator`** (ADR 0031):
auth-freier ADS-B-Bezug über einen ADSBExchange-v2-kompatiblen
Community-Aggregator. Felder: `bbox` (Pflicht), `provider`? (`adsb_lol` Default |
`adsb_fi`), `sensor_id`? (Default 230), `poll_interval_secs`? (Default 10 s).
**Kein** `cred_env` (die Dienste sind offen; ein dennoch gesetztes `cred_env`
wird ignoriert statt abgelehnt — robust gegen veraltete Referenzen nach einem
Quell-Typwechsel). Ein unbekannter `provider` ist ein **Startfehler**.
**Additiv** — bestehende Quellen unverändert; ein Leser älterer Version lehnt
den neuen Typ als unbekannt ab (Orchestrator: Typ erst nach Firefly-Rollout
anbieten). Minor-Bump.

Vorgänger **1.4.0** (2026-07-02) — `adsb_opensky` trägt ein optionales **`poll_interval_secs`**
(ADR 0029): das Poll-Intervall des OpenSky-Pollers in ganzen Sekunden, vom
Orchestrator pro Quelle setzbar. Fehlt oder `0` → Firefly-Default (10 s). Nur für
`adsb_opensky` sinnvoll (FLARM/APRS ist Push, Radar hat eine eigene Scan-Periode).
**Additiv** — bestehende Quellen unverändert; ein Leser älterer Version ignoriert
das Feld. Minor-Bump.

Vorgänger **1.3.0** (2026-06-30) — `radar_asterix` ist **unterstützt** (ADR 0028): Adapter
für einen realen Monoradar via **ASTERIX CAT048 über UDP**. Felder: `sac`/`sic`
(Sensor-Identität), `lat`/`lon` (Pflicht — Radar-Standort; CAT048 ist polar und
trägt ihn nicht), `height_m`? (Default 0), `listen`? (`group:port`, Default
`0.0.0.0:8048`), `sensor_id`?. **Kein** `cred_env` (roher UDP-Strom; Vertrauens-
grenze ist Netz-Isolation, ADR 0017). **Additiv** — bestehende Quellen
unverändert. Minor-Bump.

Vorgänger **1.2.0** (2026-06-30) — `flarm_aprs` ist **unterstützt** (ADR 0026):
Adapter für FLARM-Positionen über das Open Glider Network (OGN) via APRS-IS.
Felder: `bbox` (Pflicht), `sensor_id`?, `cred_env`? mit Wert `callsign:passcode`
(read-only anonym ohne `cred_env`). **Additiv** — kein Wire-Format-Bruch;
bestehende Quellen unverändert. Minor-Bump.

**1.1.0** (2026-06-29) — `adsb_opensky`-Cred-Wert ist nun
`client_id:client_secret` (OpenSky OAuth2 Client-Credentials, ADR 0024) statt
`benutzer:passwort`. **Wire-Vertrag unverändert** (ein String, ein Doppelpunkt) —
nur die Bedeutung der zwei Teile, daher Minor-Bump.

**1.0.0** (2026-06-29) — Erstdefinition (ADR 0023). JSON-Liste `FIREFLY_SOURCES`,
Credentials isoliert in benannten Cred-Envs, `adsb_opensky` als erster
unterstützter Quell-Typ; `flarm_aprs`/`radar_asterix` reserviert.

---

## 1. Aktivierung

| Env | Pflicht | Bedeutung |
|-----|---------|-----------|
| `FIREFLY_MODE` | ja | `live` aktiviert den Live-Tracker-Modus (ADR 0020). `FIREFLY_SOURCES` ist **nur** im Live-Modus wirksam. |
| `FIREFLY_SOURCES` | im Live-Modus | JSON-Array der Quellen (Abschnitt 2). Leer/ungesetzt = keine Quelle. |
| `FIREFLY_CAT062_GROUP` / `_PORT` | — | Ausgabe-Endpoint (CAT062/UDP). Unverändert, siehe ICD. |

**Präzedenz zu `FIREFLY_OPENSKY_*`:** Ist `FIREFLY_SOURCES` gesetzt, hat es Vorrang;
die diskreten `FIREFLY_OPENSKY_*`-Envs (Standalone-/Dev-Pfad) werden dann **nicht**
zusätzlich ausgewertet (kein Doppel-Adapter).

## 2. `FIREFLY_SOURCES` — Schema

Ein JSON-Array. Jeder Eintrag:

| Feld | Typ | Pflicht | Gilt für | Bedeutung |
|------|-----|---------|----------|-----------|
| `type` | string | ja | alle | Quell-Art (Abschnitt 3). |
| `bbox` | object | Flächenquellen | `adsb_opensky`, `adsb_aggregator`, `flarm_aprs` | `{min_lat, min_lon, max_lat, max_lon}` (WGS84, Grad). |
| `provider` | string | optional | `adsb_aggregator` | Welcher Community-Aggregator abgefragt wird: `adsb_lol` (Default) oder `adsb_fi`. Unbekannter Wert → **Startfehler** (nie ein still substituierter Anbieter). |
| `sac` / `sic` | int 0..255 | Radar | `radar_asterix` | Sensor-Identität (I048/010). |
| `lat` / `lon` | float | Radar (Pflicht) | `radar_asterix` | **Radar-Standort** (WGS84, Grad). CAT048 ist polar relativ zum Radar und trägt den Standort nicht — Firefly braucht ihn, um Polar-Plots ins Tracking-Frame zu heben (ADR 0028). |
| `height_m` | float | optional | `radar_asterix` | Radar-Standort-Höhe über dem WGS84-Ellipsoid, Meter. Default `0`. |
| `listen` | string | optional | `radar_asterix` | UDP-Endpoint `group:port` für den ASTERIX-Eingang. Multicast-Gruppe → beigetreten; sonst Unicast-Bind. Default `0.0.0.0:8048`. |
| `sensor_id` | int | optional | alle | Auf die Plots gestempelte `SensorId`. Fehlt → Firefly vergibt einen Default je Adapter (OpenSky 200, Aggregator 230, FLARM 210, Radar 220). |
| `poll_interval_secs` | int > 0 | optional | `adsb_opensky`, `adsb_aggregator` | Poll-Intervall des Pollers in ganzen Sekunden. Fehlt oder `0` → Firefly-Default (10 s). Nur für gepollte Quellen (FLARM/APRS ist Push, Radar hat eine eigene Scan-Periode). |
| `cred_env` | string | optional | `adsb_opensky`, `flarm_aprs` | **Name** der Env, die den Credential-Klartext trägt (Abschnitt 4) — **nie** der Wert selbst. Fehlt → anonymer/credential-loser Zugang. (`radar_asterix` trägt keine Credentials; `adsb_aggregator` ist auth-frei und **ignoriert** ein gesetztes `cred_env`.) |

Die `bbox`-Feldnamen sind identisch zu Wayfinders `source_config`, sodass der
Orchestrator nahezu pass-through serialisieren kann.

**Beispiel:**
```
FIREFLY_MODE=live
FIREFLY_SOURCES=[
  {"type":"adsb_opensky",
   "bbox":{"min_lat":48.0,"min_lon":7.0,"max_lat":50.0,"max_lon":9.0},
   "sensor_id":201,
   "cred_env":"FIREFLY_SOURCE_0_SECRET"}
]
FIREFLY_SOURCE_0_SECRET=alice:s3cr3t
FIREFLY_CAT062_GROUP=239.255.0.62
FIREFLY_CAT062_PORT=8600
```

## 3. Quell-Vokabular

| `type` | Status | Adapter | Felder |
|--------|--------|---------|--------|
| `adsb_opensky` | **unterstützt** (ADR 0019) | OpenSky-REST-Poller | `bbox` (Pflicht), `sensor_id`?, `poll_interval_secs`?, `cred_env`? |
| `adsb_aggregator` | **unterstützt** (ADR 0031) | Community-Aggregator-Poller (adsb.lol / adsb.fi) | `bbox` (Pflicht), `provider`?, `sensor_id`?, `poll_interval_secs`? — **kein** `cred_env` |
| `flarm_aprs` | **unterstützt** (ADR 0026) | OGN/APRS-IS-Stream | `bbox` (Pflicht), `sensor_id`?, `cred_env`? |
| `radar_asterix` | **unterstützt** (ADR 0028) | ASTERIX-CAT048-UDP-Listener | `sac`/`sic`, `lat`/`lon` (Pflicht), `height_m`?, `listen`?, `sensor_id`? |

**Hinweis `adsb_aggregator` (ADR 0031):** Die Aggregator-APIs fragen
Mittelpunkt+Radius (max. 250 NM) statt einer BBox. Firefly rechnet die
konfigurierte `bbox` in ihren **Umkreis** um und **filtert** die Antwort auf die
BBox zurück — die Quelle verhält sich also exakt wie die BBox-nativen Quellen.
Sprengt die BBox den 250-NM-Radius, wird geclampt und beim Start **prominent
gewarnt** (partielle Abdeckung wird nie verschwiegen).

**Behandlung:**
- Alle Vokabular-Typen haben einen Adapter (keine reservierten Typen mehr).
- Ein **fehlerhaft konfigurierter** Eintrag (fehlende `bbox`/`lat`/`lon`, ungültiges
  `listen`) → **Startfehler** (eine konfigurierte Quelle, die nicht laufen kann,
  wird nicht still verworfen).
- Ein **unbekannter** (vokabular-fremder) Typ → **Startfehler** (Konfigurationsfehler).

## 4. Credentials

Der Credential-**Wert** steht in der durch `cred_env` benannten **separaten** Env,
nie im `FIREFLY_SOURCES`-Blob. Das hält die Liste secret-frei und isoliert jedes
Secret.

**`adsb_opensky` (OpenSky OAuth2 Client-Credentials, ADR 0024):** Der Wert ist
`client_id:client_secret`; der Adapter **splittet am ersten `:`** (Client-IDs
enthalten kein `:`) und tauscht das Paar am OAuth2-Token-Endpoint gegen ein
kurzlebiges Bearer-Token (Basic Auth ist von OpenSky abgeschaltet). Kein `cred_env`
→ anonymer Zugang (gedrosseltes Poll-Intervall, ADR 0019). Der **Wire-Vertrag**
(ein String, ein Doppelpunkt) ist unverändert; nur die Bedeutung der zwei Teile.

**`flarm_aprs` (APRS-IS-Login, ADR 0026):** Der Wert ist `callsign:passcode`; der
Adapter **splittet am ersten `:`** (APRS-IS-Callsigns enthalten kein `:`) und meldet
sich damit am APRS-IS-Server an. Kein `cred_env` → **read-only anonymer** Zugang
(Pseudo-Callsign, Passcode `-1`); Firefly **sendet nie**. Ein benannter Account ist
nur nötig, wenn der Betreiber ihn ausdrücklich will. Gleiche Ein-String-mit-einem-
Doppelpunkt-Form wie `adsb_opensky`.

> **Sicherheits-Grenze (ehrlich).** Eine Cred-Env trägt den **Klartext** zur
> Laufzeit (sichtbar in `docker inspect`/Prozess-Env). Wayfinders Verschlüsselung
> schützt das Credential **at rest** in der DB, **nicht** den laufenden Container.
> Die Vertrauensgrenze ist die Netz-/Host-Isolation der Control-Plane (ADR 0012 §6
> dort, ADR 0017 hier).

## 5. Changelog

- **1.5.0** (2026-07-05, ADR 0031) — Neuer Quell-Typ `adsb_aggregator`
  (Community-Aggregator adsb.lol/adsb.fi, auth-frei); neues Feld `provider`?
  (`adsb_lol` Default | `adsb_fi`), `poll_interval_secs` gilt nun auch hier.
  `cred_env` wird für diesen Typ ignoriert. Additiv.
- **1.4.0** (2026-07-02, ADR 0029) — `adsb_opensky` trägt optionales
  `poll_interval_secs` (ganze Sekunden, > 0; fehlt/`0` → Default 10 s). Nur für
  `adsb_opensky`. Additiv — bestehende Quellen unverändert, ältere Leser ignorieren
  das Feld.
- **1.3.0** (2026-06-30, ADR 0028) — `radar_asterix` unterstützt (ASTERIX-CAT048-
  UDP-Listener); neue Felder `lat`/`lon` (Pflicht, Radar-Standort), `height_m`?,
  `listen`? (`group:port`, Default `0.0.0.0:8048`). Kein `cred_env`. Additiv.
- **1.2.0** (2026-06-30, ADR 0026) — `flarm_aprs` unterstützt (OGN/APRS-IS-Adapter);
  Cred-Wert `callsign:passcode`, read-only anonym ohne `cred_env`. Additiv.
- **1.1.0** (2026-06-29, ADR 0024) — `adsb_opensky`-Cred-Wert ist
  `client_id:client_secret` (OAuth2 Client-Credentials); Wire-Vertrag unverändert.
- **1.0.0** (2026-06-29, ADR 0023) — Erstdefinition.
