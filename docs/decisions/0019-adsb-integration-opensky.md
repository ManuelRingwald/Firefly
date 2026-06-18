# ADR 0019 — ADS-B-Integration via OpenSky Network

- **Status:** akzeptiert
- **Datum:** 2026-06-18
- **Schnittstellen-relevant:** ja (ICD → 2.4.0, additiv; neues `adsb_age_s`-Feld
  auf `SystemTrack` + ES-Age-Subfeld in I062/290)

## Kontext

Firefly verarbeitet bisher ausschließlich Radar-Plots von simulierten Sensoren
(PSR/SSR). ADS-B (*Automatic Dependent Surveillance–Broadcast*) erweitert das
Bild um hochpräzise **Selbstberichte** der Luftfahrzeuge über Mode S Extended
Squitter:

- **Datenrate:** 1–2 s Aktualisierungsintervall (gegenüber 4–12 s Radar-Scan).
- **Genauigkeit:** NACp 8–10 (< 75 m), wesentlich besser als PSR.
- **Identität:** ICAO-24-Bit-Adresse als harter Identitätsanker — keine
  Zuordnungsambiguität bei der JPDA.
- **Datenverfügbarkeit:** Über das **OpenSky Network** ist ein frei zugänglicher
  REST-Endpunkt verfügbar, der Echtzeit-Zustandsvektoren für eine konfigurierbare
  Bounding-Box liefert.

Die Integration macht Firefly zu einem Multi-Source-Fusionssystem, das Radar-
und ADS-B-Daten gleichzeitig verarbeitet — ein wesentlicher Schritt auf dem Weg
zur Phoenix-ASD-Integration (ADR 0006).

## Entscheidung

### Quelle: OpenSky Network REST API

Die ADS-B-Daten werden als **HTTP-Poll** gegen die OpenSky-REST-API (`/api/states/all?lamin=…&lomin=…&lamax=…&lomax=…`) bezogen. Intervall: Default 10 s (konfigurierbar, `FIREFLY_OPENSKY_POLL_INTERVAL_SECS`). Die API liefert Zustandsvektoren (ICAO24, Callsign, Lat/Lon, Baro-Höhe, Velocity, True-Track, position_source).

Alternativen (ADS-B-Dump1090-Empfänger, ADSB Exchange) wurden erwogen. OpenSky
wurde gewählt weil: kein eigenes Empfangshardware nötig, breite geografische
Abdeckung, stabile öffentliche API, optionale Authentication für höhere Rate
Limits.

### Rolle: Volle Fusion in den kinematischen Zustandsvektor

ADS-B-Plots werden **vollständig** in den bestehenden Kalman-JPDA-IMM-Tracking-
Zyklus fusioniert — nicht als Annotation eines Radar-Tracks, sondern als erste
Klasse von Eingangsdaten. Die Architektur nutzt den in AP9.1/AP9.2 eingeführten
`Measurement::Geodetic`-Pfad: WGS84 → ENU via `LocalFrame::geodetic_to_enu`,
isotrope Kovarianz `R = σ² · I₂`.

### ICAO-Adress-basierte Vorsortierung (AP9.3)

Vor dem kinematischen JPDA-Gate wird ein Identitätsschritt eingeschoben: Ein
Plot mit bekannter ICAO-Adresse, die einem lebenden Track entspricht, wird direkt
diesem Track zugeordnet (β=1, kein Mahalanobis-Gate). Nur Plots ohne Treffer
gehen in den normalen JPDA-Pool. Dies ist korrekt, weil die ICAO-Adresse eine
fahrzeugindividuelle Hardware-ID ist.

### NACp → Kovarianz

| OpenSky `position_source` | σ_pos [m] |
|--------------------------|-----------|
| 0 — ADS-B (NACp ≥ 8) | 75 |
| 1 — ASTERIX | 200 |
| 2 — MLAT | 200 |
| Default | 300 |

### CAT062-Ausgabe: I062/290 ES-Age-Subfeld (ICD 2.4.0)

`Track.adsb_last_hit_time: Option<f64>` speichert den Zeitpunkt des letzten
ADS-B-Treffers; `SystemTrack.adsb_age_s: Option<f64>` gibt das Alter an. Im
Encoder wird das ES-Age-Subfeld (`0x08` im primären Subfeld-Oktett von I062/290)
nur kodiert, wenn `adsb_age_s` vorhanden ist. Das Signal an Wayfinder: „ES-Age
vorhanden → Track hat ADS-B-Anteil".

### SensorId für den OpenSky-Adapter

Feste, konfigurierbare `SensorId` für den OpenSky-Adapter (Default
`SAC=0, SIC=200`). ADS-B-Plots kommen als `DetectionKind::Secondary`.
Mehrere ADS-B-Quellen könnten verschiedene SICs erhalten.

## Konsequenzen

### Positiv
- Multi-Source-Fusion: Firefly verarbeitet Radar + ADS-B gleichzeitig.
- Track-Stabilität verbessert (ADS-B liefert dichte Updates).
- ICAO-Adresse als Identitäts-Anker eliminiert viele Zuordnungsambiguität.
- CAT062-Ausgang kennzeichnet ADS-B-Anteil (Wayfinder-Badge).

### Negativ / Einschränkungen
- **OpenSky-Latenz:** ~5–10 s (kein Echtzeit-ADS-B). Kein Problem für den
  Tracker (er arbeitet nach Datenzeit); für operative Systeme wäre ein
  Direktempfänger (Dump1090, VRS) vorzuziehen.
- **Rate Limits:** Anonyme Nutzung ~1 Request/10 s, Authentication erhöht auf
  ~1/5 s (konfigurierbar via `FIREFLY_OPENSKY_USERNAME/PASSWORD`).
- **Spoofing-Schutz:** ICAO-Adressen sind nicht kryptografisch authentifiziert.
  Die Vorsortierung (AP9.3) vertraut der ICAO-Identität. ADR 0017 (Netz-
  Isolation) ist die primäre Schutzmaßnahme; eine kryptografische ADS-B-
  Authentifizierung (z. B. ACAS X-Erweiterungen) ist kein Teil dieses ADR.
  **Empfehlung für operative Systeme:** Kreuzvalidierung gegen Radar-Tracks
  (plausible Position?); explizite Dokumentation der Vertrauensgrenze.
- **Verfügbarkeit:** OpenSky hat Ausfälle; der Adapter muss robust mit
  HTTP-Fehlern, Timeouts und leeren Antworten umgehen.

## Verworfene Alternativen

- **Annotation statt Fusion:** ADS-B-Daten als reinen Label-Override auf
  bestehende Radar-Tracks aufstempeln — einfacher, aber verliert die
  Präzisionsvorteile der vollen kinematischen Fusion.
- **Dump1090 / RTL-SDR:** Eigener Empfänger für Echtzeit-ADS-B. Wäre das
  beste Signal, erfordert aber Hardware im Demo-/CI-Umfeld.
- **ADS-B als eigene SystemTrack-Quelle:** Eigener Track pro ADS-B-Sensor, kein
  Zusammenführen mit Radar — verliert Mehrwert der Fusion.
