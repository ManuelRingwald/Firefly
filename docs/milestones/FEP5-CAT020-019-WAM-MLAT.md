# FEP.5 — CAT020/019-WAM/MLAT-Eingang

> **Anforderungen:** FR-IO-012 (Decoder), FR-NET-017 (Adapter + Verdrahtung) ·
> **Quell-Kontrakt:** v1.7.0 (additiv) · **Ausgabe-ICD:** unverändert ·
> **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich: Warum?

**Wide Area Multilateration** ist die dritte operative Überwachungstechnologie
neben Radar und ADS-B: Bodenstationen empfangen die Transponder-Signale und
berechnen die Position aus **Laufzeitdifferenzen** (TDOA). Zwei Eigenschaften
machen WAM operativ wichtig: Es ist — anders als ADS-B — **unabhängige**
Überwachung (das Flugzeug kann seine Position nicht selbst fälschen), und es
deckt Räume ab, in denen Radar teuer oder unmöglich ist (Gebirgstäler,
Terminal-/Flughafen-Nahbereich). ARTAS konsumiert WAM als **CAT020**
(Zielmeldungen) + **CAT019** (Systemstatus). Mit FEP.5 bedient Firefly alle
vier klassischen ARTAS-Eingangsklassen: Radar (CAT048/034 modern,
CAT001/002 legacy), ADS-B (CAT021) und WAM (CAT020/019).

## Technik

**Decoder** (`firefly-asterix::cat020`/`cat019`, FR-IO-012): CAT020 liest die
track-relevanten Items der 28-FRN-UAP — Identität, **volle** Tageszeit,
Position **I020/041 WGS84 hochauflösend** (LSB 180/2²⁵ °, dieselbe Auflösung
wie unser CAT062-Ausgang), Mode 3/A, Flugfläche, geometrische Höhe,
ICAO-Adresse, Callsign, Track-Nummer, Descriptor-Flags — alle übrigen Items
längen-korrekt übersprungen (inkl. der Repetitive-Items Contributing
Receivers und Mode-S-MB). **Der Ehrlichkeits-Kern ist I020/500 SDP:** die
Standardabweichung, die das MLAT-System für genau diese Positionslösung
berechnet hat (σx/σy × 0,25 m; berichtet als konservatives max), wird zur
Mess-σ **je Meldung** — dieselbe Philosophie wie FEP.3s NACp-Ableitung, nur
mit einer noch direkteren Qualitätsquelle. CAT019 liefert Message Type
(Start-of-Update-Cycle/Periodic/Event) + **NOGO** (nur 0 = operational-
Anspruch; absent = kein Anspruch). Beide UAPs gegen die generierte
EUROCONTROL-Referenz (asterix-specs/libasterix cat020 ed 1.11 / cat019
ed 1.3) verifiziert; beide Decoder gefuzzt (4,2 M/5,0 M Läufe ohne Befund).

**Adapter** (`firefly-mlat`, FR-NET-017, spiegelt `firefly-adsb021`):
`MlatConfig` (`FIREFLY_MLAT_*`, Sensor-Default 240, Port 8020),
`mlat_report_to_plot` mit den **Drop-Regeln** — Feldmonitor (RAB: ein fester
Test-Transponder zur Systemkalibrierung, nie ein echtes Flugzeug),
Simulations-/Testziele (SIM/TST), Bodenziele (GBS) und positions-/zeitlose
Meldungen gelangen nie ins Luftlagebild. Der Listener dispatcht am
CAT-Oktett: CAT020 → Plots, **CAT019 → Liveness** (jede Statusmeldung =
Sensor-Aktivität für CAT063 — „leerer Himmel" ≠ „totes MLAT-System";
selbst-gemeldetes degraded/NOGO wird als WARN geloggt). Kein Standort, keine
bbox nötig — CAT020-Positionen sind geodätisch.

**Verdrahtung:** `mlat_asterix` im Quell-Kontrakt (v1.7.0, additiv) oder
standalone `FIREFLY_MLAT_ENABLED=true`; ein Listener je Quelle in den
geteilten Plot-Kanal, Metriken `firefly_mlat_reports_received_total` /
`firefly_sources_mlat`.

## Schnittstellen-Wirkung

- **Ausgabe-ICD (CAT062/063/065): unverändert** — reiner Eingangs-Pfad.
- **Quell-Eingangs-Kontrakt: v1.7.0, additiv** — neuer Typ `mlat_asterix`;
  Wayfinders Orchestrator-UI zieht nach (Issue `from-firefly`).

## Ehrliche Grenzen (FEP.5)

- **Provenienz erscheint als Mode S** (I062/290): MLAT-Positionen entstehen
  aus Mode-S-Transponder-Signalen — technologisch ehrlich, aber ein
  Konsument kann WAM nicht von Mode-S-Radar unterscheiden. Ein eigenes
  MLT-Age-Subfeld + `SourceKind`-Variante wäre ein Ausgabe-ICD-Bump und ist
  bewusst ein Folge-Häppchen.
- **I020/202-Geschwindigkeit wird nicht als Messung genutzt** — der Tracker
  schätzt selbst (konsistent zu CAT048/CAT021).
- **I020/110 (lokale Höhe), Mode 1/2, ACAS-RA** werden übersprungen, nicht
  ausgewertet.
- Ohne bbox **kein Beitrag zum System-Referenzpunkt** — als Einzelquelle
  `FIREFLY_SYSTEM_REF_*` setzen.
