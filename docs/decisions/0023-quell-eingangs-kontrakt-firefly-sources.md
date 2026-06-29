# ADR 0023 — Quell-Eingangs-Kontrakt (`FIREFLY_SOURCES`, env-getrieben)

- **Status:** akzeptiert
- **Datum:** 2026-06-29
- **Schnittstellen-relevant:** ja — **neuer Eingangs-Kontrakt** (Orchestrator →
  Firefly-Instanz). Der **Ausgabe**-Vertrag (CAT062/UDP, ICD) bleibt unberührt.
  Eigene versionierte Doku: `docs/source-input-contract.md` (v1.0.0).
- **Auslöser:** Wayfinder-Issue [#35](https://github.com/manuelringwald/firefly/issues/35)
  (`from-wayfinder`) — Wayfinders Auto-Orchestrierung (ADR 0012 dort) fährt eine
  Firefly-Instanz pro Feed und muss ihr sagen, **woraus** sie ihre Tracks rechnet.

## Kontext

Firefly kann **live** tracken (ADR 0020): `FIREFLY_MODE=live` startet einen
langlebigen Tracker, gespeist über einen `mpsc`-Kanal von Sensor-Adaptern. Heute
existiert genau **ein** Live-Adapter — OpenSky-ADS-B (ADR 0019), konfiguriert über
diskrete `FIREFLY_OPENSKY_*`-Envs (BBox, Poll-Intervall, User/Pass, Sensor-ID).

Der Wayfinder-Orchestrator hat aber ein **generisches Quell-Modell**: ein Feed
trägt eine **Liste** von Quellen (`adsb_opensky` / `flarm_aprs` / `radar_asterix`),
jede mit BBox bzw. SAC/SIC und optionaler Credential-Referenz. Damit eine
orchestrierte Firefly-Instanz daraus rechnen kann, braucht es einen **stabilen,
versionierten Eingangs-Kontrakt**: *wie* gibt der Orchestrator die Quell-Liste an
die Instanz, inklusive Credentials.

Die diskreten `FIREFLY_OPENSKY_*`-Envs reichen dafür nicht: sie kodieren *eine*
OpenSky-Quelle, nicht eine **Liste** heterogener Quellen je Instanz.

## Entscheidung

### 1. Form: eine JSON-Liste `FIREFLY_SOURCES`

Der Orchestrator setzt eine **JSON-Array-Env** `FIREFLY_SOURCES`, ein Eintrag je
Quelle. Begründung gegen indizierte Flach-Envs (`FIREFLY_SOURCE_0_*`):

- **Pass-through statt zweier Übersetzungs-Schichten.** Wayfinders `source_config`
  *ist* bereits ein JSON-Array mit genau diesen Feldern; der Orchestrator ersetzt
  nur die Credential-Referenz durch einen Cred-Env-Namen und serialisiert.
  Firefly liest mit `serde_json` direkt in ein `Vec<SourceSpec>`. **Beide Seiten
  nutzen ihr natives JSON-Tooling** statt handgeschriebener Flatten/Unflatten-Logik,
  die auseinanderdriften kann.
- **Verschachtelung & Typsicherheit:** BBox ist ein Objekt (gleiche Feldnamen wie
  Wayfinders Modell, `min_lat`/`min_lon`/`max_lat`/`max_lon`); `serde` liefert
  Parsing, Fehlermeldungen und Vorwärtskompatibilität (unbekannte Felder werden
  ignoriert).
- Die Nachteile von JSON-in-Env (Transparenz in `docker inspect`, Shell-/YAML-
  Escaping) treffen fast nur den **Hand-/Dev-Pfad** — und für den bleibt
  `FIREFLY_OPENSKY_*` als Standalone-Konfiguration erhalten. Im orchestrierten
  Pfad setzt Wayfinder die Env über die Docker-**Go-SDK** (kein Shell → **kein**
  Escaping), maschinen-erzeugt.

### 2. Credentials: isoliert in benannten Cred-Envs, referenziert per Name

Der **Klartext-Wert** eines Credentials steht **nie** im `FIREFLY_SOURCES`-Blob,
sondern in einer **separaten, je Quelle benannten Env** (z. B.
`FIREFLY_SOURCE_0_SECRET`), auf die der Listeneintrag nur per Namen (`cred_env`)
verweist — analog zu Wayfinders `cred_ref` (Handle, nie Wert). So bleibt die Liste
secret-frei (notfalls loggbar), und jedes Secret ist isoliert.

**Format des Cred-Werts (UX-2, abgestimmt mit Wayfinder):** Wayfinders Quell-Modell
hat **eine** Credential-Referenz je Quelle (= **ein** Secret-Wert). OpenSky braucht
zwei Teile; der Wert ist daher ein String mit **einem** Doppelpunkt, und der Adapter
**splittet am ersten `:`**. Die Wayfinder-UI bietet dafür zwei getrennte Felder und
fügt sie vor dem verschlüsselten Speichern zusammen — der kombinierte String berührt
das Backend nur verschlüsselt.

> **Aktualisierung (ADR 0024):** Die zwei Teile sind seit der OpenSky-OAuth2-
> Migration `client_id:client_secret` (nicht mehr `benutzer:passwort`). Der
> **Wire-Vertrag bleibt** (ein String, ein Doppelpunkt, Split am ersten `:`); nur
> die Bedeutung ändert sich. `docs/source-input-contract.md` v1.1.0.

### 3. Live-Schalter

`FIREFLY_MODE=live` bleibt der Schalter, der die Instanz in den Live-Tracker-Modus
versetzt (ADR 0020). `FIREFLY_SOURCES` ist nur im Live-Modus wirksam; im
Replay-Modus wird es ignoriert.

### 4. Quell-Vokabular & Behandlung (noch) nicht unterstützter Typen

Das Vokabular ist geschlossen und spiegelt Wayfinder: `adsb_opensky` (BBox,
optional `cred_env`, optional `sensor_id`), `flarm_aprs` (BBox), `radar_asterix`
(`sac`/`sic`). Heute hat **nur `adsb_opensky`** einen Adapter. Ein
vokabular-gültiger Typ **ohne** Adapter wird beim Start **prominent als WARN
geloggt und übersprungen** — die Instanz dient die Quellen, die sie kann
(Verfügbarkeit vor Vollständigkeit, konsistent mit der Fehlertoleranz des
OpenSky-Pollers). Ein **unbekannter** (vokabular-fremder) Typ ist ein
Konfigurationsfehler (Startfehler). `flarm_aprs`/`radar_asterix` sind im Kontrakt
**reserviert**; ihre Adapter folgen (je eigener ADR).

### 5. Verhältnis zu `FIREFLY_OPENSKY_*`

`FIREFLY_OPENSKY_*` bleibt für den **Standalone-/Dev-Betrieb** erhalten (eine
OpenSky-Quelle ohne Orchestrator). Ist `FIREFLY_SOURCES` gesetzt, hat es **Vorrang**;
die diskreten OpenSky-Envs werden dann nicht zusätzlich ausgewertet (kein
Doppel-Adapter). Diese Präzedenz wird im Kontrakt-Doku und im Code festgehalten.

## Konsequenzen

- **Positiv:** Ein stabiler, versionierter Eingangs-Kontrakt entkoppelt
  Orchestrator und Tracker sauber (kein gemeinsamer Code, nur der Env-Vertrag —
  Spiegel zum CAT062-Ausgabe-Prinzip). Nahezu Pass-through aus Wayfinders
  `source_config`. Mehrere Quellen je Instanz sind ausdrückbar, sobald weitere
  Adapter landen.
- **Negativ / Grenzen:** JSON-in-Env ist im Hand-Pfad weniger transparent (durch
  `FIREFLY_OPENSKY_*`-Fallback entschärft). Env-Variablen sind **kein** Geheimnis-
  Tresor — ein Cred-Env trägt den Klartext zur Laufzeit (sichtbar in
  `docker inspect`/Prozess-Env); die Verschlüsselung schützt nur **at rest** in
  Wayfinders DB, nicht den laufenden Container (ehrliche Grenze, wie ADR 0012 §6
  dort).
- **Folgearbeit (je eigener Schritt):** Firefly — `FIREFLY_SOURCES` parsen +
  Live-Tracker aus *N* Adaptern speisen (Cred-Split, Validierung, Tests). Wayfinder
  — Docker-Backend übersetzt `source_config` → `FIREFLY_SOURCES` + Cred-Injection;
  UI-Zwei-Felder (UX-2). FLARM/APRS- und Radar-ASTERIX-Adapter später.

## Alternativen erwogen

- **Indizierte Flach-Envs** (`FIREFLY_SOURCE_0_TYPE=…`): verworfen — erzwingt
  bespoke Flatten/Unflatten auf beiden Seiten und ad-hoc-Kodierung verschachtelter
  Felder (BBox als Komma-String), ohne realen Vorteil im maschinen-gesetzten Pfad.
- **Credential inline im JSON:** verworfen — würde Secrets in den (potenziell
  geloggten) Listen-Blob ziehen; die benannte Cred-Env isoliert sie.
- **Sofort FLARM/Radar-Adapter mitbauen:** verworfen — der Kontrakt wird zuerst
  ratifiziert; die Adapter sind eigene, größere Häppchen.

## Querverweise

- Kontrakt-Doku (maßgeblich, versioniert): `docs/source-input-contract.md`.
- ADR 0019 (OpenSky-Adapter), ADR 0020 (Live-Tracker-Modus), ADR 0012 §6 in
  Wayfinder (Least-Privilege-Control-Plane, Secret-Isolation).
- Cross-Project: `docs/cross-project/todo-for-wayfinder.md`; Issue #35.
