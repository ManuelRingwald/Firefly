# Firefly — Technisches Handbuch (Betriebsführung)

> **Zielgruppe:** Systembetreiber und Entwickler, die Firefly im laufenden
> Betrieb überwachen, konfigurieren und debuggen.
> Vorausgesetzt wird ein laufendes System (siehe `docs/INSTALLATION.md`).

---

## 1. Alle Umgebungsvariablen

### 1.1 Server-Grundkonfiguration

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_PORT` | u16 | `8080` | TCP-Port des HTTP/WebSocket-Servers |

> **Entfallen (ADR 0030):** `FIREFLY_MODE`, `FIREFLY_SCENE` und `FIREFLY_SPEED`
> — der Replay-/Szenen-Modus wurde ausgebaut; der Server läuft immer als
> quellen-getriebener Live-Tracker (Quellen via `FIREFLY_SOURCES`, ADR 0023,
> oder die Adapter-Envs unten — alle Opt-in). Gesetzte Alt-Variablen werden
> mit Warn-Log ignoriert. Ohne aktive Quelle: leerer Himmel + CAT065-Heartbeat.

### 1.2 CAT062-Multicast-Feed (Wayfinder-Schnittstelle)

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_CAT062_ENABLED` | bool | `false` | Feed aktivieren (`true`/`1`/`yes`). Deaktiviert → kein UDP-Verkehr |
| `FIREFLY_CAT062_GROUP` | IPv4 | `239.255.0.62` | Multicast-Gruppe (muss im Bereich 224.0.0.0–239.255.255.255 liegen) |
| `FIREFLY_CAT062_PORT` | u16 | `8600` | UDP-Port |
| `FIREFLY_CAT062_SAC` | u8 | `25` | System Area Code (I062/010) |
| `FIREFLY_CAT062_SIC` | u8 | `2` | System Identification Code (I062/010) |
| `FIREFLY_SYSTEM_REF_LAT` | f64 | Bbox-Mitte¹ | Breitengrad des System-Referenzpunkts (ADR 0021) — speist Tracking-Frame **und** I062/100-Projektion |
| `FIREFLY_SYSTEM_REF_LON` | f64 | Bbox-Mitte¹ | Längengrad des System-Referenzpunkts |

¹ Default = Mitte der Union-Bounding-Box der konfigurierten Quellen (ADR 0021/
0023). I062/105 (WGS84) ist davon unabhängig und immer absolut.

### 1.3 CAT065-Heartbeat (Feed-Liveness)

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_CAT065_ENABLED` | bool | `true` | Heartbeat aktivieren (nur wirksam wenn CAT062 auch aktiv). Ermöglicht Wayfinder, leeren Himmel von totem Feed zu unterscheiden |
| `FIREFLY_CAT065_PERIOD` | f64 | `1.0` | Heartbeat-Intervall in Wanduhrsekunden |
| `FIREFLY_CAT065_SERVICE_ID` | u8 | `1` | Service-ID in I065/015 |

### 1.4 CAT063-Sensor-Status (Per-Sensor-Liveness)

CAT063 meldet je registriertem Sensor, ob er noch Plots liefert (operationell)
oder ausgefallen ist (degradiert) — damit Wayfinder einen **Sensor-Ausfall** von
einem **leeren Himmel** unterscheidet (ADR 0022, Firefly #32). Ein Block je Tick,
ein Record je Sensor. Seit ICD 3.0.0 (ADR 0032) folgt der Record der echten
EUROCONTROL-CAT063-UAP (FSPEC `0xB8`): I063/010 = **SDPS**-Identität (SAC/SIC wie
I062/010), I063/030 = ToD, I063/050 = **Sensor**-Identität (SAC 0, SIC =
`sensor_id`), I063/060 = CON (operationell/degradiert). Seit ICD 3.1.0 (ADR 0033)
trägt ein **degradierter** Sensor mit bekanntem Grund zusätzlich das **I063/RE**
(FSPEC dann `0xB9 0x04`) mit `SRC-REASON` (`1=unreachable`/`2=auth`/`3=rate_limited`)
— der Grund kommt aus den HTTP-ADS-B-Pollern (`SensorHealthMonitor::record_failure`,
Klassifikation über `PollError::is_rate_limited`/`is_auth`); FLARM/Radar liefern
keinen Grund. Läuft mit, sobald **Feed *und* Heartbeat** aktiv sind — kein eigener
Enable-Schalter.

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_CAT063_PERIOD` | f64 | `5.0` | Intervall der Sensor-Status-Blöcke in Wanduhrsekunden. Langsamer als der Heartbeat, weil Sensor-Liveness sich auf der Skala der Antennenumläufe (4–12 s) ändert |

**Degradiert-Kriterium:** Ein Sensor gilt als aktiv, solange er innerhalb von
`2.5 × scan_period` einen Plot lieferte, sonst degradiert (NOGO `0x40`). Die
Liveness folgt dem echten Plot-Eingang des jeweiligen Quell-Adapters
(OpenSky/FLARM/Radar).

### 1.5 OpenSky Network ADS-B-Adapter

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_OPENSKY_ENABLED` | bool | `false` | Adapter aktivieren. Deaktiviert → kein ausgehender HTTP-Verkehr |
| `FIREFLY_OPENSKY_LAT_MIN` | f64 | `47.0` | Südliche Begrenzung der Bounding Box (Grad) |
| `FIREFLY_OPENSKY_LAT_MAX` | f64 | `55.0` | Nördliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_LON_MIN` | f64 | `5.0` | Westliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_LON_MAX` | f64 | `16.0` | Östliche Begrenzung (Grad) |
| `FIREFLY_OPENSKY_POLL_INTERVAL_SECS` | u64 | `10` | Abfrageintervall in Sekunden (≥ 10 ohne Account, ≥ 5 mit Account) |
| `FIREFLY_OPENSKY_CLIENT_ID` | string | — | OAuth2 Client-ID (optional; ADR 0024). Mit `_CLIENT_SECRET` zusammen → authentifiziert, sonst anonym |
| `FIREFLY_OPENSKY_CLIENT_SECRET` | string | — | OAuth2 Client-Secret (optional; ADR 0024) |
| `FIREFLY_OPENSKY_TOKEN_URL` | string | OpenSky-Keycloak-Realm | OAuth2-Token-Endpoint (Client-Credentials); überschreibbar für Test/Realm-Wechsel |
| `FIREFLY_OPENSKY_SENSOR_ID` | u16 | `200` | Sensor-ID, die ADS-B-Plots im Tracker zugeordnet werden |

> **Standalone-/Dev-Pfad.** Die `FIREFLY_OPENSKY_*`-Variablen konfigurieren **eine**
> OpenSky-Quelle. Im **orchestrierten** Betrieb wird stattdessen `FIREFLY_SOURCES`
> gesetzt (Abschnitt 1.5.1) — dann haben die `FIREFLY_OPENSKY_*`-Variablen **keinen**
> Effekt (Vorrang von `FIREFLY_SOURCES`).

#### Community-Aggregator-ADS-B-Adapter (`FIREFLY_ADSBAGG_*`, ADR 0031)

Auth-freier ADS-B-Bezug über einen ADSBExchange-v2-kompatiblen
Community-Aggregator (adsb.lol, adsb.fi) — der zweite ADS-B-Bezugsweg **neben**
OpenSky, für Umgebungen, in denen OpenSky nicht erreichbar ist (Datacenter-
IP-Sperre) oder kein OAuth2-Client gewünscht ist. Im Standalone-Pfad per
`FIREFLY_ADSBAGG_ENABLED=true` zuschaltbar, orchestriert als
`adsb_aggregator`-Eintrag in `FIREFLY_SOURCES` (Kontrakt v1.5.0). Die
konfigurierte BBox wird als **Umkreis** abgefragt (die APIs sind Punkt+Radius,
max. 250 NM — größere BBoxen werden mit WARN geclampt) und die Antwort auf die
BBox zurückgefiltert. Plots fließen in denselben Tracker (Fusion).

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_ADSBAGG_ENABLED` | bool | `false` | Adapter im Standalone-Live-Modus aktivieren |
| `FIREFLY_ADSBAGG_PROVIDER` | string | `adsb_lol` | Anbieter: `adsb_lol` \| `adsb_fi` (airplanes.live zurückgestellt, ADR 0031) |
| `FIREFLY_ADSBAGG_BASE_URL` | string | — | Basis-URL-Override (Tests, self-hosted Aggregator); leer → öffentliche Basis des Providers |
| `FIREFLY_ADSBAGG_LAT_MIN` / `_MAX` | f64 | `47.0` / `55.0` | Bounding Box Süd/Nord (Grad) |
| `FIREFLY_ADSBAGG_LON_MIN` / `_MAX` | f64 | `5.0` / `16.0` | Bounding Box West/Ost (Grad) |
| `FIREFLY_ADSBAGG_POLL_INTERVAL_SECS` | u64 | `10` | Abfrageintervall (s); öffentliche Endpoints erlauben ~1 req/s — Default bleibt höflich darunter |
| `FIREFLY_ADSBAGG_SENSOR_ID` | u16 | `230` | Sensor-ID der Aggregator-Plots |

