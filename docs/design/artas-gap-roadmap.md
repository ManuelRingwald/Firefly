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
| **AP-SPEC · Sonderfälle** | SPEC.1 | Duplikat-ICAO-Auflösung + Split/Merge-Behandlung (JPDA-Koaleszenz aktiv korrigieren) | S4 · Opus 4.8 | **69 %** | ⏳ |
| | SPEC.2 | Räumliche Clutter-Karte + Reflexions-/Mehrwege-Heuristik | S4 · Opus 4.8 | **71 %** | ⏳ |
| **AP-FPL · Flugplan-Korrelation** | FPL.0 | ADR „Korrelation im SDPS vs. CWP" — Architektur-Abstimmung mit Wayfinder (**vor** deren EFS-1!) | S3 · Opus 4.8 | **72 %** | ⏳ |
| | FPL.1 | FPL-Eingangs-Kontrakt (minimales FDPS-Interface) + Code/Callsign-Korrelation | S5 · Fable 5 | **76 %** | ⏳ |
| | FPL.2 | I062/390-Encoding + manuelle Korrelations-Kommandos, ICD-Bump | S3–S4 · Opus 4.8 | **78 %** | ⏳ |
| **AP-HA · SDPS-002** | HA.1 | Snapshot/Restore produktiv (periodischer Tracker-Zustand + Wiederanlauf; Serde-Basis existiert) | S3–S4 · Opus 4.8 | **80,5 %** | ⏳ |
| | HA.2 | Main/Standby: Leader Election, State-Sync, unterbrechungsfreier Feed-Übergang | S5 · Fable 5 | **85 %** | ⏳ |
| | HA.3 | K8s-Manifeste/Helm, Deployment-Härtung (koppelt an Wayfinder ORCH-6) | S3 · Sonnet | **86,5 %** | ⏳ |
| | HA.4 | Auswertungs-Harness (SASS-C-artig): PD/RMSE/Kontinuität gegen Referenz aus `.ffrec`/`.ffplots` | S4 · Opus 4.8 | **88,5 %** | ⏳ |
| **AP-CAP · Kapazität** | CAP.1 | Benchmark-Harness (criterion) + synthetische Lastszenarien (N Sensoren × M Tracks) | S3 · Sonnet | **90,5 %** | ⏳ |
| | CAP.2 | Hot-Path-Optimierung (JPDA-Cluster-Grenzen) + dokumentierte Auslegungsgrenzen | S4 · Opus 4.8 / Fable 5 | **94 %** | ⏳ |
| **AP-SRV · Server-Funktion** | SRV.1 | ADR „Arbeitsteilung Firefly+Wayfinder = SDPS-Server-Funktion" (CAT252-Ersatz) + optionale adressierte Dienste je Konsument | S3 · Opus 4.8 | **96 %** | ⏳ |
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
