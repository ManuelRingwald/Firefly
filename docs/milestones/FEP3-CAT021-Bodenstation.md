# FEP.3 — CAT021-Eingangsadapter: ADS-B von der eigenen Bodenstation

> **Anforderungen:** FR-IO-010 (Decoder), FR-NET-015 (Adapter + Verdrahtung) ·
> **Quell-Kontrakt:** v1.6.0 (additiv) · **Ausgabe-ICD:** unverändert ·
> **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich: Warum?

Fireflys ADS-B kam bisher aus **Internet-REST-Diensten** (OpenSky,
Community-Aggregatoren): gepollt, sekundenlatent, rate-limitiert, eine externe
Abhängigkeit mit Hobby-/Forschungsqualität. Ein Produktions-Deployment
empfängt ADS-B von der **eigenen Bodenstation** als **ASTERIX CAT021 über
UDP** — dieselbe Transportklasse wie der Radar-Feed: lokal, subsekündlich,
Push statt Poll. Genau so konsumiert ARTAS ADS-B. Mit FEP.3 kommen alle drei
operativen Sensor-Klassen (Radar CAT048/034, ADS-B CAT021) über den
Produktions-Transport an.

Der fachliche Mehrwert gegenüber den Internet-Quellen ist die **ehrliche
Messunsicherheit**: Jede CAT021-Meldung trägt ihren **NACp**
(Qualitätsindikator, DO-260B). Firefly leitet daraus die σ **je Meldung** ab
(σ ≈ EPU/2: NACp 11 → 1,5 m … NACp 1 → 9 260 m), statt pauschal 75 m
anzunehmen. Fehlt der NACp (oder ist 0 = „unbekannt"), gilt **bewusst
konservativ 250 m** — schlechter als die Internet-Annahme: Wer keine Qualität
meldet, verdient weniger Vertrauen.

**Drop-Regeln** halten das Luftlagebild sauber: Bodenziele (GBS — würden die
Assoziation um Flughäfen verschmutzen), Simulations-/Testziele (SIM/TST —
Firefly führt keinen simulierten Verkehr, FR-TRK-036) sowie positions- oder
zeitlose Meldungen werden verworfen.

## Technik

**Decoder** (`firefly-asterix::cat021`, FR-IO-010): liest die
**Edition-2.x-UAP** (49 FRNs) mit der gemeinsamen FSPEC-Mechanik. Gelesen
werden die track-relevanten Items — I021/010 (SAC/SIC, Pflicht), Position
**I021/131** (hochauflösend, LSB 180/2³⁰ °) bevorzugt vor I021/130, Zeit
I021/073 → /071 → /077 (Fallback-Kette), I021/080 (ICAO-Adresse), I021/170
(Callsign), I021/070 (Mode 3/A), I021/140/145 (geometrische Höhe /
Flugfläche), I021/090 → **NACp**, I021/040 → **GBS/SIM/TST** — alle übrigen
Items werden **längen-korrekt übersprungen** (vollständiges
Pro-FRN-Format-Modell: fixed/extended/repetitive/compound/explicit; die
Compound-Items Met/Trajectory über Subfeld-Längenmodelle). Ein Item auf einem
**Spare-FRN (43–47)** ist ein harter Fehler: eine Edition-0.26-Station
scheitert **laut** im Log statt still falsch dekodiert zu werden.
Robustheit wie bei allen Eingangs-Decodern (Charta §8): grenzen-geprüfter
Cursor, kein Panic auf Eingabe, Fuzz-Target `cat021_decode` (Smoke 5,3 M
Läufe ohne Befund).

**Adapter** (`firefly-adsb021`, FR-NET-015, spiegelt `firefly-radar`):
`Adsb021Config` (12-Factor, `FIREFLY_ADSB021_*`; Sensor-Default 230, Port
8021), `adsb_report_to_plot` (NACp→σ-Tabelle, Drop-Regeln, Höhe
geometrisch → barometrisch → 0), `datagram_to_plots`/`run` (UDP-Listener,
Multicast-Join bei Gruppe). **Kein Stations-Standort** — anders als beim
polaren Radar sind CAT021-Positionen geodätische **Selbstmeldungen**; der
Standort der Station ist für die Messung irrelevant. Die Plots gehen als
`Plot::adsb` (geodätisch) in denselben Tracker; der Sensor registriert sich
mit Nominal-Periode 5 s (Push-Strom, CAT063-Staleness konservativ je
Station, nicht je Flugzeug).

**Verdrahtung:** `adsb_asterix`-Eintrag im Quell-Kontrakt (v1.6.0, additiv:
`listen`?/`sac`?/`sic`?/`sensor_id`? — keine bbox, kein `cred_env`) oder
standalone `FIREFLY_ADSB021_ENABLED=true`; ein Listener je Quelle in den
geteilten Plot-Kanal, CAT063-Liveness, Metriken
`firefly_adsb021_reports_received_total` / `firefly_sources_adsb021`.

## Schnittstellen-Wirkung

- **Ausgabe-ICD (CAT062/063/065): unverändert** — FEP.3 ist ein reiner
  Eingangs-Pfad.
- **Quell-Eingangs-Kontrakt: v1.6.0, additiv** — neuer Typ `adsb_asterix`;
  ältere Leser lehnen ihn als unbekannt ab. Wayfinders Orchestrator-UI zieht
  nach (Issue `from-firefly`).

## Ehrliche Grenzen (FEP.3)

- **Nur Edition 2.x.** Die UAP älterer Editionen (0.23/0.26) weicht ab; eine
  solche Station scheitert laut statt falsch. Ein Editions-Schalter ist ein
  Folge-Häppchen, wenn real gebraucht.
- **Geschwindigkeit (I021/160) wird noch nicht als Messung genutzt** — der
  Tracker schätzt sie selbst; Einspeisung wäre ein eigenes Häppchen.
- Ohne bbox trägt die Quelle **nichts zum System-Referenzpunkt** bei — als
  Einzelquelle `FIREFLY_SYSTEM_REF_*` setzen (dokumentiert in
  TECHNICAL/INSTALLATION).
- Die MOPS-Versions-Angabe (I021/210) wird längen-korrekt übersprungen, aber
  **nicht ausgewertet** — eine automatische Editions-Erkennung wird erst mit
  dem Editions-Schalter relevant.