> **Sicherheit/Qualität:** Crowdgesourcte, unauthentifizierte Community-Daten
> ohne SLA (gleiche Vertrauensgrenze wie OpenSky, ADR 0017/0019); adsb.lol-Daten
> stehen unter ODbL. Keine zertifizierte Surveillance-Quelle.

#### FLARM/OGN-Adapter (`FIREFLY_FLARM_*`, ADR 0026)

Zweiter Live-Quell-Adapter: FLARM-Positionen über das Open Glider Network via
APRS-IS. Im Live-Modus per `FIREFLY_FLARM_ENABLED=true` zuschaltbar (standalone)
oder als `flarm_aprs`-Eintrag in `FIREFLY_SOURCES` (orchestriert). Plots fließen in
denselben Tracker wie OpenSky (Fusion).

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_FLARM_ENABLED` | bool | `false` | Adapter im Standalone-Live-Modus aktivieren |
| `FIREFLY_FLARM_LAT_MIN` / `_MAX` | f64 | `47.0` / `55.0` | Bounding Box Süd/Nord (Grad) → APRS-IS-Area-Filter |
| `FIREFLY_FLARM_LON_MIN` / `_MAX` | f64 | `5.0` / `16.0` | Bounding Box West/Ost (Grad) |
| `FIREFLY_FLARM_SERVER` | string | `aprs.glidernet.org` | APRS-IS-Server-Host |
| `FIREFLY_FLARM_PORT` | u16 | `14580` | APRS-IS-Port (Filter-Feed) |
| `FIREFLY_FLARM_CALLSIGN` | string | — | APRS-IS-Login-Callsign (fehlt → read-only anonym) |
| `FIREFLY_FLARM_PASSCODE` | i32 | `-1` | APRS-IS-Passcode (`-1` = read-only) |
| `FIREFLY_FLARM_SENSOR_ID` | u16 | `210` | Sensor-ID der FLARM-Plots |
| `FIREFLY_FLARM_SIGMA_M` | f64 | `20.0` | 1σ-Positionsgenauigkeit (m), isotrop |
| `FIREFLY_FLARM_RECONNECT_MIN_SECS` / `_MAX_SECS` | u64 | `5` / `300` | Reconnect-Backoff (min/max) |

> **Sicherheit:** APRS-IS-Daten sind öffentlich und nicht authentifiziert; Firefly
> sendet nie (read-only). Vertrauensgrenze = Netz-/Quellen-Isolation (ADR 0017).

#### Radar-ASTERIX-Adapter (`FIREFLY_RADAR_*`, ADR 0028)

Dritter Live-Quell-Adapter: ein realer Monoradar über **ASTERIX CAT048 über UDP**.
Im Live-Modus per `FIREFLY_RADAR_ENABLED=true` zuschaltbar (standalone) oder als
`radar_asterix`-Eintrag in `FIREFLY_SOURCES` (orchestriert). Liefert **Polar-Plots**
(`Measurement::Polar`); der Sensor wird mit seinem **eigenen Standort-Frame** +
realem Polar-Fehlermodell registriert (anders als die geodätischen Adapter). Plots
fließen in denselben Tracker (Fusion mit ADS-B/FLARM).

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_RADAR_ENABLED` | bool | `false` | Adapter im Standalone-Live-Modus aktivieren |
| `FIREFLY_RADAR_SAC` / `_SIC` | u8 | `0` / `0` | Erwartete Radar-Identität (I048/010) |
| `FIREFLY_RADAR_LAT` / `_LON` | f64 | `0.0` / `0.0` | **Radar-Standort** (Grad) — CAT048 trägt ihn nicht |
| `FIREFLY_RADAR_HEIGHT_M` | f64 | `0.0` | Radar-Standort-Höhe (m über WGS84-Ellipsoid) |
| `FIREFLY_RADAR_GROUP` | IPv4 | `0.0.0.0` | Listen-Adresse: Multicast-Gruppe → beigetreten, sonst Unicast-Bind |
| `FIREFLY_RADAR_PORT` | u16 | `8048` | UDP-Port des ASTERIX-Eingangs |
| `FIREFLY_RADAR_SENSOR_ID` | u16 | `220` | Sensor-ID der Radar-Plots |
| `FIREFLY_RADAR_SCAN_SECS` | f64 | `4.0` | Antennen-Umlaufzeit (Revisit-Budget, CAT063-Staleness) |
| `FIREFLY_RADAR_SIGMA_RANGE_M` | f64 | `50.0` | 1σ-Schrägentfernungs-Rauschen (m) |
| `FIREFLY_RADAR_SIGMA_AZ_DEG` | f64 | `0.1` | 1σ-Azimut-Rauschen (Grad) |

> **Sicherheit:** ASTERIX-UDP ist nicht authentifiziert; der Decoder ist robust
> (kein Panic auf Eingabe, length-checked, Mutations-/Trunkierungs-getestet).
> Vertrauensgrenze = Netz-/Quellen-Isolation (ADR 0017).

**CAT034-Servicemeldungen (FEP.1):** Derselbe UDP-Eingang verarbeitet zusätzlich
**CAT034** (Nordmarke, Sektor-Meldungen; Dispatch am führenden CAT-Oktett,
`0x22` = 34 / `0x30` = 48). Wirkung: (1) **Gemessene Scan-Periode** — aus den
Nordmarken-Intervallen (geglättet, Ausreißer-/Mitternachts-tolerant) wird die
echte Antennen-Umlaufzeit bestimmt; sie ersetzt den konfigurierten Nominalwert
als **CAT063-Staleness-Basis** (`2,5 ×` gemessene Periode) und erscheint als
Metrik `firefly_radar_scan_period_seconds{sensor=…}`. (2) **Liveness ohne
Verkehr** — jede Servicemeldung zählt als Sensor-Aktivität: „leerer Himmel"
und „totes Radar" sind eingangsseitig unterscheidbar. Sendet der Radarkopf
keine Servicemeldungen, bleibt alles beim konfigurierten Verhalten
(`FIREFLY_RADAR_SCAN_SECS`). Die Tracker-Löschkadenz bleibt in FEP.1 bewusst
beim Konfigurationswert. Der CAT034-Decoder ist gefuzzt (`cat034_decode`).

**Mode-S-DAPs (FEP.2):** Liefert das Radar in CAT048 **I048/250** die
EHS-Register BDS 4,0/5,0/6,0 mit, dekodiert Firefly die **Downlink Aircraft
Parameters** (Selected Altitude, Heading, IAS/Mach, Roll, …; nur Felder mit
gesetztem Status-Bit) und reicht MHG/SAL/IAR/MAC — solange frisch (≤ 30 s) —
im CAT062 **I062/380** weiter (ICD 3.4.0, additiv). Kein Schalter nötig; ohne
EHS-Daten ändert sich nichts am Draht.

**Legacy-Radare CAT001/CAT002 (FEP.4):** Derselbe UDP-Eingang versteht auch
die **Vorgänger-Generation** von CAT048/CAT034 — ein Legacy-Radarkopf wird
unverändert als `radar_asterix`-Quelle konfiguriert (keine neuen Variablen),
der Listener verzweigt am führenden CAT-Oktett (`0x01`/`0x02`). CAT001-Records
tragen nur eine **trunkierte** Tageszeit (mod 512 s); der Listener ankert sie
am letzten vollen ToD des Service-Stroms (CAT002/CAT034). **Bis zur ersten
Servicemeldung mit Zeit werden Legacy-Plots verworfen** statt mit erfundener
Zeit versehen — ein Legacy-Radar sollte also CAT002 mitsenden (das tun reale
Köpfe; die Nordmarken speisen zugleich die gemessene Scan-Periode, FEP.1).
Simulierte Meldungen (SIM-Bit) gelangen nie ins Luftlagebild. Beide Decoder
sind gefuzzt (`cat001_decode`/`cat002_decode`).

#### ADS-B-Bodenstations-Adapter (`FIREFLY_ADSB021_*`, FEP.3)

