# Gap-Analyse & Roadmap: Firefly → vollwertiges SDPS (Referenz: EUROCONTROL ARTAS)

> **Zweck:** Verbindliche Standortbestimmung und Arbeitsplan auf dem Weg zum
> vollwertigen SDPS. Referenzrahmen ist **ARTAS** (ATM suRveillance Tracker And
> Server) als funktionales Vorbild: **T**racker + **S**erver, dazu FEP
> (Sensor-Eingang), Umgebungsdaten, Betrieb/Redundanz und Assurance.
>
> **Erstellt:** 2026-07-10 (Code-/Doku-Inventur beider Repos + Synthese).
> Lebendes Dokument — nach jedem abgeschlossenen Häppchen den Status-Haken und
> die Prozent-Spalte nachziehen. Bei Widerspruch zum tagesaktuellen Stand gilt
> `docs/STATUS.md`.

---

## 1. Messlatte: Was heißt „100 %"?

**100 % = funktionale ARTAS-Äquivalenz als vollwertiges SDPS — innerhalb der
eigenen Architektur.** Bewusst dokumentierte Abweichungen zählen als erfüllt,
sobald sie per ADR festgeschrieben sind:

- **CAT252-artige Subscription-/Track-Services** → erbringt arbeitsteilig
  **Wayfinder** (mandanten-gescopte AOI/FL-Filterung am WS-Rand). Firefly
  bleibt Fire-and-Forget-Multicast + ADR über die Arbeitsteilung (SRV.1).
- **SNMP-/CMD-Supervision** → ersetzt durch Cloud-Observability
  (Prometheus/Grafana, Probes) + Laufzeit-Steuerungs-API (SRV.2).

Die Prozente sind **Fähigkeits-Abdeckung, nicht Aufwand** — HA.2 bringt z. B.
nur 4,5 Punkte, ist aber eines der teuersten Häppchen.

## 2. Gewichtungsmodell

| Block | Gewicht | Ist (2026-07-10) |
|---|---:|---:|
| Tracker-Kern (IMM/JPDA/Fusion/Lebenszyklus/Geodäsie) | 15 | ~12 |
| CAT062/065/063-Ausgabe | 7 | ~4 |
| Sensoreingang Radar (CAT048/034/001/002) | 9 | ~3 |
| ADS-B/WAM/DAPs-Eingang (CAT021/020/019, BDS) | 8 | ~1 |
| Sensor-Registrierung/Bias | 8 | 0 |
| Vertikal/3-D + Kinematik (Höhe, RoCD, MoM, Beschl.) | 11 | ~1 |
| Meteo/QNH (SDPS-003) | 3 | 0 |
| Flugplan-Korrelation (I062/390) | 7 | 0 |
| Sonderfälle (Duplikate/Reflexion/Split/Clutter-Karte) | 5 | ~1 |
| Server-Funktion/Dienste (arbeitsteilig, per ADR) | 4 | ~1,5 |
| HA/State-Sync (SDPS-002) | 9 | ~0,5 |
| Recording/Replay/Auswertung | 4 | ~2 |
| Kapazität/Lastfestigkeit | 5 | ~0,5 |
| Supervision/Betrieb/K8s | 3 | ~1 |
| Assurance (FHA, Fuzzing, Coverage, Nachweise) | 5 | ~2 |
| **Summe** | **100** | **≈ 30** |

**Startpunkt: ≈ 30 %.** Der solide Tracker-Kern (IMM/JPDA, zentrale
Mess-Fusion nach ADR 0010, Datenzeit-Determinismus, Zwei-Tore-Konzept ADR 0011,
adaptiver Lebenszyklus ADR 0012/0013) und die ICD-Disziplin (standardtreue UAP,
byte-genaue Referenz-Dumps, versionierte ICD) tragen fast den gesamten
Ist-Stand.

## 3. Die fünf größten Abstände (Kurzfassung der Analyse)

1. **Kein echter Radar-Sensoreingang im ARTAS-Sinn:** nur CAT048-Zielmeldungen;
   kein CAT034 (Nordmarke/Sektor/Servicemeldungen — ohne sie ist ein echtes
   Radar betrieblich blind), kein CAT001/002/021/020/019, keine Mode-S-DAPs
   (I048/250 wird übersprungen → I062/380 nur ADR-Subfeld).
2. **Keine Sensor-Registrierung/Bias-Korrektur** (ADR 0010 „späteres Thema"):
   ohne sie erzeugt die Fusion *echter* Radare Doppelbilder — mit dem
   Simulator unsichtbar, im Betrieb sofort sichtbar. Passend fehlen die
   CAT063-Bias-Items I063/070–092.
3. **2-D-Tracker:** keine Höhenschätzung, keine Vertikalrate, keine
   QNH-Korrektur (SDPS-003 offen), kein Mode of Movement, keine Beschleunigung
   → I062/130/135/200/210/220 fehlen.
4. **Kein „Server" im ARTAS-Sinn:** keine Flugplan-Korrelation (I062/390 —
   im gesamten Backlog bislang unbeleuchtet!), keine nutzerspezifischen
   Track-Services (bewusst → Wayfinder, per ADR festzuschreiben).
