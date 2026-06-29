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
| `bbox` | object | Flächenquellen | `adsb_opensky`, `flarm_aprs` | `{min_lat, min_lon, max_lat, max_lon}` (WGS84, Grad). |
| `sac` / `sic` | int 0..255 | Radar | `radar_asterix` | Sensor-Identität. |
| `sensor_id` | int | optional | `adsb_opensky` | Auf die Plots gestempelte `SensorId`. Fehlt → Firefly vergibt einen Default je Quell-Index. |
| `cred_env` | string | optional | quellenabhängig | **Name** der Env, die den Credential-Klartext trägt (Abschnitt 4) — **nie** der Wert selbst. Fehlt → anonymer/credential-loser Zugang. |

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
| `adsb_opensky` | **unterstützt** (ADR 0019) | OpenSky-REST-Poller | `bbox` (Pflicht), `sensor_id`?, `cred_env`? |
| `flarm_aprs` | **reserviert** (Adapter folgt) | OGN/APRS-IS | `bbox` |
| `radar_asterix` | **reserviert** (Adapter folgt) | ASTERIX-Eingang | `sac`/`sic` |

**Behandlung:**
- Ein **reservierter** Typ ohne Adapter → **WARN-Log + überspringen** (die Instanz
  dient die Quellen, die sie kann; Verfügbarkeit vor Vollständigkeit).
- Ein **unbekannter** (vokabular-fremder) Typ → **Startfehler** (Konfigurationsfehler).

## 4. Credentials

Der Credential-**Wert** steht in der durch `cred_env` benannten **separaten** Env,
nie im `FIREFLY_SOURCES`-Blob. Das hält die Liste secret-frei und isoliert jedes
Secret.

**`adsb_opensky` (OpenSky Basic-Auth):** Der Wert ist `benutzer:passwort`; der
Adapter **splittet am ersten `:`**. Kein `cred_env` → anonymer Zugang (gedrosseltes
Poll-Intervall, ADR 0019).

> **Sicherheits-Grenze (ehrlich).** Eine Cred-Env trägt den **Klartext** zur
> Laufzeit (sichtbar in `docker inspect`/Prozess-Env). Wayfinders Verschlüsselung
> schützt das Credential **at rest** in der DB, **nicht** den laufenden Container.
> Die Vertrauensgrenze ist die Netz-/Host-Isolation der Control-Plane (ADR 0012 §6
> dort, ADR 0017 hier).

## 5. Changelog

- **1.0.0** (2026-06-29, ADR 0023) — Erstdefinition.