Vierter Live-Quell-Adapter: eine **eigene ADS-B-Bodenstation** über **ASTERIX
CAT021 über UDP** — der Produktions-Bezugsweg für ADS-B (Push statt Poll,
lokal statt Internet-REST). Standalone per `FIREFLY_ADSB021_ENABLED=true`
oder als `adsb_asterix`-Eintrag in `FIREFLY_SOURCES` (Kontrakt v1.6.0).
Liefert **geodätische Plots** (WGS84-Selbstmeldungen); die Messunsicherheit
wird **je Meldung** aus dem **NACp**-Qualitätsindikator abgeleitet
(DO-260B, σ ≈ EPU/2; ohne/mit NACp 0 → konservative 250 m — bewusst
schlechter als die 75-m-Annahme der Internet-Quellen). Boden-, Simulations-
und Testziele (GBS/SIM/TST in I021/040) werden **verworfen** und gelangen
nie ins Luftlagebild.

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_ADSB021_ENABLED` | bool | `false` | Adapter im Standalone-Live-Modus aktivieren |
| `FIREFLY_ADSB021_SAC` / `_SIC` | u8 | `0` / `0` | Erwartete Stations-Identität (I021/010) |
| `FIREFLY_ADSB021_GROUP` | IPv4 | `0.0.0.0` | Listen-Adresse: Multicast-Gruppe → beigetreten, sonst Unicast-Bind |
| `FIREFLY_ADSB021_PORT` | u16 | `8021` | UDP-Port des CAT021-Eingangs |
| `FIREFLY_ADSB021_SENSOR_ID` | u16 | `230` | Sensor-ID der ADS-B-Stations-Plots |

> **Hinweis Referenzpunkt:** `adsb_asterix` trägt keine bbox und keinen
> Standort zum System-Referenzpunkt bei. Ist es die **einzige** Quelle,
> `FIREFLY_SYSTEM_REF_*` setzen (ADR 0021), sonst liegt der Tracking-Frame-
> Ursprung auf dem Default.
>
> **Sicherheit:** wie beim Radar — ASTERIX-UDP ist nicht authentifiziert; der
> CAT021-Decoder ist robust (kein Panic auf Eingabe, gefuzzt:
> `cat021_decode`), Vertrauensgrenze = Netz-Isolation (ADR 0017). Der Decoder
> erwartet die **Edition-2.x-UAP**; eine 0.26-Station scheitert laut (Decode-
> Fehler im Log) statt still falsch zu dekodieren.

#### WAM/MLAT-Adapter (`FIREFLY_MLAT_*`, FEP.5)

Fünfter Live-Quell-Adapter: ein **WAM/Multilaterations-System** über
**ASTERIX CAT020 (Zielmeldungen) + CAT019 (Systemstatus) über UDP** —
unabhängige kooperative Überwachung neben Radar und ADS-B (das Bodensystem
berechnet die Position aus Laufzeitdifferenzen; das Flugzeug kann sie nicht
selbst fälschen). Standalone per `FIREFLY_MLAT_ENABLED=true` oder als
`mlat_asterix`-Eintrag in `FIREFLY_SOURCES` (Kontrakt v1.7.0). Liefert
**geodätische Plots**; die Messunsicherheit kommt **je Meldung** aus
**I020/500 SDP** (Standardabweichung der Positionslösung; fehlend →
konservative 150 m). Feldmonitor- (RAB), Simulations-/Test- (SIM/TST) und
Bodenziele (GBS) werden **verworfen**. CAT019-Statusmeldungen zählen als
Sensor-Aktivität (Liveness ohne Verkehr); meldet sich das System selbst
degradiert/NOGO, wird gewarnt.

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_MLAT_ENABLED` | bool | `false` | Adapter im Standalone-Live-Modus aktivieren |
| `FIREFLY_MLAT_SAC` / `_SIC` | u8 | `0` / `0` | Erwartete System-Identität (I020/010) |
| `FIREFLY_MLAT_GROUP` | IPv4 | `0.0.0.0` | Listen-Adresse: Multicast-Gruppe → beigetreten, sonst Unicast-Bind |
| `FIREFLY_MLAT_PORT` | u16 | `8020` | UDP-Port des CAT020/019-Eingangs |
| `FIREFLY_MLAT_SENSOR_ID` | u16 | `240` | Sensor-ID der WAM/MLAT-Plots |

> **Hinweise:** wie `adsb_asterix` — kein Standort/bbox nötig (geodätische
> Positionen); als **einzige** Quelle `FIREFLY_SYSTEM_REF_*` setzen
> (ADR 0021). ASTERIX-UDP unauthentifiziert → Netz-Isolation (ADR 0017);
> beide Decoder gefuzzt (`cat020_decode`/`cat019_decode`). **Provenienz:**
> MLAT-Plots erscheinen in I062/290 als **Mode S** (die zugrundeliegende
> Technologie); ein eigenes MLT-Age-Subfeld wäre ein ICD-Bump und ist
> bewusst ein Folge-Häppchen.

#### Meteo/QNH-Dienst (`FIREFLY_METEO_QNH`, VERT.1)

Der SDPS-003-Baustein: **regionale QNH-Werte** für die Vertikal-Kette.
Mode-C/Flugflächen sind Druckhöhen (Referenz 1013,25 hPa); unterhalb der
Transition Altitude braucht die wahre Höhe das lokale **QNH** (~27–30 ft
Fehler pro hPa). Der Dienst hält eine Menge von QNH-Regionen und liefert per
Positions-Lookup das anwendbare QNH; ohne anwendbare Region antwortet er
**ehrlich gekennzeichnet** mit der Standardatmosphäre (nie ein erfundenes
QNH). Die Verwertung im Höhen-Tracking (QNH-korrigierte Höhe → I062/135)
ist VERT.2.

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_METEO_QNH` | JSON-Array | — (leer) | QNH-Regionen: `[{"name":"EDDF","lat":50.03,"lon":8.57,"radius_nm":60,"qnh_hpa":1008}, …]`. `radius_nm` optional (fehlt = unbegrenzt, konkurriert nur über Nähe); `qnh_hpa` muss im Plausibilitätsband [870, 1085] liegen. Malformes JSON oder implausible Werte → **Start-Abbruch**. Unset/leer = Standardatmosphäre überall (INFO im Log). |

> **Betrieb:** Der env-getriebene Provider ist bewusst der erste Schritt —
> der Betreiber (oder Wayfinders Orchestrator) setzt die Werte und
> aktualisiert sie extern im Wetter-Zyklus. Ein Live-Provider (periodischer
> METAR-Abruf) braucht eine Netz-Freigabe-Entscheidung des Deployments und
> einen eigenen ADR (Folge-Häppchen).
>
> **Verwertung (VERT.2):** Jeder Track führt einen Vertikal-Filter (Höhe +
> Steig-/Sinkrate aus den Mode-C-Meldungen, Ausreißer-Gating) und eine
> getrennte geometrische Höhe (nur aus echt geometrischen Quellen: ADS-B
> I021/140, MLAT I020/105). Auf dem Draht: **I062/135** (barometrisch,
> QNH-Bit gesetzt nur bei Korrektur auf ein **beobachtetes** regionales
> QNH), **I062/130** (geometrisch), **I062/220** (RoCD) — je nur bei
> frischem Schätzwert (≤ 30 s), ICD 3.5.0 additiv.

#### Flugplan-Eingang + Korrelation (`FIREFLY_FLIGHT_PLANS`, FPL.1)

Der FPL-Baustein (ADR 0038): Firefly korreliert System-Tracks **zentral**
mit gefileten Flugplänen — eine Zuordnung für alle Konsumenten, statt dass
jedes Display selbst rät. Regeln (Lektion Weeze — ein falsches Label ist
schlimmer als ein fehlendes): **Callsign zuerst** (normalisiert);
**Squawk nur als Fallback**, wenn eindeutig unter allen Plänen, der Track
keinen `identity_conflict` trägt, der Code nicht Conspicuity 1000 ist und
die Erwartungszeit plausibel liegt (±45 min); jede Verweigerung zählt
sichtbar (`firefly_correlation_refused`). Die Korrelation läuft
**je Output-Tick** am Ausgabe-Rand (nach der QNH-Korrektur); der
Tracker-Kern bleibt flugplan-frei. Ergebnis auf dem WS-JSON:
`SystemTrack.flight_plan` (`{callsign, departure?, destination?}`) und
`SystemTrack.identity_conflict` — beide **additiv** — und seit FPL.2 auf
dem CAT062-Draht als **I062/390** (FRN 21: CSN/DEP/DST, ICD 3.7.0,
additiv; nur bei korreliertem Track).

**Manuelle Korrelation (FPL.2, ADR 0039):** Der Lotse (via Wayfinder)
kann die Zuordnung übersteuern — manuell schlägt Automatik:

| Endpunkt | Wirkung |
|----------|---------|
| `POST /correlation` (JSON `{"track_number": N, "callsign": "DLH123"}`) | pinnt den Plan auf den Track (422 bei unbekanntem Callsign) |
| `POST /correlation` (JSON `{"track_number": N}`) | pinnt den Track auf **unkorreliert** — die Automatik darf das entfernte Label nicht wieder anbringen |
| `DELETE /correlation/{N}` | Pin löschen, Automatik übernimmt wieder (idempotent) |
| `GET /correlation` | aktuelle Pins auflisten |

Ohne konfigurierte Flugpläne antworten Mutationen **409**. Auth: dasselbe
Token wie `/ws` (`FIREFLY_WS_TOKEN`), aber **nur** als
`Authorization: Bearer`-Header (kein Query-Fallback — Query-Strings landen
in Logs); Origin-Check nur bei mitgesendetem `Origin`-Header. **Pins
sterben mit dem TSE ihres Tracks** (Draht-Nummern werden wiederverwendet)
und sind **flüchtig** (Neustart verliert sie; Persistenz = HA.1).

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_FLIGHT_PLANS` | JSON-Array | — (leer) | Gefilete Flugpläne: `[{"callsign":"DLH123","squawk":1234,"departure":"EDDF","destination":"EDDM","expected_time":1752580800}, …]`. Nur `callsign` Pflicht (Duplikate → **Start-Abbruch**). `squawk` **oktal wie geschrieben** (Zahl `1234` oder String `"1234"` = Oktal 1234; Ziffer 8/9 → **Start-Abbruch**). `expected_time` = Unix-Sekunden (Fensterzentrum). Malformes JSON → **Start-Abbruch**; unset/leer = keine Flugpläne (INFO). |