5. **Keine HA/Redundanz** (SDPS-002: Main/Standby, State-Sync;
   Eingangs-Recorder war im Live-Pfad nicht verdrahtet) und **kein
   Kapazitätsnachweis** (kein Benchmark-Harness; größtes Testszenario
   ~8 Ziele/3 Radare; JPDA-Cluster-Worst-Case ungetestet).

Weitere Befunde: kein aktives Duplikat-/Split-Merge-Handling, keine
Reflexions-Erkennung, globales statt räumliches Clutter-Modell, verspätete
Plots werden verworfen statt gepuffert (FR-TRK-033, bewusst), kein echtes
Fuzzing (nur deterministischer Byte-Flip-Test am CAT048-Decoder), FHA offen.

## 4. Roadmap zu 100 % (kumulierte Abdeckung nach jedem Häppchen)

| AP | Häppchen | Inhalt | Stufe · Modell | danach bei | Status |
|---|---|---|---|---:|---|
| **AP-QW · Quick Wins** | QW.1 | TrackId-u16-Trunkierung fixen: verwalteter Track-Nummern-Pool mit 60-s-Quarantäne (FR-TRK-035, ICD 3.1.1) | S3 · Fable 5 | **30,5 %** | ✅ 2026-07-10 |
| | QW.2 | Echtes Fuzzing: cargo-fuzz für CAT048/062/063/065-Decoder + `FIREFLY_SOURCES`-Parser, CI-Job. Erster Ertrag: u8-Überlauf in der FSPEC-FRN-Arithmetik gefunden & gefixt (`FspecTooLong`, NFR-SAFE-002) | S2–S3 · Fable 5 | **31,5 %** | ✅ 2026-07-10 |
| | QW.3 | I062/080-Vertrauens-Flags: MON (monosensor, gefenstert) + SPI (end-to-end aus CAT048 I048/020) + SIM-Slot dokumentiert; ICD 3.2.0 additiv, FR-TRK-036. *I062/295 bewusst weggelassen — dupliziert I062/290 (Betreiber-Freigabe 2026-07-10)* | S2–S3 · Fable 5 | **32,5 %** | ✅ 2026-07-10 |
| | QW.4 | PlotRecorder im Live-Pfad verdrahten: opt-in `FIREFLY_PLOT_RECORD_PATH`, nicht-fatal bei unöffenbarem Pfad; end-to-end am echten Server verifiziert (FR-OPS-006) | S2 · Opus 4.8 | **33,5 %** | ✅ 2026-07-10 |
| **AP-REG · Sensor-Registrierung** | REG.1 | ADR 0034 + Bias-Modell (Range/Azimut; Zeit-Offset bewusst Folge-Häppchen) + Offline-SVD-Schätzer über ICAO-gepaarte Korrespondenzen inkl. Beobachtbarkeits-Diagnose (FR-TRK-037) | S5 · Fable 5 | **36,5 %** | ✅ 2026-07-10 |
| | REG.2a | Online-**Schatten**-Monitor im Live-Server: gleitendes Fenster, Schätz-Kadenz, Ablehnung dünner Evidenz; opt-in `FIREFLY_REGISTRATION_ENABLED`, Logs + `firefly_registration_*`-Metriken — **ohne** Fusions-Rückkopplung (FR-TRK-038) | S4 · Fable 5 | **38 %** | ✅ 2026-07-11 |
| | REG.2b | Korrektur **vor der Fusion** mit Anwendungs-Politik (Beobachtbarkeit, signifikanter RMS-Gewinn, Plausibilitäts-Deckel, Hold + geglättete Übergänge; Tiefpass ohne Integrator — oszillationsfrei per Konstruktion) + Konvergenz-Tests; opt-in `FIREFLY_REGISTRATION_APPLY` (FR-TRK-039) | S5 · Fable 5 | **40 %** | ✅ 2026-07-11 |
| | REG.3 | Bias-Statistik auf den Draht: I063/080/081 (angewandte Korrektur, Absenz statt Null; I063/070/090–092 bewusst ungesendet — kein Zeit-/PSR-Bias geschätzt), byte-genaue Referenz-Vektoren, ICD 3.3.0 additiv (FR-IO-008) | S3 · Fable 5 (Empf. Sonnet; Schnittstellen-Wirkung) | **41,5 %** | ✅ 2026-07-11 |
| **AP-FEP · Sensoreingang** | FEP.1 | CAT034-Decoder (Nordmarke/Sektor, Compound-Skip, gefuzzt) + `ScanPeriodEstimator` (datenzeit-getrieben, Ausreißer-/Wrap-tolerant) → gemessene Scan-Periode speist CAT063-Staleness; Liveness ohne Verkehr (FR-IO-009, FR-NET-014). *Tracker-Löschkadenz bewusst noch statisch.* | S4 · Fable 5 | **45,5 %** | ✅ 2026-07-11 |
| | FEP.2 | Mode-S-DAPs: I048/250 (BDS 4,0/5,0/6,0, bit-genau mit Status-Bit-Disziplin) → Track-Führung mit Freshness → I062/380-Ausbau MHG/SAL/IAR/MAC (ICD 3.4.0 additiv, Wayfinder #238; FR-TRK-040). *BDS-5,0-Roll/GS geführt, IMM-Nutzung folgt.* | S4 · Fable 5 | **49,5 %** | ✅ 2026-07-11 |
| | FEP.3 | CAT021-Eingangsadapter (ADS-B-Bodenstation statt nur Internet-REST): ed-2.x-Decoder (49-FRN-UAP, Spare-FRN = lauter Editions-Fehler, gefuzzt) + `firefly-adsb021` (NACp→σ je Meldung, GBS/SIM/TST-Drop, kein Stations-Standort nötig); Quell-Kontrakt v1.6.0 additiv (`adsb_asterix`; FR-IO-010, FR-NET-015). *I021/160-Geschwindigkeit noch ungenutzt; nur ed 2.x.* | S4 · Fable 5 (Empf. Opus 4.8) | **52 %** | ✅ 2026-07-11 |
| | FEP.4 | CAT001/002-Legacy-Radar-Eingang: Decoder mit zweigeteilter CAT001-UAP (Plot/Track via TYP-Bit, Ablehnung ohne Selektor), trunkierte ToD am CAT002-Anker expandiert (nächst-kongruent, ohne Anker verworfen statt erfunden), gleicher Plot-/Service-Pfad wie CAT048/034, gefuzzt; kein Kontrakt-/ICD-Bezug (FR-IO-011, FR-NET-016). *I001/042/200/120 übersprungen; RFS abgelehnt.* | S3 · Fable 5 (Empf. Sonnet) | **53,5 %** | ✅ 2026-07-11 |
| | FEP.5 | CAT020/019 WAM/MLAT-Eingang: Decoder (28-FRN-UAP, σ je Meldung aus I020/500 SDP, gefuzzt; CAT019-NOGO/Liveness) + Crate `firefly-mlat` (Drop-Regeln RAB/SIM/TST/GBS, kein Standort nötig); Kontrakt v1.7.0 additiv (`mlat_asterix`; FR-IO-012, FR-NET-017). *Provenienz als Mode S (eigenes MLT-Subfeld = ICD-Bump, Folge-Häppchen); I020/202 ungenutzt.* | S4 · Fable 5 (Empf. Opus 4.8) | **55,5 %** | ✅ 2026-07-11 |
| **AP-VERT · Vertikal & Kinematik** | VERT.1 | SDPS-003: Meteo/QNH-Dienst — Crate `firefly-meteo`: regionaler QNH-Lookup (nächstgelegene anwendbare Region; ohne Region ehrlich Standardatmosphäre), exakte ICAO-Barometrie (Druckhöhe ↔ QNH-Höhe), env-Provider `FIREFLY_METEO_QNH` (Plausibilitätsband, Startfehler bei Kaputt-Konfiguration), Metriken (FR-TRK-041). *Live-METAR-Provider + Temperatur-Korrektur = Folge-Häppchen; Verwertung → VERT.2.* | S3 · Fable 5 (Empf. Sonnet) | **58,5 %** | ✅ 2026-07-11 |
| | VERT.2 | Höhen-Tracking + RoCD auf dem Draht: per-Track-Vertikal-Kalman im Druckhöhen-Raum (25-ft-Quantisierung, 5σ-Gating, Reinit nach 3 Rejects), geometrische Höhe strikt getrennt (`ModeAC.geometric_height_ft`, nur echt geometrische Quellen), QNH-Korrektur am Ausgang nur bei beobachteter Region (I062/135-QNH-Bit ehrlich); I062/130/135/220 (ICD 3.5.0 additiv, byte-genaue Vektoren; FR-TRK-042). *Per-Quelle-Gewichtung, Temperatur-Korrektur, BDS-6,0-Raten-Fusion = Folge-Häppchen.* | S5 · Fable 5 | **62,5 %** | ✅ 2026-07-11 |
| | VERT.3 | **Anzeige-Hälfte:** Mode of Movement + Beschleunigung → I062/200/210: Beschleunigungs-Schätzer (EWMA über d/dt der IMM-Kombinationsgeschwindigkeit, < 0,5-s-Samples übersprungen), TRANS aus CT-Modellwahrscheinlichkeiten (Drehung erst bei µ > 0,5), LONG along-track (0,2 m/s², erst ab 5 m/s), VERT aus Vertikal-Filter-Rate (±300 ft/min); I062/200 nur wenn eine Achse bestimmt, I062/210 i8 × 0,25 m/s² mit Sättigung (ICD 3.6.0 additiv, byte-genaue Vektoren; FR-TRK-043). *Die abgeleitete Beschleunigung dient nur der Ausgabe, nicht der Prädiktion — die **Tracking-Hälfte** (CA-Modell in der IMM-Bank) ist als eigenes Häppchen **VERT.4** ausgewiesen; keine Trend-Hysterese; ADF immer 0.* | S4–S5 · Fable 5 | **65 %** | ✅ 2026-07-11 |
| | VERT.4a | **Tracking-Hälfte, Fundament (ADR 0035, Weg A):** 6-D-Zustandsfundament fürs CA-Modell, **bewusst noch nicht verdrahtet** — `LinearKalman6` (Numerik-Spiegel des 4-D-Filters: Joseph-Form, `2π·√|S|`-Likelihood), 6-D-Transitionen mit ehrlicher Beschleunigungs-Aussage je Hypothese (CA voll gekoppelt; CV **Null-Zeilen** = „keine Beschleunigung"; CT **Zentripetal-Zeilen** `a' = ω·J·v'`, linear im Zustand — sonst ginge I062/210 in Kurven fälschlich gegen 0), White-Noise-**Jerk**-Q, Weg-A-Rand-Abbildungen (Einbettung 4-D → 6-D / exakte Marginal-Projektion 6-D → 4-D — der Fusionskern bleibt auf seinem 4-D-Vertrag, **kein** Kern-Umbau nötig: die Bank reicht nach außen nur die kombinierte Schätzung); Kernnachweis: Beschleunigung als Filterzustand aus reinen Positionsmessungen (FR-TRK-044). *Kein Wire-/Verhaltens-Bezug; Q-Tuning CV/CT und Jerk-Kalibrierung → 4b.* | S5 · Fable 5 | **65,5 %** | ✅ 2026-07-14 |
| | VERT.4b | **Tracking-Hälfte, Integration:** IMM-Bank vollständig auf `LinearKalman6` (Weg A: Mischung/Kombination/PDA dimensionsrein 6-D, 4-D-Marginale am Rand — Kern-Vertrag eingehalten), Default-Bank `cv_turns_and_ca` (CV klebrig 0,94; RMSE-Szenario-Test nachgetuned gehalten), modellspezifisches Q (CWNA-Block + Floor / Jerk), **I062/210 aus dem Filterzustand** (`combined_acceleration`; VERT.3-Ableiter bleibt nur Frische-Zeuge), End-to-End-Nachweise: Startlauf 2,5 m/s² ⇒ µ_CA > 0,7 + Zustand trifft Wahrheit; stationäre Kurve meldet Zentripetalwert (FR-TRK-044 verifiziert). *MMSE-Schrumpfung bei mehrdeutiger Evidenz dokumentiert; Snapshot-Layout gebrochen (vor HA.1 billig).* | S5 · Fable 5 | **66,5 %** | ✅ 2026-07-14 |
| **AP-SPEC · Sonderfälle** | SPEC.1 | Duplikat-ICAO-Auflösung + Koaleszenz-Behandlung (ADR 0036): ICAO-Fastpath kinematisch gegated (Identität = weicher Schlüssel; Duplikat gründet eigenen Track statt Teleport), Duplikat-Scan mit `identity_conflict`-Flag (ICAO **und** Squawk, WARN, nie Merge), **Koaleszenz-Wächter** (`decouple_coalescing_pairs`: 2σ-unauflösbare Paare → geteilte Hypothesen exklusiv dem stärkeren Anwärter; gemessen 113 m → 148–150 m gehalten, Negativ-Check), Registrierungs-Deckel (> 5-km-Korrespondenzen = Identitäts-Kontamination); FR-TRK-045, FR-TRK-031 revidiert. *Kein Split/Merge-Historien-Manager; Flag nur intern bis AP-FPL.* | S4 · Fable 5 | **69 %** | ✅ 2026-07-14 |
| | SPEC.2 | Räumliche Clutter-Karte + Reflexions-Heuristik (ADR 0037): per-Sensor-Polar-Raster (5 km × 64 Sektoren, exponentiell vergessene Ereignisrate τ = 600 s) aus unassoziierten Plots, **per-Track-λ in JPDA** (`joint_association_probabilities_local`), Reflexions-Verdacht bei Geburt (PSR-only, ±2°, ≥ 500 m hinter bestätigtem Track) ⇒ Bestätigungs-Schwelle +2 (nie exekutiert; SSR löscht); snapshot-fähig; FR-TRK-046. ***Design-Korrektur:** Floor = Default — ohne Expositions-Buchführung nur Hotspot-Anhebung (Erst-Entwurf riss den Zwei-Ziel-Regressions-Test); Sauber-Regionen-Absenkung + Metrik-Ausleitung = Folge-Häppchen.* | S4 · Fable 5 | **71 %** | ✅ 2026-07-14 |
| | SPEC.2b | **Verfeinerung (ADR-0037-Nachtrag):** Expositions-Buchführung — `mark_active` kreditiert Beobachtungszeit je Sensor-Batch (Lücken-Kredit max. 30 s, Feed-Ausfall reift nie); ab 1200 s Reife (2τ, je Zelle ab erstem Ereignis) sinkt der Floor auf 0,1 × Default — belegbar ruhige Regionen entlasten die Assoziation ehrlich, unreife Evidenz behält den Default (Gründungs-Plot-Regression konstruktiv geschützt). Metrik `firefly_clutter_cells` (On-Tick-Kette, TECHNICAL.md). Kein Fähigkeits-Zuwachs — 71 % bleibt. | S3 · Fable 5 | **71 %** | ✅ 2026-07-14 |
| **AP-FPL · Flugplan-Korrelation** | FPL.0 | ADR 0038 „Korrelation im SDPS vs. CWP": Korrelation ist **Server-Funktion, zentral in Firefly** (eine Zuordnung für alle Arbeitsplätze; ARTAS-konsistent; Zutaten — I062/245, Squawk, `identity_conflict`, Kinematik — liegen im SDPS); Wayfinder zeigt/bedient (manuelle Kommandos via API, FPL.2), Anzeige-Mandantierung bleibt dort. Korrelations-Regeln aus der Weeze-Notiz verbindlich (Callsign-first; Squawk nur eindeutig, nie bei `identity_conflict`, nie 1000; räumlich-zeitlich plausibilisiert). Abstimmung: Wayfinder #244 (**Vorbedingung EFS-1**; ADR ratifiziert mit Bestätigung). Kein Code, kein ICD-Bezug. | S3 · Fable 5 | **72 %** | ✅ 2026-07-15 |
| | FPL.1 | FPL-Eingang + Auto-Korrelation (ADR 0038 als Code): Crate `firefly-fpl` — env-Provider `FIREFLY_FLIGHT_PLANS` (Meteo-Muster: Kaputt-Konfiguration = Start-Fehler; Squawk **oktal wie geschrieben**, Ziffer 8/9 lauter Fehler; Duplikat-Callsign lauter Fehler), `CorrelationService` (Callsign-first normalisiert; Squawk nur eindeutig + nie 1000 + nie bei `identity_conflict` + Zeitfenster ±45 min; Verweigerung sichtbar), Anwendung **zustandslos je Output-Tick** am Ausgabe-Rand (`live::apply_correlation`, Tracker-Kern flugplan-frei), WS-JSON additiv (`identity_conflict`, `flight_plan`), Metriken `firefly_flight_plans`/`firefly_tracks_correlated`/`firefly_correlation_refused`; FR-TRK-047. *Kein ICD-Bezug (I062/390 = FPL.2); gehaltener Zustand + manuelle Übersteuerung = FPL.2; räumliche Plausibilität braucht Routen-Geometrie; Live-FDPS-Provider = Folge-ADR; Feldsatz wächst additiv nach Wayfinder #244.* | S5 · Fable 5 | **76 %** | ✅ 2026-07-15 |
| | FPL.2 | Flugplan aufs Kabel + manuelle Korrelation (ADR 0039): **I062/390** (FRN 21, Compound: CSN 7 Okt. ASCII, DEP/DST je 4 Okt.; nur bei korreliertem Track, unkorreliert byte-identisch — ICD 3.7.0, additiv, byte-genaue Vektoren + Decoder-Rückweg); **Kommando-API** `POST /correlation` (Plan-Pin, 422 bei unbekanntem Callsign; ohne Callsign = Pin auf unkorreliert — Automatik gesperrt) / `DELETE /correlation/{track}` (zurück zur Automatik) / `GET /correlation`; **manuell schlägt Automatik** je Output-Tick; **Pin stirbt mit TSE** (Draht-Nummern wiederverwendet, FR-TRK-035); Auth = `/ws`-Token nur als Bearer-Header (kein Query-Fallback), Origin-Check nur bei Browser-Kontext; Metrik `firefly_correlation_manual`; FR-TRK-048. *Pins flüchtig (Persistenz = HA.1); kein Benutzer-Audit (Attribution = Wayfinder); Subfelder wachsen additiv nach #244.* | S3–S4 · Fable 5 | **78 %** | ✅ 2026-07-15 |
| **AP-HA · SDPS-002** | HA.1 | Zustands-Snapshot + Wiederanlauf (ADR 0040): periodischer, **atomarer** JSON-Snapshot (`.tmp`+fsync+rename) des Arbeitszustands — Tracker-Kern (Tracks, IMM, Nummern-Pool, Clutter-Karten), Datenzeit, manuelle Pins (FPL.2) — je `FIREFLY_SNAPSHOT_PERIOD` (10 s) auf `FIREFLY_SNAPSHOT_PATH` (unset = aus, kaputte Knobs = Start-Fehler); Schreibfehler nicht fatal (WARN + Zähler, Wiederversuch); Restore beim Start hinter **drei Torwächtern** (Format-Version, **Konfigurations-Fingerprint** Referenzpunkt+Sensor-Liste, Alter ≤ `FIREFLY_SNAPSHOT_MAX_AGE` 300 s — jede Ablehnung laut, leerer Start; korrupter Inhalt nie Panic); Bild nach Neustart im nächsten Output-Tick zurück (vor dem ersten Plot), `/ready` bleibt am ersten Quell-Plot; Metriken `firefly_snapshot_writes_total`/`_errors_total`/`_age_seconds`/`firefly_restore`; FR-TRK-049. *Plots seit letztem Snapshot verloren (≤ Periode; Forensik = `.ffplots`); K8s-Volume = HA.3; Main/Standby-Sync = HA.2.* | S3–S4 · Fable 5 | **80,5 %** | ✅ 2026-07-15 |
| | HA.2a | Standby-Rolle + automatische Übernahme (ADR 0041): `FIREFLY_ROLE ∈ {main, standby}` (Tippfehler = Start-Fehler; Standby verlangt Feed + Heartbeat); Standby = Probes-only (`/ready` 503 „standby"), **kein** Senden, **keine** Quellen; **Heartbeat-Wache** auf CAT065 der eigenen SAC/SIC (fremde SDPS/Garbage re-armieren nie, NOGO = lebendig, Uhr ab Standby-Start — schon toter Main ⇒ Übernahme nach einem Timeout); Promotion bei Stille > `FIREFLY_FAILOVER_TIMEOUT` (3 s) = voller Live-Stack inkl. HA.1-Restore vom gemeinsamen Volume, eigener Heartbeat erst danach; `SO_REUSEADDR`-Rebind; End-to-End-Test über echtes UDP-Multicast; FR-TRK-050. *Timeout-Detektion, kein Konsens — Demotion/Split-Brain-Schutz + Failover-Metriken = HA.2b; gemeinsames Volume = HA.3.* | S4 · Fable 5 | **83 %** | ✅ 2026-07-15 |
| | HA.2b | Split-Brain-Schutz + Failover-Observability (ADR-0041-Nachtrag): **Startup-Arbitrierung** (main lauscht vor dem ersten Senden 1 Timeout; fremder Heartbeat der eigenen Identität ⇒ Standby statt Doppel-Feed; fail-open bei Socket-Fehler; Kaltstart +3 s) + **Laufzeit-Demotion crash-only** (fremder Heartbeat der eigenen SAC/SIC = Split-Brain-Evidenz; deterministischer Tie-Breaker: höhere Absender-Adresse weicht — genau eine Seite — und beendet sich mit Exit-Code 3; Supervisor-Neustart re-arbitriert in den Standby; Restart-Policy = Betriebs-Voraussetzung); Eigen-Erkennung über Egress-IP + Heartbeat-Socket-Port (unbestimmbar ⇒ Wache aus, nie Selbst-Kill); Metriken `firefly_role`/`firefly_failovers_total`/`firefly_main_heartbeat_age_seconds`; FR-TRK-050 erweitert. *Kein Konsens: echte Partition = zwei Sender bis zur Heilung (dokumentiert); Multi-homed-Restlücke dokumentiert.* | S4 · Fable 5 | **85 %** | ✅ 2026-07-15 |
| | HA.3 | Kubernetes-Deployment (NFR-OPS-002): Helm-Chart `deploy/helm/firefly/` + statisches kubectl-Äquivalent — erzwingt die ADR-0040/0041-Voraussetzungen strukturell: **eine ConfigMap für beide Instanzen** (Fingerprint-Disziplin), **RWX-Snapshot-PVC**, Deployments mit `Recreate` (Restart-Policy für die Exit-3-Demotion; kein Rolling-Split-Brain), **ein Service mit Readiness-Routing** (Standby-503 ⇒ Traffic folgt dem Failover ohne Eingriff), **`hostNetwork` + Pflicht-Anti-Affinity** als ehrlicher Multicast-Default (ADR 0017; Multus-Alternative dokumentiert), non-root/read-only-rootfs/Caps gedroppt, Secret-Muster, `deploy/validate.sh` (Syntax überall; helm lint + Voll-Render, wo Helm existiert) + `deploy/README.md` mit Begründungs-Tabelle; INSTALLATION §6a. *Kein Cluster-Smoke-Test im Repo (umgebungsspezifisch); Helm-Lint in der Sandbox nicht ausführbar (dokumentiert, via validate.sh in CI/Betreiber-Umgebung); Monitoring bewusst außerhalb; koppelt an Wayfinder ORCH-6.* | S3 · Fable 5 (Empf. Sonnet) | **86,5 %** | ✅ 2026-07-15 |
| | HA.4 | Auswertungs-Harness (FR-TRK-051; SASS-C für uns nicht verfügbar — Betreiber-Abstimmung 2026-07-15: **Aussagekraft über öffentliche ESASSP-Metrik-Definitionen** statt Werkzeug-Autorität): Crate `firefly-eval` (Lib + CLI, Text/JSON-Bericht, deterministisch) misst gegen **exakte Simulator-Wahrheit** (`TruthTrajectory` öffentlich) am **projizierten Ausgabe-Bild** (`snapshot_at` — Erst-Entwurf maß den Last-Update-Zustand, RMSE ×6 überschätzt, korrigiert): Track-PD, Positions-RMSE, Kontinuität (IDs/Switches), Falsch-Tracks, Bestätigungs-Latenz; Zuordnung greedy-exklusiv im 500-m-Gate; produktive Tracker-Konfiguration (`tracker_for`); **Instrument-Tests** (PD-Metrik beißt bei degradierter Detektion; vorenthaltene Wahrheit ⇒ Falsch-Track) + Regression-Gates am Ist-Stand (Single: PD ≥ 0,95, RMSE < 60 m). *Misst nur Simuliertes (kein Clutter-Modell); Live-Mitschnitte ohne Wahrheit = Folgearbeit; Frankfurt-Szene in die Suite = Folge-Häppchen.* | S4 · Fable 5 | **88,5 %** | ✅ 2026-07-15 |
| | HA.5 | **Unabhängiger Gegen-Check (NFR-SAFE-003, Betreiber-Abstimmung 2026-07-15):** dokumentierter, reproduzierbarer Workflow (`docs/verification/compass-gegen-check.md`) — PCAP-Mitschnitt per `tcpdump` (IGMP-Hinweis), Schnell-Sichtung via Wireshark-ASTERIX-Dissector (zweiter Fremd-Decoder), **OpenATS-COMPASS**-Import mit Prüf-Checkliste C1–C6 (0 Dekodier-Fehler, Kategorien, Item-Abdeckung gegen ICD 3.7.0 inkl. I062/390-nur-korreliert, Update-Raten, Track-/Korrelations-Konsistenz gegen `/metrics`) + Abgleich-Bericht-Template (je Lauf eingecheckt; Abweichungs-Klassifikation). *COMPASS-Lauf = GUI-gebundener Betreiber-/Abnahme-Schritt, kein CI-Gate; keine wahrheitsbasierte Genauigkeit aus Track-only-Daten (bleibt HA.4); Wiederholung je ICD-Bump.* | S2–S3 · Fable 5 (Empf. Sonnet) | **89 %** | ✅ 2026-07-15 (Verfahren; Betreiber-Lauf ausstehend) |
| **AP-CAP · Kapazität** | CAP.1 | Benchmark-Harness (NFR-CAP-001): criterion-Bench `firefly-eval/benches/load.rs` misst `process_plots` über komplette Plot-Ströme synthetischer Lastszenarien (`load_grid`: N Radare mit eigenen Site-Frames × M Ziele auf 5-km-Raster — separationstreu, Stressgröße Volumen), Tracker wie Live-Verdrahtung gebaut, Durchsatz in Plots/s; Generator gegen HA.4-Harness abgesichert (Test: alle Ziele je 1 Track, 0 Geister). **Baseline 2026-07-15:** 221 k (1R×10Z) … 114 k Plots/s (3R×100Z) ⇒ > 1500× Echtzeit-Reserve. *Host-abhängig (auf Zielhardware wiederholen); dichte Konflikt-Cluster (JPDA-Worst-Case) + Auslegungsgrenzen = CAP.2; kein CI-Zeit-Gate (flaky by design), Trends via criterion-Historie.* | S3 · Fable 5 (Empf. Sonnet) | **90,5 %** | ✅ 2026-07-15 |
| | CAP.2 | JPDA-Cluster-Kappe + dokumentierte Auslegungsgrenzen (FR-TRK-052): gemessener Worst-Case (dichte 120-m-Kolonne, ein Cluster: 8 Ziele 149 ms, 10 Ziele **27,8 s** je 60-s-Szenario — Echtzeitbruch); Kappe `MAX_CLUSTER_TRACKS=8`/`MAX_CLUSTER_PLOTS=10`, darüber degradiert **nur dieser Cluster** auf Pro-Track-PDA (exakte Einzeltrack-Formel, nur Cross-Track-Exklusivität entfällt; SPEC.1-Koaleszenz-Schutz bleibt) ⇒ 10er-Kolonne 0,75 ms; sichtbar via `firefly_jpda_cluster_cap_hits_total` + WARN (1. und jeder 100.); Zähler snapshot-kompatibel (`serde(default)`); Bench `dense_cluster`, Auslegungsgrenzen in TECHNICAL §11. *Ehrlich: oberhalb der Kappe gröbere Zuordnung (gemessene Kolonne ohnehin physikalisch unauflösbar: 2 Tracks vorher wie nachher); teuerster exakter Fall ≈ 160 ms an der Kappe; Host-abhängig.* | S4 · Fable 5 | **94 %** | ✅ 2026-07-16 |
| **AP-SRV · Server-Funktion** | SRV.1 | ADR „Arbeitsteilung Firefly+Wayfinder = SDPS-Server-Funktion" (**ADR 0042**, CAT252-Ersatz, rein dokumentarisch): Die ARTAS-Server-Leistungen (Subscription-Verwaltung, Zuschnitt je Konsument, adressierte Zustellung, Dienst-Status) sind als **Verbund-Leistung** festgeschrieben und je Zeile mit gelebtem Code/ADR belegt — Firefly = Fire-and-Forget-Multicast (konsumenten-blind), Wayfinder = Mandanten/Abos + fail-closed AOI/FL-Filter (WF2-21.2) + Auth-WS + Ingest-Gateway (dort ADR 0007) + Sensor-Mix via Instanz-je-Feed (dort ADR 0012); **Konsumenten-Matrix K1–K5** als Anschluss-Leiter (adressierte Dienste = Optionen); CAT252 in Firefly bewusst verworfen (Rand-Adapter bliebe möglich, neuer ADR). Spiegel: Wayfinder#257. *Ehrlich: kein CAT252-Endpunkt (K5 heute nicht bedienbar), zugeschnittene Dienste liefern JSON statt ASTERIX, keine Ratenanpassung je Konsument.* | S3 · Fable 5 (Empf. Opus) | **96 %** | ✅ 2026-07-16 |
| | SRV.2 | Laufzeit-Steuerung (Sensor an/aus, Service-Kommandos via API) + Supervision-Ausbau | S3 · Sonnet | **97,5 %** | ⏳ |
| **AP-ASSUR · Assurance** | ASSUR.1 | FHA/Hazard-Analyse (bestehender Roadmap-Punkt #7) | S4 · Opus 4.8 | **99 %** | ⏳ |
| | ASSUR.2 | Coverage-Messung + Property-Tests + Genauigkeits-Nachweisdossier | S3 · Sonnet | **100 %** | ⏳ |

## 5. Lesehinweise

- **Meilenstein-Schwellen:** Bei **~41 %** (nach AP-REG) ist Firefly erstmals
  mit *echten* Radaren fusionsfähig; bei **~66 %** (nach AP-VERT) ist der
  Track-Inhalt ARTAS-vergleichbar; bei **~88 %** (nach AP-HA) ist es
  betrieblich ein SDPS; der Rest ist Nachweis- und Feinschliff-Arbeit.
- **Reihenfolge-Logik:** REG vor FEP — jedes echte Radar, das vor der
  Bias-Korrektur angeschlossen wird, produziert Doppelbilder. Quick Wins und
  ASSUR-Häppchen sind jederzeit parallel ziehbar (Haiku/Sonnet-Spur).
- **Abhängigkeiten nach außen:** FPL.1 braucht die Wayfinder-Abstimmung
  (FPL.0 daher früh terminieren, obwohl das Epic spät kommt); FEP.3/FEP.5
  brauchen reale Datenquellen bzw. Aufzeichnungen; VERT.2 braucht VERT.1 (QNH).
- Die Prozentwerte tragen eine Unsicherheit von grob ±3 Punkten — sie sind ein
  Steuerungsinstrument, kein Messwert.
- **Verhältnis zu bestehenden Backlogs:** SDPS-001…004, Roadmap-Pakete #5/#7/#8
  (Wayfinder-`ROADMAP.md` §3) gehen in den APs auf: SDPS-001 → AP-FEP,
  SDPS-002 → AP-HA, SDPS-003 → VERT.1, SDPS-004 (STCA) bleibt eigenständig
  **nach** VERT (braucht saubere Kinematik) und ist hier bewusst nicht
  eingepreist (Safety-Nets sind in ARTAS ein separates System).
