# VERT.1 — Meteo/QNH-Dienst (SDPS-003)

> **Anforderung:** FR-TRK-041 · **Quell-Kontrakt/Ausgabe-ICD:** unverändert ·
> **Einstufung:** S3 · umgesetzt auf Fable 5

## Fachlich: Warum?

Mode-C-Antworten und Flugflächen sind **Druckhöhen**: die Höhe, bei der der
gemessene statische Druck in der ICAO-Standardatmosphäre (1013,25 hPa)
auftritt. Oberhalb der Transition Altitude ist das genau richtig — dort
fliegen alle nach Standard, und die Staffelung stimmt relativ. **Unterhalb**
fliegt der Verkehr nach **QNH**, dem lokalen, wetterabhängigen Luftdruck.
Ohne QNH-Korrektur ist die angezeigte Höhe um ca. **27–30 ft pro hPa**
falsch; bei einem kräftigen Tief (983 hPa) sind das über **800 ft** — im
An-/Abflugbereich sicherheitsrelevant. ARTAS führt dafür die
**Meteo-Funktion (SDPS-003)**: eine QNH-Quelle mit Regionen und
Aktualisierungszyklus. VERT.1 baut diesen Dienst als Fundament; die
Verwertung im Höhen-Tracking (QNH-korrigierte Höhe → I062/135) folgt in
VERT.2.

## Technik

**`QnhService`** (rein, immutable): eine Menge von **QNH-Regionen**
(`{name, lat, lon, radius_nm?, qnh_hpa}` — typisch ein Flugplatz oder ein
Met-Regions-Zentrum). `lookup(lat, lon)` wählt die **nächstgelegene
anwendbare** Region (Haversine; Radius fehlt = unbegrenzt, konkurriert nur
über Nähe). **Ehrlichkeits-Kern:** Ohne anwendbare Region antwortet der
Dienst mit der Standardatmosphäre, **explizit gekennzeichnet**
(`QnhSource::StandardAtmosphere`) — „keine Daten" und „gemessene 1013 hPa"
sind operativ verschiedene Aussagen; ein QNH wird nie erfunden.

**Barometrie** (`pressure_altitude_to_qnh_altitude`): die **exakte**
ICAO-Troposphären-Umrechnung — Druckhöhe → statischer Druck
(`P = 1013,25 · (1 − L·Hp/T₀)^(1/κ)`) → Höhe in der QNH-Atmosphäre
(`H = (T₀/L) · (1 − (P/QNH)^κ)`, κ = R·L/g₀ = 0,1902632). Nicht die lineare
Faustregel: Bei Standard-QNH ist die Umrechnung die Identität, für kleine
Abweichungen fällt die ~27-ft/hPa-Regel von selbst heraus (per Test
verifiziert), und große Abweichungen/Höhen bleiben korrekt.

**Konfiguration** (`MeteoConfig`, 12-Factor): `FIREFLY_METEO_QNH` trägt die
Regionen als JSON-Array. Malformes JSON, implausibles QNH (außerhalb
**[870, 1085] hPa** — die Rekord-Extreme), leerer Name, Koordinaten außerhalb
WGS84 oder Radius ≤ 0 sind **Startfehler**: Eine konfigurierte-aber-kaputte
Meteo-Quelle darf nie still zur Standardatmosphäre degradieren. Unset =
leere Konfiguration (erlaubt; der Server loggt den Modus).

**Verdrahtung:** Der Server parst beim Start (Fehler fatal, wie
`FIREFLY_SOURCES`) und exponiert `firefly_meteo_qnh_regions` +
`firefly_meteo_qnh_hpa{region=…}`. Der Dienst selbst wird in VERT.2 an das
Höhen-Tracking gereicht.

## Schnittstellen-Wirkung

Keine — reiner interner Dienst. Ausgabe-ICD und Quell-Kontrakt unverändert;
die Draht-Wirkung (I062/135, additiver ICD-Bump) kommt mit VERT.2 und wird
dann separat angekündigt (inkl. Wayfinder-Issue).

## Ehrliche Grenzen (VERT.1)

- **Env-getriebener Provider:** Der Betreiber (oder Wayfinders Orchestrator)
  setzt die QNH-Werte und aktualisiert sie **extern** im Wetter-Zyklus.
  Ein Live-Provider (periodischer METAR-Abruf) braucht eine
  Netz-Freigabe-Entscheidung des Deployments und einen eigenen ADR —
  bewusstes Folge-Häppchen.
- **Keine zeitliche Gültigkeit** am Wert (kein `valid_until`): Die
  Aktualität liegt beim externen Update-Prozess. Ein Staleness-Mechanismus
  gehört zum Live-Provider.
- **Temperatur-Korrektur** (kalte Atmosphäre ≠ ISA-Lapse) ist bewusst außen
  vor — QNH ist der dominante Effekt; QNH+Temperatur wäre ein späterer
  Verfeinerungsschritt.