> **Betrieb:** Wie beim Meteo-Dienst ist der env-getriebene Provider bewusst
> der erste Schritt; eine Live-FDPS-Anbindung (Datei/Netz, Reload) braucht
> einen eigenen ADR. Der Feldsatz wächst **additiv** nach dem
> EFS-Feedback aus Wayfinder #244.

#### Zustands-Snapshot + Wiederanlauf (`FIREFLY_SNAPSHOT_*`, HA.1)

Der SDPS-002-Baustein (ADR 0040): Der Live-Tracker sichert seinen
Arbeitszustand (Tracks, IMM-Filterzustände, Track-Nummern-Pool,
Clutter-Karten, letzte Datenzeit, manuelle Korrelations-Pins) periodisch
als versioniertes JSON und liest ihn beim Start wieder ein — das Bild ist
nach einem Neustart binnen eines Output-Ticks zurück statt nach Minuten
der Neu-Bestätigung. Geschrieben wird **atomar** (`.tmp` + fsync +
rename); Schreibfehler sind nicht fatal (WARN + Zähler, Wiederversuch).
Restore nur, wenn **Format-Version**, **Konfigurations-Fingerprint**
(Referenzpunkt + vollständige Sensor-Liste — ein Zustand für eine andere
`FIREFLY_SOURCES`-Konfiguration wird nie wiederbelebt) **und Alter**
passen; jede Ablehnung ist ein lautes WARN mit Grund, dann leerer Start.
`/ready` bleibt an den ersten Quell-Plot gekoppelt.

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_SNAPSHOT_PATH` | Pfad | — (aus) | Snapshot-Datei (K8s: persistentes Volume!). Unset/leer = keine Snapshots. |
| `FIREFLY_SNAPSHOT_PERIOD` | Sekunden > 0 | 10 | Schreib-Kadenz (wall-clock, geprüft je Output-Tick). Malform → **Start-Abbruch**. |
| `FIREFLY_SNAPSHOT_MAX_AGE` | Sekunden > 0 | 300 | Maximales Snapshot-Alter beim Restore — älter wird laut verworfen (veralteter Verkehr ist gefährlicher als ein leerer Start). Malform → **Start-Abbruch**. |

> **Ehrliche Grenzen:** Plots zwischen letztem Snapshot und Absturz sind
> verloren (Fenster ≤ Periode; forensisches Replay = `.ffplots`,
> ADR 0020); Metrik-Zählerstände starten bei 0; Main/Standby = HA.2.

#### Main/Standby + Heartbeat-Failover (`FIREFLY_ROLE`, HA.2a)

Der zweite SDPS-002-Baustein (ADR 0041): Eine **Standby-Instanz** läuft
als warm spare mit — Probes-only (`/ready` = 503 „standby"), kein Senden
(eine SDPS-Identität, ein Sender), keine Quellen (kein doppeltes
Rate-Limit-Budget). Sie beobachtet den **CAT065-Heartbeat der eigenen
SAC/SIC** auf der Multicast-Gruppe (das Liveness-Signal aus ADR 0018 —
kein externer Koordinator); ein NOGO-Heartbeat zählt als lebendig.
Verstummt der Heartbeat länger als der Failover-Timeout, **promotet**
sich der Standby: voller Live-Stack inkl. HA.1-Snapshot-Restore vom
gemeinsamen Volume (gleiche Track-Nummern/Identitäten/Pins); der eigene
Heartbeat startet erst nach der Promotion.

| Variable | Typ | Default | Bedeutung |
|----------|-----|---------|-----------|
| `FIREFLY_ROLE` | `main` \| `standby` | `main` | Instanz-Rolle. Unbekannter Wert → **Start-Abbruch**. `standby` verlangt `FIREFLY_CAT062_ENABLED=true` + CAT065-Heartbeat (sonst **Start-Abbruch**). |
| `FIREFLY_FAILOVER_TIMEOUT` | Sekunden > 0 | 3 | Heartbeat-Stille bis zur Übernahme (3 = drei verpasste 1-s-Heartbeats). Die Uhr läuft ab Standby-Start — ein beim Start schon toter Main wird einen Timeout später übernommen. Malform → **Start-Abbruch**. |

**Split-Brain-Schutz (HA.2b):** (1) **Startup-Arbitrierung** — ein
`main` lauscht vor dem ersten Senden einen Failover-Timeout lang; ein
fremder Heartbeat der eigenen Identität ⇒ Standby statt Doppel-Feed
(Kaltstart-Latenz +1 Timeout; fail-open bei Socket-Fehler). (2)
**Laufzeit-Demotion** — die aktive Instanz beobachtet die Gruppe weiter;
bei Split-Brain weicht deterministisch die Seite mit der **höheren
Absender-Adresse** und beendet sich mit **Exit-Code 3** (crash-only:
der Supervisor-Neustart re-arbitriert in den Standby — **eine
Restart-Policy ist Betriebs-Voraussetzung**). Eigene Loopback-Heartbeats
werden über Egress-IP + Heartbeat-Socket-Port erkannt; bei unbestimmbarer
Selbst-Adresse bleibt die Wache aus (laut geloggt).

> **Ehrliche Grenzen (ADR 0041):** Timeout-Detektion, **kein Konsens** —
> während einer echten Netz-Partition senden beide Seiten, bis die
> Partition heilt und die Demotion greift. Übernahme-Bild = letzter
> Snapshot (Verlustfenster ≤ Snapshot-Periode + Timeout); gemeinsames
> Volume ist Deployment-Sache; Multi-homed-Hosts können die
> Eigen-Erkennung täuschen (dokumentierte Restlücke).

> **Deployment (HA.3):** Das geprüfte Kubernetes-Rezept für das
> Main/Standby-Paar (Helm-Chart + statisches Manifest, geteilte
> ConfigMap, RWX-Snapshot-PVC, Readiness-Routing über einen Service,
> `hostNetwork` + Anti-Affinity für Multicast) liegt unter `deploy/` —
> Begründungen in `deploy/README.md`, Installation in
> `docs/INSTALLATION.md` §6a.

#### Laufzeit-Steuerung — Sensor an/aus + Supervision (`/sensors`, `/status`, SRV.2)

Der SNMP-/CMD-Ersatz aus der ARTAS-Roadmap (FR-OPS-008): Der Betreiber
kann einen störenden Sensor (defektes Radar mit systematisch falschen
Positionen, müllflutende ADS-B-Quelle) **zur Laufzeit aus der Fusion
nehmen** — und wieder hereinholen — ohne Neustart und ohne
Lagebild-Unterbrechung für die übrigen Quellen. Ein deaktivierter Sensor
wird **am Eingang verworfen**, bevor seine Plots Aufzeichnung,
Registrierung oder Tracker erreichen. Keine neuen Env-Variablen; Auth wie
bei `/correlation` (Bearer-Header-only, `FIREFLY_WS_TOKEN`).

| Endpunkt | Wirkung |
|----------|---------|
| `GET /sensors` | Sensor-Inventar: `[{sensor_id, kind, active, disabled}]` — konfigurierte Quelle, CAT063-Liveness, Gate-Zustand |
| `POST /sensors/{id}/disable` | Sensor aus der Fusion nehmen (422 bei unbekannter Sensor-ID, 409 auf einem Standby); idempotent, Antwort meldet `changed` |
| `POST /sensors/{id}/enable` | Sensor wieder hereinnehmen (idempotent) |
| `GET /status` | Supervision-Übersicht als JSON: Rolle (active/standby), Readiness, Restore-Flag, Failover-Zähler, Track-/Plot-Zähler, Sensoren (gesamt/aktiv/deaktiviert + verworfene Plots), Korrelation, Snapshot-Buchführung, JPDA-Kappen-Zähler |

**Bewusste Eigenschaften (ehrlich dokumentiert):**

- **Flüchtig, fail-open:** Neustart/Failover startet mit **allen Sensoren
  aktiv**. Ein vergessenes, still persistiertes Gate, das das Lagebild
  dauerhaft ausdünnt, ist der schlimmere Fehlerfall als ein nach Neustart
  wieder aktiver Störsensor (der erneut deaktiviert werden kann — der
  WARN-Log und `firefly_sensors_disabled` machen den Zustand sichtbar).
- **Pro Instanz:** Ein Standby übernimmt das Gate **nicht** — nach einem
  Failover sind alle Sensoren aktiv (gleiche fail-open-Logik).
- **CAT063 bleibt Quell-Wahrheit:** Der Sensor-Status auf dem Draht meldet
  weiterhin, ob die Quelle **Daten liefert** — ein manuell deaktivierter
  Sensor sendet ja weiter und bleibt dort „active". Der Gate-Zustand ist
  in `/sensors`, `/status` und den Metriken sichtbar, nicht im
  CAT063-Strom (kein stiller Semantik-Wechsel des ICD).

### 1.5.1 Quell-Eingangs-Kontrakt (`FIREFLY_SOURCES`, ADR 0023)

Maßgeblich: `docs/source-input-contract.md` v1.7.0. Im **Live-Modus** liest Firefly
seine Quellen aus einer JSON-Liste, die ein Orchestrator (Wayfinder) je Instanz
setzt — ein Eintrag je Quelle, mehrere Adapter speisen denselben Live-Tracker.

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_SOURCES` | JSON-Array | — | Quell-Liste. Gesetzt → **Vorrang** vor `FIREFLY_OPENSKY_*`/`FIREFLY_ADSBAGG_*`/`FIREFLY_FLARM_*`/`FIREFLY_RADAR_*`/`FIREFLY_ADSB021_*`/`FIREFLY_MLAT_*`. Eintrag: `{type, bbox?, provider?, sac?, sic?, sensor_id?, cred_env?, lat?, lon?, height_m?, listen?, poll_interval_secs?}`. `type` ∈ `adsb_opensky` / `adsb_aggregator` / `flarm_aprs` / `radar_asterix` / `adsb_asterix` / `mlat_asterix` (alle unterstützt). `provider` (nur `adsb_aggregator`): `adsb_lol` (Default) \| `adsb_fi`; unbekannt → **Start-Abbruch**. `poll_interval_secs` (`adsb_opensky`/`adsb_aggregator`, `> 0`; fehlt/`0` → Default 10 s, ADR 0029/0031) überschreibt das Poll-Intervall. Unbekannter `type` oder malformes JSON → **Start-Abbruch**. |
| `FIREFLY_SOURCE_<n>_SECRET` o. ä. | string | — | Beliebig **benannte** Credential-Env, von einem Eintrag per `cred_env` referenziert. Wert quellenabhängig: `client_id:client_secret` (`adsb_opensky`) bzw. `callsign:passcode` (`flarm_aprs`), Split am ersten `:`; nie im JSON-Blob. |

