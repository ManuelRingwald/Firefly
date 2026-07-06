# Community-Aggregator-ADS-B-Adapter (`adsb_aggregator`, ADR 0031)

> **Baustein:** Crate `firefly-adsbagg` + Kontrakt v1.5.0 + Server-Verdrahtung.
> **Anforderung:** FR-NET-014. **Auslöser:** Firefly #53 (`from-wayfinder`),
> Wayfinder-Gegenstück #201.

## Fachlich — warum ein zweiter ADS-B-Adapter?

Das ASD braucht Live-ADS-B auch dort, wo **OpenSky nicht erreichbar** ist. Am
2026-07-05 real diagnostiziert: OpenSky verwirft Verbindungen aus der
Azure-/Codespaces-IP-Range (DNS löst auf, TCP-Timeout, restliches Internet
erreichbar) — der `adsb_opensky`-Pfad ist aus solchen Umgebungen strukturell
tot. Zusätzlich ist OpenSkys OAuth2-Zugang (Registrierung, Secret-Verwaltung)
die häufigste Fehlkonfigurations-Quelle im Demo-/Dev-Betrieb (Wayfinder
#198/#199).

Die **Community-Aggregatoren** des readsb/tar1090-Ökosystems (adsb.lol,
adsb.fi) liefern dieselbe crowdgesourcte ADS-B-Lage **ohne jede Anmeldung**
über ein gemeinsames, ADSBExchange-v2-kompatibles JSON-API. OpenSky bleibt
vollwertig erhalten — der Operator wählt den Bezugsweg **pro Quelle**
(Anbieter-Diversität = Ausfall-Resilienz; beide gleichzeitig ist zulässig, der
Tracker fusioniert über Sensor-IDs).

## Technisch

**Crate-Aufbau (Blaupause `firefly-opensky`):**

| Modul | Inhalt |
|-------|--------|
| `config` | `AdsbAggConfig` (12-Factor `FIREFLY_ADSBAGG_*`), `Provider`-Enum (`adsb_lol` Default, `adsb_fi`; URL-Basis + versionierter Pfad je Provider, `BASE_URL`-Override für Tests/self-hosted) |
| `geometry` | BBox↔Kreis-Brücke: `circle_for_bbox` (Umkreis = Mittelpunkt + max. Haversine-Distanz zu den Ecken; Floor 1 NM, Clamp 250 NM mit `clamped`-Flag), `BBoxDeg::contains` (Antwort-Rückfilter) |
| `api` | ADSBEx-v2-Schema → `Plot`: `hex` (`~` = Nicht-ICAO), `alt_baro` Fuß **oder** `"ground"`, `alt_geom` bevorzugt, `type`→σ (75/200/300 m analog ADR 0019), `seen_pos`-Staleness (30 s), `now` ms→s-Heuristik, Datenzeit = `now` − `seen_pos` |
| `poller` | `AdsbAggPoller`: eine URL je Quelle (bei Konstruktion abgeleitet, Clamp-WARN), 429 → `RateLimited` + exponentielles Backoff (Muster #49), Endlos-`run`-Schleife wie OpenSky |

**Mathematik (BBox → Umkreis).** Die Aggregator-APIs fragen Punkt+Radius. Der
Umkreis um die BBox garantiert **vollständige Abdeckung** (Radius = Großkreis-
Distanz Mittelpunkt→fernste Ecke, Haversine auf der mittleren Kugel; der
~0,5-%-Kugelfehler ist irrelevant, weil aufgerundet und zurückgefiltert wird).
Der Überschuss an den Rändern wird client-seitig auf die BBox getrimmt — nach
außen verhält sich die Quelle exakt wie die BBox-nativen. Deckel 250 NM
(Provider-Limit): größere BBoxen werden geclampt und beim Start **prominent
gewarnt** — partielle Surveillance-Abdeckung wird nie verschwiegen.

**Verdrahtung (`firefly-server`):** `SourceType::AdsbAggregator` +
`adsbagg_config_from_spec` (Kontrakt v1.5.0; unbekannter `provider` →
Startfehler; `cred_env` ignoriert — auth-frei, robust gegen verwaiste
Referenzen), `spawn_adsbagg_poller_live` in den geteilten Plot-Kanal,
`representative_config` bezieht die Quelle in Union-BBox/Kadenz/Frame-Anker
ein, Sensor-ID-Default **230** (Dekaden-Schema 200/210/220/230), CAT063-
Liveness, Metriken `firefly_adsbagg_poll_errors_total` /
`firefly_adsbagg_rate_limited_total` / `firefly_sources_adsbagg`.

**Grundwahrheit der Tests:** eine echte `api.adsb.lol`-Antwort (Frankfurt,
2026-07-05) als Fixture, ergänzt um synthetische Randfälle (`"ground"`,
Staleness, `~`-Hex, Out-of-Box).

## Grenzen (ehrlich)

- Community-Betrieb ohne SLA; crowdgesourcte, unauthentifizierte Daten —
  Hobby-/Forschungsqualität wie OpenSky, keine zertifizierte Quelle
  (Vertrauensgrenze ADR 0017/0019).
- adsb.lol-Daten stehen unter **ODbL**; bei Weiterveröffentlichung zu beachten.
- **airplanes.live zurückgestellt:** Radius-Einheit des `/v2/point`-Endpoints
  öffentlich widersprüchlich (km vs. NM) — eine falsche Einheit halbierte den
  Abdeckungskreis unsichtbar. Aufnahme erst nach Verifikation (ADR 0031).
