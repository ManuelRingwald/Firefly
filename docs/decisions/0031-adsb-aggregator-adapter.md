# ADR 0031 — Community-Aggregator-ADS-B-Adapter (`adsb_aggregator`: adsb.lol / adsb.fi)

- **Status:** akzeptiert
- **Datum:** 2026-07-05
- **Schnittstellen-relevant:** Eingangs-Kontrakt — neuer Quell-Typ
  `adsb_aggregator` mit optionalem `provider`-Feld;
  `docs/source-input-contract.md` → **v1.5.0** (additiv). Der
  **Ausgabe**-Vertrag (CAT062/UDP, ICD) bleibt unberührt — Wayfinders
  Decoder/ASD merkt nicht, woher die Plots kamen.
- **Auslöser:** Firefly-Issue [#53](https://github.com/manuelringwald/firefly/issues/53)
  (`from-wayfinder`) — ADS-B-Bezug aus Umgebungen, in denen OpenSky nicht
  erreichbar ist. Wayfinder-Gegenstück: Wayfinder #201.

## Kontext

Der bisher einzige ADS-B-Adapter ist OpenSky (ADR 0019, OAuth2 seit ADR 0024).
Zwei operative Schwächen wurden am 2026-07-05 real:

1. **Erreichbarkeit.** OpenSky verwirft Verbindungen aus verbreiteten
   Rechenzentrums-IP-Ranges (verifiziert aus GitHub Codespaces/Azure: DNS löst
   auf, TCP läuft in den Timeout, restliches Internet erreichbar). Aus solchen
   Umgebungen ist der `adsb_opensky`-Pfad **strukturell tot** — kein
   Konfigurations-, kein Code-Problem.
2. **Zugangs-Hürde.** OpenSky verlangt einen OAuth2-Client (Registrierung,
   Secret-Verwaltung über Wayfinders Feed-Secrets, Token-Lebenszyklus). Für
   Demo-/Dev-Betrieb ist das die häufigste Fehlkonfigurations-Quelle
   (Wayfinder #198/#199 entstanden genau dort).

**Die Community-Aggregatoren** des readsb/tar1090-Ökosystems — adsb.lol,
adsb.fi (und weitere) — servieren dieselbe crowdgesourcte ADS-B-Lage über ein
**gemeinsames, ADSBExchange-v2-kompatibles JSON-API**: offen, **ohne jede
Authentifizierung**, ohne bekannte Datacenter-IP-Sperren (adsb.lol aus dem
betroffenen Codespace live verifiziert). Ein Adapter bedient damit mehrere
Anbieter; nur Host und Pfad-Präfix unterscheiden sich.

## Entscheidung

### 1. Ein zweiter ADS-B-Adapter **neben** OpenSky — kein Ersatz

`adsb_opensky` bleibt vollwertig erhalten: Er funktioniert im lokalen/VPS-Betrieb,
hat mit OAuth2 dokumentierte, planbare Rate-Limits und Forschungs-Datenqualität.
Der neue Typ `adsb_aggregator` tritt daneben; der Operator wählt den Bezugsweg
**pro Quelle** (Wayfinder-UI: Quell-Typ-Dropdown). Anbieter-Diversität ist für
ein reales System ein Feature (Ausfall-Ausweichweg), kein Ballast. Ein Feed darf
beide Quellen gleichzeitig tragen — der Tracker fusioniert ohnehin über
Sensor-IDs.

### 2. Ein Adapter, mehrere Provider (`provider`-Feld)

Neues Crate **`firefly-adsbagg`** (Blaupause `firefly-opensky`): REST-Poller →
`Vec<Plot>` (`Measurement::Geodetic`) → geteilter `mpsc` des Live-Trackers.
Das `provider`-Feld (Kontrakt v1.5.0) wählt den Host:

| Provider | Basis | Pfad | Limit |
|----------|-------|------|-------|
| `adsb_lol` (Default) | `https://api.adsb.lol` | `/v2/lat/{lat}/lon/{lon}/dist/{NM}` | dynamisch |
| `adsb_fi` | `https://opendata.adsb.fi/api` | `/v3/lat/{lat}/lon/{lon}/dist/{NM}` | 1 req/s |

Ein unbekannter Provider ist ein **Startfehler** — nie ein still substituierter
Anbieter. Ein automatisches Failover *innerhalb* einer Quelle wird bewusst
**nicht** gebaut: Der Operator soll sehen und entscheiden, woher seine
Surveillance-Daten kommen.

**airplanes.live ist zurückgestellt:** Die Einheit des Radius-Parameters seines
`/v2/point`-Endpoints ist öffentlich widersprüchlich dokumentiert (km vs. NM).
Eine falsch geratene Einheit halbierte den Abdeckungskreis **ohne
Fehlermeldung** — eine unsichtbare Überwachungslücke. Aufnahme erst nach
Verifikation der Einheit.

### 3. BBox bleibt der Kontrakt; der Adapter rechnet den Umkreis

Die Aggregator-APIs fragen **Mittelpunkt+Radius** (max. 250 NM), das
Quell-Modell (Wayfinder + Kontrakt) beschreibt Abdeckung als **BBox**. Der
Adapter überbrückt: Umkreis der BBox (Mittelpunkt + Großkreis-Distanz zur
fernsten Ecke, Haversine), Antwort **zurückgefiltert auf die BBox**. Damit
verhält sich die Quelle nach außen exakt wie die BBox-nativen Quellen — kein
neues Geo-Konzept im Kontrakt oder in Wayfinders UI. Sprengt die BBox den
250-NM-Deckel, wird geclampt und beim Start **prominent gewarnt** (WARN):
partielle Abdeckung wird nie verschwiegen.

### 4. Auth-frei, robust gegen veraltete Credentials

Der Typ trägt **kein** `cred_env`. Ein dennoch gesetztes `cred_env` (z. B. eine
nach einem Quell-Typwechsel verwaiste Referenz, Wayfinder-#198-Klasse) wird
**ignoriert statt abgelehnt** — wie bei `radar_asterix`.

### 5. Schema-Mapping & Robustheit (untrusted input)

Named-Fields-JSON statt OpenSky-Positions-Arrays. Mapping auf `Plot`:
`hex`→ICAO (ein `~`-Präfix markiert Nicht-ICAO/TIS-B → Plot ohne
ICAO-Identität), `flight`→Callsign (getrimmt), `lat`/`lon` (Pflicht),
`alt_baro` in **Fuß** oder Literal `"ground"` (→ verworfen, wie OpenSkys
`on_ground`), `alt_geom` (Fuß) bevorzugt für die Position, `squawk`
(4-Oktal-Ziffern), `type`→σ analog ADR-0019-Tabelle (`adsb*`/`adsr*` 75 m,
`mlat` 200 m, sonst 300 m). **Staleness-Filter:** `seen_pos` > 30 s → verworfen
(gecoastete API-Einträge sind keine aktuellen Messungen). **Datenzeit** je
Luftfahrzeug = Server-`now` (ms→s-normalisiert) − `seen_pos` (datenzeit-
getrieben, ADR 0013). Kein Panic auf Eingabe; Unbrauchbares wird verworfen.

### 6. Betriebs-Verhalten wie OpenSky

Poll-Intervall Default 10 s (höflich unter dem 1-req/s-Limit), pro Quelle via
`poll_interval_secs` (Kontrakt v1.5.0). 429 → eigener Fehlertyp + exponentielles
Backoff (Muster #49). Sensor-ID-Default **230** (Dekaden-Schema: OpenSky 200,
FLARM 210, Radar 220). Metriken: `firefly_adsbagg_poll_errors_total`,
`firefly_adsbagg_rate_limited_total`, `firefly_sources_adsbagg`. CAT063-
Sensor-Liveness und `live_ready` wie bei allen Adaptern. Standalone-Envs
`FIREFLY_ADSBAGG_*` (inkl. `FIREFLY_ADSBAGG_BASE_URL` für Tests/self-hosted
Aggregatoren).

## Konsequenzen

- **Positiv:** ADS-B funktioniert aus egress-beschränkten/OpenSky-gesperrten
  Umgebungen; kein Secret-Handling für den Demo-/Dev-Pfad; Anbieter-Redundanz;
  Wire-Ausgabe unverändert.
- **Negativ / Grenzen (ehrlich):** Community-Betrieb ohne SLA; Daten sind
  crowdgesourct und unauthentifiziert (gleiche Vertrauensgrenze wie OpenSky,
  ADR 0017/0019 — Hobby-/Forschungsqualität, keine zertifizierte
  Surveillance-Quelle). adsb.lol-Daten stehen unter **ODbL** (Namensnennung,
  Share-Alike) — für den internen ASD-Betrieb unkritisch, bei
  Weiterveröffentlichung zu beachten. Der Umkreis fragt mehr Fläche ab als die
  BBox (Server-Last beim Provider, durch die 250-NM-Kappe begrenzt).
- **Folgearbeit:** Wayfinder #201 (Store-Vokabular, Orchestrator-Pass-through,
  UI-Typ + Provider-Auswahl ohne Credential-Block). airplanes.live nach
  Einheiten-Verifikation als dritter Provider (additiv, nur Vokabular-Erweiterung
  des `provider`-Felds).

## Alternativen erwogen

- **OpenSky ersetzen:** verworfen — Breaking Change für bestehende Feeds ohne
  Not; OpenSky bleibt im lokalen Betrieb der besser planbare Weg.
- **Ein generischer `adsb`-Typ mit Provider inkl. OpenSky:** verworfen — OpenSky
  hat ein völlig anderes API (Auth, Schema, BBox-Query); ein Sammel-Typ hätte
  Migrations-Aufwand für bestehende `adsb_opensky`-Einträge und verwischt die
  Typ↔Adapter-Zuordnung des Kontrakts.
- **Automatisches Provider-Failover im Adapter:** verworfen — versteckte Magie;
  die Anbieterwahl ist eine sichtbare Operator-Entscheidung.
- **Proxy/VPN um OpenSkys IP-Sperre:** verworfen als Produkt-Lösung — externe
  Infrastruktur-Abhängigkeit statt eines sauberen zweiten Bezugswegs.