Beispiel: siehe `docs/source-input-contract.md` §2. Referenzpunkt = Mittelpunkt der
**Union** aller Quell-BBoxen (`FIREFLY_SYSTEM_REF_*` überschreibt); Ausgabe-Takt =
**min** Poll-Intervall der Quellen. Jede Quelle stempelt ihre `sensor_id` auf ihre
Plots; die Sensor-Liveness (CAT063) verfolgt alle Quellen.

### 1.5.2 Sensor-Registrierung — Monitor & Korrektur (REG.2a/2b, ADR 0034)

Der Registrierungs-Monitor beobachtet den Live-Plot-Strom, paart
Radar-Messungen mit geodätischer Referenz (ADS-B/FLARM) bzw. anderen Radaren
über die ICAO-Adresse und schätzt periodisch die systematischen Messfehler
(Range-/Azimut-Bias) jedes Radars. Sinnvoll nur mit mindestens einer
`radar_asterix`-Quelle; ohne Radar ist der Monitor ein No-op (Warn-Log).

**Zwei getrennte Schalter** — das Schließen eines Regelkreises in den
Fusionspfad wird ausdrücklich aktiviert, nie impliziert:

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_REGISTRATION_ENABLED` | bool | *(aus)* | `1`/`true`/`yes` (case-insensitiv) aktiviert den **Monitor** (Schätzung). Feste Parameter (REG.2a): 120 s Gleitfenster (Datenzeit), 1 s Pairing-Fenster, Schätz-Kadenz 10 s, min. 20 Korrespondenzen je Lauf. Ohne `_APPLY` reiner Schattenmodus: nur Logs + Metriken. |
| `FIREFLY_REGISTRATION_APPLY` | bool | *(aus)* | `1`/`true`/`yes` aktiviert zusätzlich die **Korrektur** (REG.2b): Die geschätzten Biases werden vor der Fusion von den Radar-Messungen abgezogen. Erfordert den laufenden Monitor (sonst Warn-Log, No-op). |

**Anwendungs-Politik (REG.2b, fest):** Eine Schätzung wird nur übernommen,
wenn sie **beobachtbar** ist, die Residuen **signifikant erklärt**
(RMS nachher ≤ 0,5 × RMS vorher) und **plausibel** ist (|Δr| ≤ 1000 m,
|Δθ| ≤ 1°). Die angewandte Korrektur folgt der Schätzung **geglättet**
(exponentiell, α = 0,3 je Schätzlauf — kein Sprung im Lagebild); fällt die
Politik dauerhaft durch (> 3 Läufe), klingt die Korrektur zur Null ab.
Stabilität per Konstruktion: Der Monitor schätzt weiterhin auf dem
**rohen** Strom (voller Bias), die Korrektur ist ein reiner Tiefpass davon —
kein Integrator, nichts kann oszillieren. Die `.ffplots`-Aufzeichnung
(§6) enthält bewusst die **rohen** Plots (Replay durchläuft dieselbe
Korrektur-Logik, statt doppelt zu korrigieren).

Jede frische Schätzung erscheint als `info`-Log (`registration estimate`)
mit Paar-Anzahl, RMS vor/nach Korrektur, Beobachtbarkeits-Flag und den
Bias-Werten je Sensor; Übernahme/Rücknahme der Korrektur loggt
`registration correction engaged/disengaged (REG.2b)`. Die Metriken stehen
in §3.2 (`firefly_registration_*`).

**Draht (REG.3, ICD 3.3.0):** Bei aktiver Korrektur trägt der
CAT063-Sensor-Status je Radar zusätzlich die **angewandte** Bias-Korrektur
in I063/080 (Range Gain/Bias) und I063/081 (Azimut-Bias) — nachgelagerte
Konsumenten sehen, was das SDPS gerade herausrechnet. Ohne Korrektur werden
die Items nicht gesendet (Absenz = „keine Korrektur").

### 1.6 WebSocket-Zugangskontrolle (NFR-SEC-001, ADR 0017)

Beide Variablen sind **opt-in** — ohne Konfiguration ist kein Schutz aktiv
(geeignet für lokales Demo/Entwicklung). Für Produktionsbetrieb wird mindestens
ein Token empfohlen.

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `FIREFLY_WS_TOKEN` | string | — | Wenn gesetzt, muss jede `/ws`-Verbindung den Token via `Authorization: Bearer <token>` oder `?token=<wert>` vorlegen. Fehlend oder falsch → 401 |
| `FIREFLY_WS_ALLOWED_ORIGIN` | string | — | Wenn gesetzt, muss der `Origin`-Header exakt mit diesem Wert übereinstimmen. Fehlt oder stimmt nicht → 403. Ergänzt Token-Auth (fail-closed) |

**Hinweis Browser-API:** `WebSocket` im Browser unterstützt keine Custom-Header.
Für Browser-Clients daher den `?token=`-Queryparameter verwenden.

### 1.7 Logging

| Variable | Typ | Standard | Bedeutung |
|----------|-----|----------|-----------|
| `RUST_LOG` | string | `info` | Log-Verbosity. Formate: `debug`, `info`, `warn`, `error`, `firefly_server=debug,info` |

---

## 2. Log-Inspektion

### 2.1 Log-Format

Firefly schreibt strukturierte Logs im **JSON-Format** (via `tracing`). Jede
Zeile ist ein eigenständiges JSON-Objekt:

```json
{"timestamp":"2026-07-04T10:23:01.442Z","level":"INFO","target":"firefly_server","message":"starting Firefly server (sources-driven live tracker)","port":8080}
```

### 2.2 Verbosity steuern

```bash
# Nur Fehler und Warnungen:
RUST_LOG=warn ./target/release/firefly-server

# Debug-Ausgaben für den OpenSky-Adapter:
RUST_LOG=firefly_opensky=debug,info ./target/release/firefly-server

# Alles auf debug:
RUST_LOG=debug ./target/release/firefly-server
```

### 2.3 Wichtige Log-Nachrichten

| Nachricht | Bedeutung |
|-----------|-----------|
| `starting Firefly server (sources-driven live tracker)` | Server startet; zeigt `port`. |
| `listening; open http://{addr} in a browser` | Server ist bereit |
| `CAT062 multicast feed enabled destination=...` | Multicast-Feed aktiv; zeigt Ziel-Adresse |
| `CAT062 multicast feed disabled` | Feed aus (Normal bei `FIREFLY_CAT062_ENABLED` nicht gesetzt) |
| `CAT065 heartbeat enabled destination=... period_s=1` | Heartbeat aktiv |
| `CAT063 sensor status sender enabled destination=... period_s=5 sensors_total=3` | Sensor-Status aktiv; zeigt Sensor-Anzahl |
| `OpenSky ADS-B poller enabled lat_min=... lat_max=...` | ADS-B-Adapter läuft |
| `OpenSky ADS-B poller disabled` | Adapter aus (Normal-Zustand) |
| `OpenSky plots received count=42` | Erfolgreiche ADS-B-Abfrage |
| `shutdown signal received` | Graceful-Shutdown eingeleitet |
| `shutdown complete` | Server sauber beendet |

### 2.4 Logs mit `jq` filtern (Empfehlung)

```bash
# Nur Fehlermeldungen:
./target/release/firefly-server 2>&1 | jq 'select(.level == "ERROR")'

# OpenSky-Abfragen zählen:
./target/release/firefly-server 2>&1 | jq 'select(.message | contains("OpenSky plots"))'

# Track-Anzahl über Zeit:
./target/release/firefly-server 2>&1 | jq 'select(.message | contains("CAT062")) | .scans'
```

---

## 3. Prometheus-Metriken

### 3.1 Endpunkt

```
GET http://localhost:8080/metrics
Content-Type: text/plain; version=0.0.4
```

### 3.2 Verfügbare Metriken

| Metrik | Typ | Bedeutung |
|--------|-----|-----------|
| `firefly_ws_clients_connected` | gauge | Aktuell verbundene WebSocket-Clients |
| `firefly_ws_clients_total` | counter | Gesamt-WebSocket-Verbindungen seit Start |
| `firefly_cat062_scans_sent_total` | counter | Gesendete CAT062-Datenblöcke (Scans) |
| `firefly_cat062_send_errors_total` | counter | Fehlgeschlagene CAT062-Sends |
| `firefly_cat065_heartbeats_sent_total` | counter | Gesendete CAT065-Heartbeats |
| `firefly_tracks_active` | gauge | Tracks im zuletzt gesendeten CAT062-Scan |
| `firefly_live_plots_ingested_total` | counter | **Live-Modus:** Plots insgesamt in den Tracker eingespeist |
| `firefly_plot_records_written_total` | counter | **Live-Modus:** In `.ffplots`-Datei geschriebene Records |
| `firefly_opensky_poll_errors_total` | counter | **Live-Modus:** HTTP/Netz-Fehler beim OpenSky-Poll |
| `firefly_opensky_rate_limited_total` | counter | **Live-Modus:** OpenSky-Polls mit HTTP 429 (Rate-Limit; Teilmenge der Poll-Fehler). Jeder 429 dehnt das Poll-Intervall exponentiell (Backoff), Reset bei Erfolg (#49). |
| `firefly_adsbagg_poll_errors_total` | counter | **Live-Modus:** HTTP/Netz-Fehler beim Community-Aggregator-Poll (adsb.lol/adsb.fi, ADR 0031) |
| `firefly_adsbagg_rate_limited_total` | counter | **Live-Modus:** Aggregator-Polls mit HTTP 429 (Teilmenge der Poll-Fehler); jeder 429 dehnt das Poll-Intervall exponentiell (Backoff wie #49) |
| `firefly_flarm_plots_received_total` | counter | **Live-Modus:** Empfangene FLARM/OGN-Plots (APRS-IS, ADR 0026) |
| `firefly_radar_plots_received_total` | counter | **Live-Modus:** Dekodierte Radar-ASTERIX-Plots (CAT048/UDP, ADR 0028) |
| `firefly_adsb021_reports_received_total` | counter | **Live-Modus:** Dekodierte ADS-B-Bodenstations-Meldungen, die Plots wurden (CAT021/UDP, FEP.3) |
| `firefly_mlat_reports_received_total` | counter | **Live-Modus:** Dekodierte WAM/MLAT-Meldungen, die Plots wurden (CAT020/UDP, FEP.5) |
| `firefly_live_plot_batches_dropped_total` | counter | **Live-Modus:** Plot-Batches verworfen, weil der Quell→Tracker-Kanal voll war (Back-Pressure-Verlust). Wächst nur unter Überlast — Operator-Signal zum Skalieren/Drosseln. |
| `firefly_sources_opensky` | gauge | Anzahl konfigurierter `adsb_opensky`-Quellen (Quell-Mix, ADR 0023) |
| `firefly_sources_adsbagg` | gauge | Anzahl konfigurierter `adsb_aggregator`-Quellen (ADR 0031) |
| `firefly_sources_flarm` | gauge | Anzahl konfigurierter `flarm_aprs`-Quellen |
| `firefly_sources_radar` | gauge | Anzahl konfigurierter `radar_asterix`-Quellen |
| `firefly_sources_adsb021` | gauge | Anzahl konfigurierter `adsb_asterix`-Quellen (CAT021-Bodenstation, FEP.3) |
| `firefly_sources_mlat` | gauge | Anzahl konfigurierter `mlat_asterix`-Quellen (WAM/MLAT, FEP.5) |
| `firefly_clutter_cells` | gauge | **SPEC.2b:** Gelernte Zellen der räumlichen Clutter-Karten über alle Sensoren (wachsender Wert = der Tracker kartiert Hotspots) |
| `firefly_jpda_cluster_cap_hits_total` | counter | **CAP.2:** JPDA-Cluster, die über der Enumerations-Kappe lagen (> 8 Tracks oder > 10 Plots) und auf Pro-Track-PDA degradiert wurden. Dauerhaft > 0 wachsend = extrem dichtes Szenario, Abschnitt 11 lesen. |
| `firefly_flight_plans` | gauge | **FPL.1:** Anzahl geladener Flugpläne (`FIREFLY_FLIGHT_PLANS`) |
| `firefly_tracks_correlated` | gauge | **FPL.1:** Tracks mit Flugplan-Korrelation im letzten Output-Tick |
| `firefly_correlation_refused` | gauge | **FPL.1:** sichtbar verweigerte Squawk-Korrelationen im letzten Output-Tick (Duplikat unter Plänen, Conspicuity 1000, Identitätskonflikt) |
| `firefly_correlation_manual` | gauge | **FPL.2:** manuelle Korrelations-Pins in Kraft auf lebenden Tracks im letzten Output-Tick |
| `firefly_snapshot_writes_total` | counter | **HA.1:** erfolgreiche Zustands-Snapshot-Schreibvorgänge (0 ohne `FIREFLY_SNAPSHOT_PATH`) |
| `firefly_snapshot_errors_total` | counter | **HA.1:** fehlgeschlagene Snapshot-Schreibvorgänge — Persistenz kaputt, Lagebild läuft weiter (es wird weiter versucht) |
| `firefly_snapshot_age_seconds` | gauge | **HA.1:** Sekunden seit dem letzten erfolgreichen Snapshot — das Verlustfenster eines Neustarts |
| `firefly_restore` | gauge | **HA.1:** 1 = dieser Prozess hat sein Luftlagebild beim Start aus einem Snapshot wiederhergestellt |
| `firefly_role` | gauge | **HA.2:** 1 = aktiv (main), 0 = standby (beobachtet den Main-Heartbeat) |
| `firefly_failovers_total` | counter | **HA.2:** Promotions dieses Prozesses (ein übernommen habender Standby zählt eine) |
| `firefly_main_heartbeat_age_seconds` | gauge | **HA.2:** Sekunden seit dem letzten beobachteten Main-Heartbeat (aussagekräftig im Standby) |
| `firefly_cat063_status_sent_total` | counter | Gesendete CAT063-Sensor-Status-Blöcke |
| `firefly_sensors_total` | gauge | Anzahl registrierter Sensoren (statisch) |
| `firefly_sensors_active` | gauge | Anzahl aktuell aktiver Sensoren (Plot innerhalb `2.5 × scan_period`) |
| `firefly_sensors_disabled` | gauge | **SRV.2:** Sensoren, die der Betreiber per Kommando aus der Fusion genommen hat (`POST /sensors/{id}/disable`) |
| `firefly_sensor_disabled_plots_dropped_total` | counter | **SRV.2:** am Eingang verworfene Plots deaktivierter Sensoren — wächst nur, solange ein Gate in Kraft ist |
| `firefly_registration_estimates_total` | counter | **Registrierung (REG.2a):** Bias-Schätzläufe des Schatten-Monitors. Bleibt 0 ohne `FIREFLY_REGISTRATION_ENABLED` bzw. ohne ausreichende Radar↔Referenz-Korrespondenzen. |
| `firefly_registration_correspondences` | gauge | **Registrierung:** Korrespondenzen des letzten Schätzversuchs (auch bei Ablehnung wegen zu dünner Evidenz gesetzt — zeigt dem Operator, *warum* keine Schätzung erscheint) |
| `firefly_registration_observable` | gauge | **Registrierung:** 1 = letzte Schätzung voll beobachtbar, 0 = (noch) keine Schätzung oder rangdefiziente Geometrie |
| `firefly_registration_bias_range_m{sensor="…"}` | gauge | **Registrierung:** geschätzter Range-Bias je Radar, Meter (roher Schätzwert). Erscheint erst nach der ersten Schätzung. |
| `firefly_registration_bias_azimuth_deg{sensor="…"}` | gauge | **Registrierung:** geschätzter Azimut-Bias je Radar, Grad (roher Schätzwert). Erscheint erst nach der ersten Schätzung. |
| `firefly_registration_apply_active` | gauge | **Registrierung (REG.2b):** 1 = eine Korrektur ist aktuell in Kraft, 0 = keine (ohne `FIREFLY_REGISTRATION_APPLY` immer 0; auch 0, solange das Anwendungs-Gate jede Schätzung ablehnt) |
| `firefly_registration_applied_bias_range_m{sensor="…"}` | gauge | **Registrierung (REG.2b):** aktuell **angewandter** Range-Bias je Radar, Meter — der geglättete, Gate-geprüfte Wert, der tatsächlich von den Messungen abgezogen wird (≠ roher Schätzwert) |
| `firefly_registration_applied_bias_azimuth_deg{sensor="…"}` | gauge | **Registrierung (REG.2b):** aktuell **angewandter** Azimut-Bias je Radar, Grad |
| `firefly_meteo_qnh_regions` | gauge | **VERT.1:** Anzahl konfigurierter QNH-Regionen (0 = Standardatmosphäre überall) |
| `firefly_meteo_qnh_hpa{region="…"}` | gauge | **VERT.1:** konfiguriertes QNH je Region, hPa. Erscheint nur bei konfigurierten Regionen. |
| `firefly_radar_north_markers_total` | counter | **FEP.1:** empfangene CAT034-Nordmarken über alle Radar-Quellen |
| `firefly_radar_scan_period_seconds{sensor="…"}` | gauge | **FEP.1:** **gemessene** Antennen-Umlaufzeit je Radar (Sekunden, aus Nordmarken-Intervallen). Erscheint erst nach der ersten Messung; speist die CAT063-Staleness-Schwelle. |

### 3.3 Prometheus scrape-Konfiguration

```yaml
# prometheus.yml (Ausschnitt)
scrape_configs:
  - job_name: firefly
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: /metrics
    scrape_interval: 10s
```

### 3.4 Nützliche PromQL-Abfragen

```promql
# Tracks pro Sekunde (Rate):
rate(firefly_cat062_scans_sent_total[1m])

# Fehlerrate des CAT062-Feeds:
rate(firefly_cat062_send_errors_total[5m])

# Aktuelle Track-Anzahl:
firefly_tracks_active

# Back-Pressure-Verlust (Live-Pipeline): verworfene Plot-Batches/min.
# > 0 bedeutet, der Tracker kommt mit der Quell-Rate nicht mit.
rate(firefly_live_plot_batches_dropped_total[1m])

# Konfigurierter Quell-Mix dieser Instanz:
firefly_sources_opensky + firefly_sources_adsbagg + firefly_sources_flarm + firefly_sources_radar + firefly_sources_adsb021 + firefly_sources_mlat
```

---

## 4. Health- und Readiness-Probes

### 4.1 Liveness-Probe (`/health`)

Prüft, ob der HTTP-Server antwortet:

```bash
curl http://localhost:8080/health
# → {"status":"ok"}
```

Kubernetes-Konfiguration:

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 10
```

### 4.2 Readiness-Probe (`/ready`)

Prüft, ob der Server Traffic verarbeiten kann.

- `503 not ready`, bis der erste Plot einer konfigurierten Quelle eingetroffen
  ist. Danach `200 ready`. Kubernetes sendet damit keinen Traffic an einen Pod,
  der noch kein Luftlagebild hat (ADR 0020, AP9.4c-4).
- **Ausnahme:** Eine Instanz **ohne** konfigurierte Quellen ist **sofort**
  `200 ready` — ihr leerer Himmel ist das vollständige Lagebild (ADR 0030);
  der CAT065-Heartbeat läuft unabhängig davon.

```bash
# nach erstem Quell-Plot (oder sofort, wenn quellenlos):
curl http://localhost:8080/ready
# → ready  (HTTP 200)

# vor dem ersten Quell-Plot:
curl -o /dev/null -w "%{http_code}" http://localhost:8080/ready
# → 503
```

Kubernetes-Konfiguration:

```yaml
readinessProbe:
  httpGet:
    path: /ready
    port: 8080
  initialDelaySeconds: 3
  periodSeconds: 5
```

---

## 5. Track-Aufzeichnung (`.ffrec`)

### 5.1 Was ist `.ffrec`?

Das `.ffrec`-Format (**Firefly Recording**) speichert den vollständigen
Simulator-Output (Scans mit SystemTracks) für deterministische Replay-Tests.

Aufzeichnungen entstehen automatisch bei Nutzung des `firefly-recorder`-Crates
(konfigurierbar). Das Format ist JSON-Lines (ein `Frame` pro Zeile).

Die beiden CLI-Werkzeuge des Crates (`record`, `replay`) sind env-getrieben:

| Variable | Werkzeug | Default | Bedeutung |
|----------|----------|---------|-----------|
| `FIREFLY_RECORD_OUTPUT` | `record` | `recording.ffrec` | Pfad der `.ffrec`-Ausgabedatei |
| `FIREFLY_REPLAY_INPUT` | `replay` | `recording.ffrec` | Pfad der `.ffrec`-Eingabedatei |
| `FIREFLY_REPLAY_SPEED` | `replay` | `1.0` | Abspieltempo (`2.0` = doppelt so schnell) |

> Nicht zu verwechseln mit den `FIREFLY_REPLAY_PLOTS_*`-Variablen des
> **Plot**-Replays (§5.2/§6): `.ffrec` speichert den Tracker-**Output**
> (Frames), `.ffplots` den Tracker-**Input** (Plots).

### 5.2 Replay einer Aufzeichnung

```bash
# Aufzeichnung abspielen (env-getrieben, ADR 0020):
FIREFLY_REPLAY_PLOTS_INPUT=session.ffplots \
FIREFLY_REPLAY_PLOTS_SPEED=1.0 \
FIREFLY_CAT062_ENABLED=true \
./target/release/firefly-replay-plots
```

| Variable | Default | Bedeutung |
|----------|---------|-----------|
| `FIREFLY_REPLAY_PLOTS_INPUT` | *(Pflicht)* | Pfad zur `.ffplots`-Eingabedatei |
| `FIREFLY_REPLAY_PLOTS_SPEED` | `1.0` | Abspieltempo; `0` = so schnell wie möglich |
| `FIREFLY_REPLAY_PLOTS_OUTPUT_PERIOD_SECS` | *(Tracker-Default)* | Ausgabetakt beim Replay |

> **Hinweis:** Das Replay-Binary (`firefly-replay-plots`) ist implementiert
> (`crates/firefly-server/src/bin/replay_plots.rs`, AP9.4c-5 / ADR 0020) und wird
> ausschließlich über die obigen `FIREFLY_REPLAY_PLOTS_*`-Envs gesteuert (keine
> `--input`-Flags). Alternativ ist der `firefly-player`-Crate programmatisch nutzbar.

---

## 6. Plot-Aufzeichnung für Live-Betrieb (`.ffplots`)

### 6.1 Was ist `.ffplots`?

Das `.ffplots`-Format speichert jeden eingehenden ADS-B-Plot mit
Wall-Clock-Zeitstempel (Unix-Nanosekunden). Zweck: Der nicht-deterministisch
ankommende Live-Datenstrom wird für deterministisches Replay aufgezeichnet
(ADR 0020 — „Non-deterministic arrival ≠ non-reproducible").

Format: JSON-Lines. Jede Zeile:
```json
{"ts_unix_ns":1750243381000000000,"plot":{...}}
```

### 6.2 PlotRecorder-Pfad konfigurieren (`FIREFLY_PLOT_RECORD_PATH`)

Die Aufzeichnung ist im Live-Server **opt-in** über eine Umgebungsvariable
(QW.4). Ist sie gesetzt, schreibt der Live-Tracker jeden eingehenden Plot vor
der Verarbeitung in die `.ffplots`-Datei; unset bedeutet **kein** Recording.

| Variable | Standard | Bedeutung |
|----------|----------|-----------|
| `FIREFLY_PLOT_RECORD_PATH` | *(leer = aus)* | Pfad zur `.ffplots`-Aufzeichnungsdatei. Gesetzt ⇒ jeder Eingangs-Plot wird aufgezeichnet (Wiederanlauf-/Replay-Grundlage, ADR 0020). Ein **nicht öffenbarer** Pfad (fehlendes Verzeichnis o. Ä.) ist **nicht-fatal**: Warn-Log, der Server läuft ohne Aufzeichnung weiter (Verfügbarkeit vor Aufzeichnung). |

```bash
FIREFLY_PLOT_RECORD_PATH=/var/log/firefly/session.ffplots ./firefly-server
```

Wiedergabe der so entstandenen Datei über `firefly-replay-plots`
(`FIREFLY_REPLAY_PLOTS_INPUT`, siehe §5.2).

### 6.3 Wichtig: Float-Genauigkeit

`.ffplots`-Dateien nutzen `serde_json` mit `float_roundtrip`-Feature, das
`f64`-Werte bit-exakt rund-reist (Standard-serde_json ist nur näherungsweise).
Ohne dieses Feature würde ein Replay leicht abweichen. (Aktiviert
workspace-weit in `Cargo.toml`.)

---

## 7. Graceful Shutdown

Firefly beendet sich sauber auf:

- **Ctrl-C** (SIGINT)
- **SIGTERM** (von Kubernetes, `docker stop`, systemd)

Der laufende HTTP-Server wartet auf den Abschluss offener Anfragen, bevor der
Prozess endet. Im Log erscheint:

```
INFO shutdown signal received
INFO shutdown complete
```

Kubernetes-Empfehlung: `terminationGracePeriodSeconds: 30` im Pod-Spec.

---

## 8. Betriebsart: quellen-getriebener Live-Tracker (einziger Modus, ADR 0030)

Der Server betreibt **immer** den Live-Tracker: die konfigurierten Quellen
(`FIREFLY_SOURCES`, ADR 0023 — oder standalone die Opt-in-Adapter-Envs) speisen
Plots in den langlebigen Tracker; Snapshots gehen über einen `watch`-Kanal an
den WebSocket- und den CAT062-Ausgang. Der Live-Server ist auf automatische
`.ffplots`-Aufzeichnung ausgelegt (ADR 0020) — für deterministisches Replay
(`firefly-replay-plots`, NFR-REPRO-001) und Debugging. **Stand:** Die Verdrahtung
des Recorders im Server ist noch offen (`main.rs`: `LiveTracker::new(tracker,
None)`, AP9.4c-4) — es wird derzeit **keine** `.ffplots`-Datei geschrieben und die
Metrik `firefly_plot_records_written_total` bleibt `0`, bis der Recorder aktiviert
ist. Das Replay-Tooling selbst (oben) ist einsatzbereit für bereits vorhandene
`.ffplots`-Dateien (z. B. aus Tests/`firefly-player`).

Ohne aktive Quelle läuft der Tracker leer: **leerer Himmel + CAT065-Heartbeat**
— so unterscheidet der Konsument einen ruhigen Himmel von einem toten Feed
(ADR 0018). Der frühere Replay-/Szenen-Modus (`FIREFLY_MODE`/`FIREFLY_SCENE`/
`FIREFLY_SPEED`) wurde ausgebaut (ADR 0030); die Frankfurt-Mehrradar-Szene
lebt als Regressions-Fixture in `firefly-player/tests/frankfurt_regression.rs`
weiter.

---

## 9. CAT062-Strom verifizieren (Wireshark)

Zum Verifizieren des Ausgabestroms auf dem Netz:

```
# Wireshark Filter:
udp.dstport == 8600

# ASTERIX Plugin: in Wireshark unter Analyze → Decode As → ASTERIX auswählen,
# oder manuell: Payload enthält 0x3E (CAT062), 0x3F (CAT063) oder 0x41 (CAT065) als erstes Byte
# als erstes Byte, gefolgt von 2 Byte Länge (Big Endian).
```

Mit `tcpdump` auf der Konsole:

```bash
sudo tcpdump -i lo udp port 8600 -X
```

---

## 10. Kubernetes-Deployment (Kurzreferenz)

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: firefly-server
spec:
  replicas: 1
  selector:
    matchLabels:
      app: firefly-server
  template:
    metadata:
      labels:
        app: firefly-server
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      containers:
        - name: firefly-server
          image: firefly-server:latest
          ports:
            - containerPort: 8080
          env:
            - name: FIREFLY_CAT062_ENABLED
              value: "true"
            - name: FIREFLY_OPENSKY_ENABLED
              value: "true"
            - name: FIREFLY_OPENSKY_CLIENT_ID
              valueFrom:
                secretKeyRef:
                  name: opensky-credentials
                  key: client_id
            - name: FIREFLY_OPENSKY_CLIENT_SECRET
              valueFrom:
                secretKeyRef:
                  name: opensky-credentials
                  key: client_secret
            - name: RUST_LOG
              value: "info"
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /ready
              port: 8080
            initialDelaySeconds: 3
            periodSeconds: 5
          resources:
            requests:
              cpu: "100m"
              memory: "64Mi"
            limits:
              cpu: "500m"
              memory: "256Mi"
      terminationGracePeriodSeconds: 30
```

---

## 11. Auslegungsgrenzen — Kapazität & JPDA-Worst-Case (CAP.1/CAP.2)

Dieser Abschnitt dokumentiert die **gemessenen** Kapazitätsgrenzen des
Trackers und den Schutzmechanismus gegen den kombinatorischen
JPDA-Worst-Case. Alle Zahlen stammen aus `cargo bench -p firefly-eval`
(criterion, Release-Build) auf einem Sandbox-Entwicklungshost — **auf der
Zielhardware wiederholen**, bevor sie in eine Betriebsauslegung eingehen
(die *Verhältnisse* sind übertragbar, die Absolutwerte nicht).

### 11.1 Durchsatz-Basislinie (CAP.1, NFR-CAP-001)

Voller Produktions-Hot-Path (`Tracker::process_plots`, Tracker exakt wie
im Live-Betrieb konfiguriert), Szenario: separierter 5-km-Raster
(`load_grid`), 60 s:

| Konstellation | Durchsatz | Echtzeit-Reserve¹ |
|---------------|-----------|-------------------|
| 1 Radar × 10 Ziele | ≈ 221 000 Plots/s | > 1500× |
| 1 Radar × 50 Ziele | ≈ 160 000 Plots/s | > 1500× |
| 2 Radare × 50 Ziele | ≈ 151 000 Plots/s | > 1500× |
| 3 Radare × 100 Ziele | ≈ 114 000 Plots/s | > 1500× |

¹ Bezogen auf die reale Plot-Rate der Konstellation (z. B. 3 Radare ×
100 Ziele ≈ 75 Plots/s bei 4-s-Umlauf). Der Tracker ist in normalem,
auch dichtem Verkehr **nicht** CPU-gebunden.

### 11.2 Der JPDA-Worst-Case und die Cluster-Kappe (CAP.2, FR-TRK-052)

Die JPDA-Assoziation enumeriert **alle zulässigen Zuordnungen** eines
Konflikt-Clusters — im Worst Case O((Plots+1)^Tracks). Normale Szenarien
zerfallen per Gating in kleine Cluster; ein **dichter Pulk**, in dem
jeder Track jeden Plot sieht, kettet aber zu *einem* Cluster zusammen
und explodiert. Gemessen (120-m-Kolonne, ein Cluster, 60-s-Szenario,
Release):

| Kolonnen-Größe | ohne Kappe | mit Kappe |
|----------------|-----------|-----------|
| 8 Ziele | 149 ms | 149 ms (exakt, unter der Kappe) |
| 10 Ziele | **27,8 s** | 0,75 ms |
| 12 Ziele | (Stunden, extrapoliert) | 0,57 ms |

**Mechanismus:** Übersteigt ein Cluster **8 Tracks oder 10 Plots**
(`MAX_CLUSTER_TRACKS`/`MAX_CLUSTER_PLOTS` in `firefly-track/src/jpda.rs`),
degradiert genau dieser Cluster auf **Pro-Track-PDA**: jede Track-Zeile
wird unabhängig normalisiert — das ist die exakte Einzeltrack-JPDA-Formel;
aufgegeben wird nur die Track-übergreifende Exklusivität („ein Plot gehört
nur einem Track"). Der Koaleszenz-Schutz (SPEC.1) läuft unverändert
danach. Kleine Cluster (die Regel) rechnen weiterhin exakt; der teuerste
exakte Fall liegt jetzt **an** der Kappe (8er-Kolonne ≈ 160 ms je
60-s-Szenario im Bench).

**Sichtbarkeit:** Jeder degradierte Cluster zählt
`firefly_jpda_cluster_cap_hits_total` hoch; der erste Treffer und jeder
100. erzeugen ein WARN-Log. Im normalen Betrieb (auch `load_grid` mit
100 Zielen) bleibt der Zähler 0.

**Ehrliche Grenze:** In einem Pulk oberhalb der Kappe ist die Zuordnung
messbar gröber (Plots können mehreren Tracks zugleich Gewicht geben).
Das gemessene dichte-Kolonnen-Szenario ist mit einem 50-m/0,08°-Sensor
allerdings ohnehin **physikalisch unauflösbar** — der Tracker bestätigt
vor wie nach der Kappe 2 Tracks; die Kappe tauscht dort also
unbeobachtbare Genauigkeit gegen begrenzte Latenz. Reproduzieren:
`cargo bench -p firefly-eval` (Gruppe `dense_cluster`).

---

## 12. Bekannte Einschränkungen (Stand 2026-07-16)

| Einschränkung | ADR / Issue | Geplante Lösung |
|---------------|-------------|-----------------|
| Multicast ohne Authentifizierung | ADR 0017 | Netz-Isolation + anwendungsseitige Absicherung |
| OpenSky-OAuth2-Credentials (`FIREFLY_OPENSKY_CLIENT_ID`/`_CLIENT_SECRET`) nur via Env-Variable | ADR 0024/0003 | Kubernetes Secret (bereits empfohlen) |
| Track-Nummernraum (I062/040): max. 65 535 gleichzeitige Tracks inkl. 60-s-Quarantäne gelöschter Nummern; darüber wird die Track-Initiierung abgelehnt (Warn-Log) | FR-TRK-035, ICD 3.1.1 | Bewusste, ehrliche Grenze — weit jenseits realer Kapazität; keine Änderung geplant |
| JPDA-Cluster > 8 Tracks oder > 10 Plots werden auf Pro-Track-PDA degradiert (Abschnitt 11.2); Zuordnung dort gröber, dafür begrenzte Latenz | FR-TRK-052 | Bewusste Auslegungsgrenze; Zähler + WARN machen den Fall sichtbar |

---

## Weiterführend

- **Installationshandbuch** (`docs/INSTALLATION.md`): Erstinbetriebnahme.
- **ICD CAT062** (`docs/ICD-CAT062.md`): Byte-genauer Draht-Vertrag mit Wayfinder.
- **ADR-Verzeichnis** (`docs/decisions/`): Alle Architekturentscheide.
- **Anforderungsregister** (`docs/requirements/README.md`): Rückverfolgbarkeit.
