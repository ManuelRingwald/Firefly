# Arbeitsstand (Handover-Notiz) — Firefly

> **Zweck:** Diese Datei beschreibt den **aktuellen IST-Stand** von Firefly.
> Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

> 🗺️ **Roadmap & Arbeitspakete:** siehe `docs/ROADMAP.md` im **Wayfinder-Repo**
> (zentrale Quelle für beide Repos). Cross-Project-Abhängigkeiten in
> `docs/cross-project/todo-for-firefly.md`.

---

## 🎯 Stand 2026-07-14 (SPEC.2 — Clutter-Karte + Reflexionen)

- **Zuletzt aktualisiert:** 2026-07-14
- **SPEC.2 (FR-TRK-046, ADR 0037):** Je Radar eine **räumliche
  Clutter-Karte** (Polar-Raster 5 km × 64 Sektoren, exponentiell
  vergessene Ereignisrate τ = 600 s, gelernt aus unassoziierten Plots,
  snapshot-fähig); JPDA assoziiert jeden Track unter dem **lokalen λ**
  seiner Zelle (`joint_association_probabilities_local` — Clutter-Term
  hängt im Joint-Event am Track). **Reflexions-Heuristik:** Primary-only-
  Neugeburt ±2° / ≥ 500 m hinter bestätigtem Track ⇒ Verdacht, nur
  Bestätigungs-Schwelle +2 (verzögert, nie exekutiert; SSR löscht).
  **Design-Korrektur unterwegs:** Erst-Entwurf ließ λ unter den Default
  sinken — Regressions-Test riss (Gründungs-Plots echter Ziele kippten
  eine knappe Assoziation); Wurzel: Event-only-Schätzer ohne Exposition
  kann „wenig Evidenz" nicht von „sauber" trennen ⇒ **Floor = Default**,
  nur Hotspot-Anhebung (Deckel 100×). 11 neue Tests, Gates grün
  (53 Suiten). Kein Wire-/ICD-Bezug. Roadmap: **71 %**.
- **Nächster Schritt:** **FPL.0** ankündigen — ADR „Korrelation im SDPS
  vs. CWP", Architektur-Abstimmung mit Wayfinder (**vor** deren EFS-1;
  S3, 72 %) — und Freigabe abwarten. (Die SPEC.2-Verfeinerung ist als
  **SPEC.2b** sichtbar in der Roadmap ausgewiesen — Expositions-
  Buchführung + Metriken, ohne Prozent-Zuwachs, jederzeit ziehbar.)

## 🎯 Stand 2026-07-14 (SPEC.1 — Duplikat-Identitäten + Koaleszenz)

- **Zuletzt aktualisiert:** 2026-07-14
- **SPEC.1 (FR-TRK-045, ADR 0036):** Identität ist jetzt ein **weicher
  Schlüssel** — der ICAO-Fastpath assoziiert nur noch im kinematischen
  Gate hart (Kern-Befund: vorher teleportierte ein Duplikat-ICAO den
  Träger-Track zwischen beiden Maschinen); Duplikat-Scan flaggt alle
  Träger gleicher ICAO/gleichen Squawks (`identity_conflict`, WARN, nie
  Merge; ORCAM/Weeze-Lektion). **Koaleszenz-Wächter** gegen die
  strukturelle JPDA-Schwäche: 2σ-unauflösbare Paare bekommen geteilte
  Hypothesen exklusiv zugeteilt — gemessen hält ein 150-m-Parallel-Paar
  148–150 m statt auf ≤ 113 m zusammenzudriften (Negativ-Check: Test
  beißt). **Registrierungs-Deckel:** > 5-km-Korrespondenzen verworfen
  (Duplikat vergiftet sonst die Bias-Schätzung); zeitfenster-basierte
  Erst-Idee verworfen (kann Duplikat nicht von Scan-Wiederbesuch
  unterscheiden). 5 neue/2 revidierte Tests, Gates grün (53 Suiten).
  Kein Wire-/ICD-Bezug. Roadmap: **69 %**.
- **Nächster Schritt:** **SPEC.2** ankündigen — räumliche Clutter-Karte
  + Reflexions-/Mehrwege-Heuristik (S4, 71 %) — und Freigabe abwarten.

## 🎯 Stand 2026-07-14 (VERT.4b — CA-Modell in der IMM-Bank)

- **Zuletzt aktualisiert:** 2026-07-14
- **VERT.4b — Integration (FR-TRK-044 verifiziert, ADR 0035 Weg A):**
  Bank vollständig auf `LinearKalman6` (Mischung/Kombination/PDA in 6-D),
  nach außen exakte 4-D-Marginale (Kern unberührt — Weg-A-Versprechen
  eingehalten). **Default-Bank `cv_turns_and_ca`**; nach anfänglichem
  RMSE-Riss (40,3 > 40,0 m) per Tuning gehalten (CV 0,94 klebrig,
  CA-Einstieg 0,02–0,03) statt Schwelle aufzuweichen. **I062/210 aus dem
  Filterzustand** (`combined_acceleration`: CT-zentripetal/CA-längs/CV-0);
  VERT.3-Ableiter nur noch Frische-Zeuge. Nachweise: Startlauf 2,5 m/s² ⇒
  µ_CA > 0,7 + Zustand ±0,3; stationäre Kurve meldet ω·v (±15 %).
  Ehrlich: MMSE-Schrumpfung bei mehrdeutiger Evidenz dokumentiert;
  Snapshot-Layout gebrochen (vor HA.1 billig). 2 neue IMM-Tests, Gates
  grün (53 Suiten). Roadmap: **66,5 %** — AP-VERT abgeschlossen,
  Track-Inhalt ARTAS-vergleichbar (Meilenstein-Schwelle erreicht).
- **Nächster Schritt:** **SPEC.1** ankündigen — Duplikat-ICAO-Auflösung +
  Split/Merge (S4; Vorarbeit `korrelation-code-duplikate-weeze.md`) —
  und Freigabe abwarten.

## 🎯 Stand 2026-07-14 (VERT.4a — 6-D-Fundament fürs CA-Modell)

- **Zuletzt aktualisiert:** 2026-07-14
- **VERT.4a — 6-D-Zustandsfundament (FR-TRK-044, ADR 0035 Weg A):**
  Erstes von zwei freigegebenen Häppchen der VERT.4-Tracking-Hälfte
  (Betreiber-Go 2026-07-14: „Weg A, 2 Häppchen"). Die Code-Inspektion vor
  ADR 0035 ergab: die IMM-Bank reicht nach außen **nur ihre kombinierte
  4-D-Schätzung** — der 6-D-Zustand `[E, N, vE, vN, aE, aN]` kann in der
  Bank **gekapselt** bleiben, der Fusionskern (Gating/JPDA/Registrierung)
  bleibt unverändert; **kein Kern-Umbau** nötig (Korrektur der VERT.3-
  Worst-Case-Annahme). Geliefert, bewusst **noch nicht verdrahtet**:
  `firefly-track::kalman6` — `LinearKalman6` (Numerik-Spiegel: Joseph-Form,
  `2π·√|S|`-Likelihood), 6-D-Transitionen mit ehrlicher Beschleunigungs-
  Aussage je Hypothese (CA voll gekoppelt; CV **Null-Zeilen**; CT
  **Zentripetal-Zeilen** `a' = ω·J·v'` — sonst ginge I062/210 in
  stationären Kurven fälschlich gegen 0), White-Noise-**Jerk**-Q
  (CWNA eine Ableitung höher), Rand-Abbildungen `from_kalman4`/`to_kalman4`
  (Einbettung/exakte Marginale, Identität auf (p, v)). Kernnachweis:
  0,5 m/s² aus reinen Positionsmessungen als **Filterzustand** geschätzt.
  7 neue Tests, Gates grün. Kein Wire-/ICD-/Verhaltens-Bezug.
  Roadmap-Stand: **65,5 %**.
- **Nächster Schritt:** **VERT.4b** umsetzen (bereits freigegeben, Teil
  desselben Go): Bank auf `LinearKalman6`, CA-Modell in `ImmConfig`
  (Tuning: Transition/Prior, Jerk-PSD), 4-D-Projektion am Bank-Rand,
  I062/210 aus dem Filterzustand, End-to-End-Tests; Snapshot-Layout-
  Wechsel (vor HA.1 billig). Start nach Merge des VERT.4a-PRs (Branch
  frisch von `origin/main`).

## 🎯 Stand 2026-07-11 (VERT.3 — Mode of Movement + Beschleunigung → I062/200/210)

- **Zuletzt aktualisiert:** 2026-07-11
- **VERT.3 — Kinematik-Trends auf dem Draht (FR-TRK-043, ICD 3.6.0 additiv):**
  Jeder Track führt jetzt einen **Beschleunigungs-Schätzer**
  (`firefly-track::acceleration`: EWMA α = 0,3 über den Differenzenquotienten
  konsekutiver **IMM-Kombinationsgeschwindigkeiten**; Samples < 0,5 s Abstand
  übersprungen — Multi-Sensor-Treffer verstärken sonst Jitter zu
  Phantom-Beschleunigung) und leitet den **Mode of Movement** ab
  (`Track::mode_of_movement`): **TRANS** aus den CT-Modellwahrscheinlichkeiten
  der IMM-Bank (Σµ der Dreh-Modelle; Drehung erst bei µ > 0,5; Bank ohne
  Dreh-Modelle → ehrlich `Undetermined`), **LONG** along-track (Schwelle
  0,2 m/s², erst ab 5 m/s), **VERT** aus der Vertikal-Filter-Rate
  (±300 ft/min, VERT.2). Frische-Disziplin 30 s je Achse. **Draht:**
  I062/200 (FRN 15, 1 Oktett TRANS/LONG/VERT/ADF=0) **nur wenn mindestens
  eine Achse bestimmt**; I062/210 (FRN 8, Ax/Ay i8 × 0,25 m/s², Sättigung);
  Track ohne beides byte-identisch alt; byte-genaue Vektoren in ICD §4.9.
  **⚠️ Abweichung von der Ankündigung — Scope-Split:** das **CA-Modell in
  der IMM-Bank wurde bewusst zurückgestellt** — ein 6-D-Zustand schneidet
  durch den gesamten 4-D-Fusionskern (LinearKalman/Matrix4/Vector4, Gating,
  JPDA, Registrierung; verifiziert). VERT.3 liefert damit die
  **Anzeige-Hälfte** (Trends + Beschleunigung auf dem Draht, aus der
  Ableitung); die **Tracking-Hälfte** (CA-Modell → bessere Prädiktion in
  Beschleunigungs-Phasen, Filterzustand statt Ableitung) ist als eigenes
  Häppchen **VERT.4** (S5, Kern-Refactor mit eigenem ADR) in der Roadmap
  ausgewiesen. Betreiber-Entscheidung 2026-07-11: Weg (A) — VERT.3 gilt als
  fertig, VERT.4 trägt die Differenz. Weitere ehrliche Grenzen: keine
  Trend-Hysterese (Glättung der Schätzer entprellt), ADF immer 0.
  **Wayfinder-Nachzug: Issue #242** (`from-firefly`; Decoder + WS-JSON +
  Kurven-/Trend-Indikator im Label). 5 neue Tests (Schätzer 2, Track 2,
  Encoder/Decoder 1), Gates grün, cat062-Fuzz-Smoke 7,0 M Läufe.
  Roadmap-Stand: **65 %** (VERT.3); AP-VERT voll bei 66,5 % nach VERT.4.
- **Nächster Schritt:** offen zwischen **VERT.4** (CA-Modell in die
  IMM-Bank, S5-Kern-Refactor mit eigenem ADR — neu ausgewiesen) und
  **SPEC.1** (Duplikat-ICAO-Auflösung + Split/Merge, S4; Vorarbeit
  `docs/design/korrelation-code-duplikate-weeze.md`). VERT.4 ist der
  schwerere Umbau (berührt den Fusionskern); SPEC.1 ist unabhängig davon
  ziehbar. Reihenfolge mit dem Betreiber abstimmen, dann das gewählte
  Häppchen per Charter ankündigen und Freigabe abwarten.

## 🎯 Stand 2026-07-11 (VERT.2 — Höhen-Tracking + RoCD → I062/135/130/220)

- **Zuletzt aktualisiert:** 2026-07-11
- **VERT.2 — Vertikal-Kette auf dem Draht (FR-TRK-042, ICD 3.5.0 additiv):**
  Jeder Track führt jetzt einen **Vertikal-Filter** (`firefly-track::
  vertical`, 2-Zustands-Kalman im Druckhöhen-Raum: Höhe + Rate; 5σ-Gating
  gegen Mode-C-Garbling, **Reinit nach 3 konsekutiven Rejects** — echter
  Level-Sprung statt Ausreißer) und eine strikt getrennte **geometrische
  Höhe** (neues `ModeAC.geometric_height_ft`, nur von echt geometrischen
  Quellen gesetzt: ADS-B I021/140, MLAT I020/105; EWMA α = 0,3;
  barometrisch/geometrisch nie gemischt). **Frische-Disziplin:** Ausgabe
  nur ≤ 30 s nach der letzten akzeptierten Vertikal-Messung. **QNH am
  Ausgang** (`apply_qnh` im Live-Pfad): nur ein **beobachtetes** regionales
  QNH (VERT.1) korrigiert (exakte ICAO-Barometrie) und setzt das
  I062/135-QNH-Bit — Standardatmosphäre ⇒ Druckhöhe, Bit 0. **Draht:**
  I062/130 (FRN 18, i16 × 6,25 ft), I062/135 (FRN 19, QNH-Bit + 15-Bit-ZK
  × 25 ft), I062/220 (FRN 20, i16 × 6,25 ft/min); Absenz statt Null, Track
  ohne Vertikal-Daten byte-identisch alt, I062/136 unverändert daneben;
  byte-genaue Referenz-Vektoren in ICD §4.8. Ehrliche Grenzen: ein
  Filter-Satz für alle Baro-Quellen; RoCD aus eigener Messung (BDS-6,0-
  Fusion = Folge-Häppchen); keine Temperatur-Korrektur. **Wayfinder-Nachzug:
  Issue #241** (`from-firefly`; Decoder + Label: geglättete Höhe,
  QNH-Kennzeichnung, RoCD-Pfeil).
  8 neue Tests (Filter 4, Track 1, Encoder/Decoder 2, apply_qnh 1), Gates
  grün, cat062-Fuzz-Smoke 5,5 M Läufe. Roadmap-Stand: **62,5 %**.
- **Nächster Schritt:** **VERT.3** ankündigen — Mode of Movement +
  Beschleunigung + IMM-Bank-Ausbau (CA-Modell) → I062/200/210 (S4–S5) —
  und Freigabe abwarten.

## 🎯 Stand 2026-07-11 (VERT.1 — Meteo/QNH-Dienst)

- **Zuletzt aktualisiert:** 2026-07-11
- **VERT.1 — Meteo/QNH-Dienst (FR-TRK-041, SDPS-003-Analogon):** Fundament
  der Vertikal-Kette. Neue Crate **`firefly-meteo`**: `QnhService`
  (regionaler Lookup — nächstgelegene anwendbare Region, Radius optional;
  **ohne anwendbare Region ehrlich `StandardAtmosphere`** — ein QNH wird
  nie erfunden), **exakte ICAO-Barometrie**
  `pressure_altitude_to_qnh_altitude` (Druckhöhe → Druck → QNH-Höhe,
  κ = 0,1902632; Identität bei Standard-QNH, Faustregel ~27 ft/hPa fällt
  im Test heraus), `MeteoConfig` (`FIREFLY_METEO_QNH` JSON;
  Plausibilitätsband [870, 1085] hPa, malform/implausibel = **Startfehler**,
  unset = leer + INFO). Server-Verdrahtung: Parse beim Start (fatal wie
  `FIREFLY_SOURCES`), Metriken `firefly_meteo_qnh_regions` +
  `firefly_meteo_qnh_hpa{region}`. **Kein Wire-/ICD-Bezug** — die
  Verwertung (QNH-korrigierte Höhe → I062/135, additiver ICD-Bump +
  Wayfinder-Issue) ist VERT.2. Ehrliche Grenzen: env-Provider (extern
  aktualisiert); Live-METAR-Provider + Temperatur-Korrektur =
  Folge-Häppchen. 8 neue Tests, Gates grün. Roadmap-Stand: **58,5 %**.
- **Nächster Schritt:** **VERT.2** ankündigen — Höhen-Tracking (Mode-C +
  geometrisch) + RoCD → I062/135/130/220, QNH-korrigiert (S5) — und
  Freigabe abwarten.

## 🎯 Stand 2026-07-11 (FEP.5 — CAT020/019-WAM/MLAT-Eingang)

- **Zuletzt aktualisiert:** 2026-07-11
- **FEP.5 — WAM/MLAT CAT020/019 (FR-IO-012 + FR-NET-017, Quell-Kontrakt
  v1.7.0 additiv):** Firefly empfängt jetzt **Multilateration** — die dritte
  operative Überwachungstechnologie; damit sind **alle vier klassischen
  ARTAS-Eingangsklassen** bedient (Radar 048/034 + 001/002, ADS-B 021,
  WAM 020/019). Neue Decoder `firefly-asterix::cat020`/`cat019`: Position
  I020/041 WGS84 hochauflösend (LSB 180/2²⁵ °), **σ je Meldung aus I020/500
  SDP** (Standardabweichung der Positionslösung, konservatives max(σx,σy);
  fehlend → 150-m-Default), CAT019 mit NOGO-Disziplin (nur 0 =
  operational-Anspruch). Neue Crate **`firefly-mlat`** (spiegelt
  `firefly-adsb021`): **Drop-Regeln** Feldmonitor (RAB)/SIM/TST/GBS bzw.
  positions-/zeitlos; Dispatch CAT020 → Plots, **CAT019 → CAT063-Liveness**
  (Statusmeldung = Aktivität; degraded/NOGO → WARN); kein Standort/bbox
  nötig. Verdrahtung: `mlat_asterix` (Kontrakt v1.7.0) oder
  `FIREFLY_MLAT_*` standalone; Sensor-Default 240, Port 8020, Metriken
  `firefly_mlat_reports_received_total`/`firefly_sources_mlat`. UAPs gegen
  asterix-specs/libasterix-Referenz verifiziert; Fuzz-Targets
  `cat020_decode`/`cat019_decode` (4,2 M/5,0 M Läufe ohne Befund).
  **Ausgabe-ICD unverändert.** Ehrliche Grenzen: Provenienz erscheint als
  Mode S (eigenes MLT-Age-Subfeld = ICD-Bump, Folge-Häppchen);
  I020/202-Geschwindigkeit ungenutzt. Wayfinder-Nachzug (Orchestrator-UI):
  **Issue #240** (`from-firefly`, analog #239). 22 neue Tests, Gates grün.
  Roadmap-Stand: **55,5 %**.
- **Nächster Schritt:** **VERT.1** ankündigen — SDPS-003 Meteo/QNH-Dienst
  (S3) — und Freigabe abwarten.

## 🎯 Stand 2026-07-11 (FEP.4 — CAT001/002-Legacy-Radar-Eingang)

- **Zuletzt aktualisiert:** 2026-07-11
- **FEP.4 — Legacy-Radar CAT001/CAT002 (FR-IO-011 + FR-NET-016):** Der
  `radar_asterix`-Eingang versteht jetzt auch die **Vorgänger-Generation**
  von CAT048/CAT034 — Bestandsradare werden ohne neue Quelle/Variablen
  angeschlossen (Dispatch am CAT-Oktett `0x01`/`0x02`). Neue Decoder
  `firefly-asterix::cat001`/`cat002`: CAT001 mit **zweigeteilter UAP**
  (Plot-/Track-Profil, Selektor TYP-Bit in I001/020; Record mit FRN ≥ 3
  ohne Selektor **abgelehnt statt geraten**), RHO-LSB 1/128 NM, Spare/RFS =
  harte Fehler; CAT002 liefert dasselbe `DecodedServiceMessage` wie CAT034
  (Typ 3 = Südmarker → `Other`, explizit gemappt). **Zeit-Anker:**
  CAT001-Zeit ist trunkiert (mod 512 s); der Listener ankert am letzten
  vollen ToD des Service-Stroms (`expand_truncated_tod`: nächst-kongruent,
  ±256 s tolerant, Mitternachts-Wrap); **ohne Anker verworfen statt
  erfunden**. Simulierte Meldungen (SIM) gedroppt (FR-TRK-036);
  CAT002-Nordmarken speisen den ScanPeriodEstimator (FEP.1) unverändert.
  UAPs gegen asterix-specs/libasterix-Referenz verifiziert; Fuzz-Targets
  `cat001_decode`/`cat002_decode` (5,5 M/7,0 M Läufe ohne Befund).
  **Kontrakt + Ausgabe-ICD unverändert** — kein Wayfinder-Nachzug. 25 neue
  Tests, Gates grün. Roadmap-Stand: **53,5 %**.
- **Nächster Schritt:** **FEP.5** ankündigen — CAT020/019 WAM/MLAT-Eingang
  (S4) — und Freigabe abwarten.

## 🎯 Stand 2026-07-11 (FEP.3 — CAT021-Eingang: ADS-B von der Bodenstation)

- **Zuletzt aktualisiert:** 2026-07-11
- **FEP.3 — CAT021-Eingangsadapter (FR-IO-010 + FR-NET-015, Quell-Kontrakt
  v1.6.0 additiv):** Firefly empfängt ADS-B jetzt auch von einer **eigenen
  Bodenstation** als **ASTERIX CAT021 über UDP** — der Produktions-Bezugsweg
  (Push statt Poll, lokal statt Internet-REST), wie ARTAS ihn konsumiert.
  Neuer Decoder `firefly-asterix::cat021` (Edition-2.x-UAP, 49 FRNs;
  track-relevante Items gelesen, alle übrigen längen-korrekt übersprungen;
  **Spare-FRN = lauter Editions-Fehler** statt stillem Fehl-Parse; Fuzz-Target
  `cat021_decode`, Smoke 5,3 M Läufe ohne Befund). Neue Crate
  **`firefly-adsb021`** (spiegelt `firefly-radar`): σ **je Meldung aus NACp**
  (DO-260B, σ ≈ EPU/2; fehlend/0 → konservative 250 m — schlechter als die
  75-m-Internet-Annahme), **Drop-Regeln** GBS/SIM/TST bzw. positions-/zeitlos;
  **kein Stations-Standort nötig** (geodätische Selbstmeldungen).
  Verdrahtung: `adsb_asterix` im Quell-Kontrakt (keine bbox, kein `cred_env`)
  oder `FIREFLY_ADSB021_*` standalone; Sensor-Default 230, Nominal 5 s,
  CAT063-Liveness, Metriken `firefly_adsb021_reports_received_total` /
  `firefly_sources_adsb021`. **Ausgabe-ICD unverändert** (reiner Eingang).
  Ehrliche Grenzen: nur ed 2.x (ältere Station scheitert laut);
  I021/160-Geschwindigkeit noch ungenutzt; als Einzelquelle
  `FIREFLY_SYSTEM_REF_*` setzen. Gates grün. Roadmap-Stand: **52 %**.
- **Nächster Schritt:** **FEP.4** ankündigen — CAT001/002-Legacy-Radar-Eingang
  (S3) — und Freigabe abwarten.

## 🎯 Stand 2026-07-11 (FEP.2 — Mode-S-DAPs: BDS 4,0/5,0/6,0 → I062/380)

- **Zuletzt aktualisiert:** 2026-07-11
- **FEP.2 — Mode-S-DAPs end-to-end (FR-TRK-040, ICD 3.4.0 additiv):** Die
  Downlink Aircraft Parameters eines EHS-Radars fließen jetzt vom
  CAT048-Eingang bis auf den CAT062-Draht. Neuer **BDS-Decoder**
  (`firefly-asterix::bds`, bit-genau nach ICAO Doc 9871, **Status-Bit-
  Disziplin**: kein Feld wird aus Nullen geraten): BDS 4,0 (Selected
  Altitude — Level-Bust-Basis), BDS 5,0 (Roll/Track/GS/TAS), BDS 6,0
  (Heading/IAS/Mach/Vertikalrate). CAT048 dekodiert I048/250 (Merge über
  Register); `Daps` auf `ModeAC` → Track (per-Feld-Merge + `daps_time`) →
  `SystemTrack.daps` **nur solange frisch** (30 s — Absenz statt
  Stale-Behauptung). **I062/380 jetzt echt compound:** MHG (#3), SAL (#6,
  SAS/MCP + 13-Bit-Zweierkomplement × 25 ft), IAR (#26), MAC (#27);
  DAP-loser Track byte-identisch alt, erst IAR/MAC verlängern die Spec auf
  4 Oktette; Decoder liest subfeld-getrieben zurück. 9 neue Tests inkl.
  byte-genauem Referenz-Dump; Fuzz-Smoke 9,7 M Läufe ohne Befund. Ehrliche
  Grenzen: BDS-5,0-Roll/GS geführt, IMM-Nutzung folgt; kein
  DAP-Konsistenz-Check. **Wayfinder-Nachzug: Issue #238** (`from-firefly`).
  Gates grün. Roadmap-Stand: **49,5 %**.
- **Nächster Schritt:** **FEP.3** ankündigen — CAT021-Eingangsadapter
  (ADS-B von der Bodenstation statt nur Internet-REST; S4).

## 🎯 Stand 2026-07-11 (FEP.1 — CAT034: Nordmarke/Sektor, gemessene Scan-Periode)

- **Zuletzt aktualisiert:** 2026-07-11
- **FEP.1 — CAT034-Servicemeldungen (FR-IO-009 + FR-NET-014):** Der
  Radar-Eingang versteht jetzt **CAT034** (Dispatch am CAT-Oktett auf
  demselben UDP-Socket wie CAT048). Neuer Decoder
  `firefly-asterix::cat034` (Nordmarke/Sektor/ToD/Sektornummer/gemeldete
  Umlaufzeit; Compound-Items I034/050/060 längen-korrekt übersprungen,
  Spare-Bit = harter Fehler; Fuzz-Target `cat034_decode`, 13 M Läufe ohne
  Befund). **`ScanPeriodEstimator`** (rein, datenzeit-getrieben): misst die
  echte Antennen-Umlaufzeit aus Nordmarken-Intervallen — Plausibilitätsband
  1–60 s, Mitternachts-Wrap korrigiert, verpasste Marke verworfen statt
  eingemittelt, exponentiell geglättet (α = 0,25). **Wirkung:** gemessene
  Periode ersetzt den Nominalwert als CAT063-Staleness-Basis
  (`update_scan_period`, beidseitig wirksam); jede Servicemeldung =
  Sensor-Aktivität (Liveness ohne Verkehr); Metriken
  `firefly_radar_scan_period_seconds{sensor}` +
  `firefly_radar_north_markers_total`. **Ehrliche Grenze:**
  Tracker-Löschkadenz bleibt statisch (eigenes Folge-Häppchen). Kein
  Wire-/ICD-Bezug. 18 neue Tests, Gates grün. Zusätzlich festgehalten:
  Design-Notiz **Squawk-Duplikate/Korrelation** (Weeze-Lektion des
  Betreibers) in `docs/design/korrelation-code-duplikate-weeze.md` — Vormerkung
  fürs spätere Korrelations-AP. Roadmap-Stand: **45,5 %**.
- **Nächster Schritt:** **FEP.2** ankündigen — Mode-S-DAPs: I048/250
  (BDS 4,0/5,0/6,0) dekodieren → I062/380-Ausbau (Selected Altitude,
  Heading, IAS/Mach; S4).

## 🎯 Stand 2026-07-11 (REG.3 — Bias-Statistik auf den Draht; AP-REG komplett)

- **Zuletzt aktualisiert:** 2026-07-11
- **REG.3 — CAT063-Bias-Items (FR-IO-008, ICD 3.3.0 additiv):** Bei aktiver
  Registrierungs-Korrektur (REG.2b) trägt der CAT063-Sensor-Status je Radar
  die **angewandte** Korrektur — **I063/080** (SRG=0 + SRB, LSB 1/128 NM ≈
  14,47 m) und **I063/081** (SAB, LSB 360/2¹⁶ ° ≈ 0,0055°), Sättigung statt
  Wrap. Sende-Regel: **nur bei in Kraft befindlicher Korrektur** (Absenz =
  „keine Korrektur", keine Null-Behauptung); ohne Korrektur byte-identisch
  zur alten Form, mit Korrektur FSPEC `0xBB 0x80` (16-Oktett-Record).
  Datenfluss: LiveTracker-Tick → `Metrics.registration_applied_biases` →
  Bias-Provider-Closure des `run_cat063_sender` (kein neuer geteilter
  Zustand). I063/070/090–092 bewusst ungesendet (kein Zeit-/PSR-Bias
  geschätzt). 5 neue Asterix-Tests (byte-genauer Referenz-Dump) + 1
  UDP-End-to-End-Test. Gates grün. Wayfinder-Nachzug: **Issue #237**
  (`from-firefly`, Decoder FRN 7/8 + Bias-Anzeige), referenziert in
  `docs/cross-project/todo-for-wayfinder.md`. Roadmap-Stand: **41,5 %** —
  **AP-REG komplett**.
- **Nächster Schritt:** **AP-FEP** beginnt — **FEP.1** ankündigen
  (CAT034-Decoder: Nordmarke/Sektor/Servicemeldungen → dynamische
  `scan_period`, Sensor-Liveness aus dem Datenstrom; S4).

## 🎯 Stand 2026-07-11 (REG.2b — Bias-Korrektur vor der Fusion; AP-REG-Kern komplett)

- **Zuletzt aktualisiert:** 2026-07-11
- **REG.2b — Korrektur vor der Fusion (ADR 0034, FR-TRK-039):** Der Kreis ist
  geschlossen — geschätzte Radar-Biases werden **vor** der Fusion abgezogen,
  abgesichert durch die **Anwendungs-Politik** (`ApplyPolicy` +
  `RegistrationApplier` in `firefly-track`): Gate = `observable` ∧
  RMS-nachher ≤ 0,5 × RMS-vorher ∧ |Δr| ≤ 1000 m ∧ |Δθ| ≤ 1°; angewandt =
  exponentieller Tiefpass (α = 0,3 je Lauf), Gate-Ausfälle 3 Läufe gehalten,
  dann Abklingen zur Null. **Oszillationsfrei per Konstruktion:** Monitor
  schätzt weiter auf dem rohen Strom (voller Bias), Korrektur = reiner
  Tiefpass — kein Integrator. Server: Korrektur vor `process_plots` (nur
  gelistete Radare), Applier rückt genau einmal je Schätzlauf vor
  (`runs_total`), `.ffplots` bleibt roh (Replay-Parität). **Doppeltes
  Opt-in:** `FIREFLY_REGISTRATION_APPLY` zusätzlich zu `_ENABLED`. Metriken:
  `firefly_registration_apply_active` + angewandte Bias-Gauges je Sensor
  (getrennt vom rohen Schätzwert). 6 neue Tests, darunter geschlossene Kette
  (monotone Konvergenz auf 150 m/0,3°, korrigierte Messung < 10 m neben der
  Wahrheit) und Server-End-to-End (korrigiertes Lagebild auf der Wahrheit,
  unkorrigiertes trägt den 800-m-Bias). **Kein Wire-/ICD-Bezug.** Gates grün.
  Roadmap-Stand: **40 %**.
- **Nächster Schritt:** **REG.3** ankündigen — Bias-Statistik auf den Draht
  (I063/070–092, Referenz-Vektoren, ICD-Bump; S3, additiv, Wayfinder-Issue).

## 🎯 Stand 2026-07-11 (REG.2a — Registrierungs-Schatten-Monitor im Live-Server)

- **Zuletzt aktualisiert:** 2026-07-11
- **REG.2a — Online-Schatten-Monitor (ADR 0034, FR-TRK-038):** Der
  REG.1-Schätzer läuft jetzt **live im Server mit — ohne die Fusion zu
  verändern**. `firefly-track::RegistrationMonitor` (rein,
  datenzeit-getrieben): gleitendes 120-s-Fenster registrierungs-nutzbarer
  Plots, Pairing/Schätzung in 10-s-Kadenz, Läufe mit < 20 Korrespondenzen
  werden abgelehnt. Server: `LiveTracker::with_registration`, `observe`
  bewusst **nach** der Tracker-Verarbeitung (Schatten belegt im Test:
  identische Snapshots mit/ohne Monitor); opt-in
  **`FIREFLY_REGISTRATION_ENABLED`** (ohne Radar-Quelle: Warn-Log, No-op).
  Observability: `info`-Log je frischer Schätzung + Metriken
  `firefly_registration_estimates_total`/`_correspondences`/`_observable`
  und gelabelte Bias-Gauges je Sensor (erst nach erster Schätzung). 3 neue
  Monitor-Tests (injizierte 150 m/0,3° aus dem Strom zurückgewonnen) + 4
  Server-/Metrik-Tests. **Kein Wire-/ICD-Bezug.** Gates grün.
  Roadmap-Stand: **38 %** (REG.2 in 2a ✅ / 2b ⏳ gesplittet).
- **Nächster Schritt:** **REG.2b** ankündigen — Korrektur **vor der Fusion**
  mit Anwendungs-Politik (nur bei `observable` + Mindest-Evidenz +
  signifikantem RMS-Gewinn; geglättete Übergänge) + Konvergenz-Tests.

## 🎯 Stand 2026-07-10 (REG.1 — Sensor-Registrierung: Fundament)

- **Zuletzt aktualisiert:** 2026-07-10 (spät nachts)
- **REG.1 — Bias-Schätzung offline (ADR 0034, FR-TRK-037):** Erstes Häppchen
  des AP-REG-Pakets (kritischster ARTAS-Gap: unkorrigierte systematische
  Radar-Fehler ⇒ Doppelbilder in der Fusion). Neues Modul
  `firefly-track::registration`: `SensorBias` (Range/Azimut,
  `gemessen = wahr + Bias`), Identitäts-Pairing über die ICAO-Adresse
  (Betreiber-Entscheid Option a; enges Zeitfenster, Zeit-Offset bewusst
  Folge-Häppchen), linearisierte **SVD-Kleinste-Quadrate** über die
  Lift-Residuen (`d = J_a·b_a − J_b·b_b`; Jacobi numerisch auf dem exakten
  Sensor→WGS84→Common-Lift; ADS-B-Selbstreports als bias-freie
  Referenzwahrheit), **Beobachtbarkeits-Diagnose** über das
  Singulärwert-Spektrum + RMS vor/nach. 9 Ground-Truth-Tests (injizierte
  150 m/0,3° unter Rauschen zurückgewonnen; Zwei-Radar-Fall ohne Referenz;
  Ko-Lokation als unbeobachtbar geflaggt). **Kein Live-Eingriff, kein
  Wire-Change** (REG.2 = Online-Korrektur, REG.3 = I063/070–092). Gates grün.
  Roadmap-Stand: **36,5 %**.
- **Nächster Schritt:** **REG.2** ankündigen — Online-Schätzung im Live-Pfad
  + Korrektur vor der Fusion (Akkumulations-Fenster, Anwendungs-Politik nur
  bei `observable` + signifikantem RMS-Gewinn, Metriken).

## 🎯 Stand 2026-07-10 (QW.4 — PlotRecorder im Live-Pfad; Quick-Win-Block komplett)

- **Zuletzt aktualisiert:** 2026-07-10 (Nacht)
- **QW.4 — PlotRecorder-Verdrahtung (FR-OPS-006, Betriebs-Härtung):** Der
  `.ffplots`-Eingangs-Recorder (ADR 0020) war unit-getestet, aber der
  Live-Server übergab `LiveTracker::new(tracker, None)` — zeichnete im echten
  Betrieb **nichts** auf (stale Kommentar „recorder wired in AP9.4c-4"). Jetzt:
  opt-in-Env **`FIREFLY_PLOT_RECORD_PATH`** → `resolve_plot_recorder` (reiner,
  testbarer Resolver in `live.rs`): unset/leer → kein Recording; gesetzter Pfad
  → Recorder an `LiveTracker`; **unöffenbarer Pfad → nicht-fatal** (Warn-Log,
  Server läuft weiter — Verfügbarkeit vor Aufzeichnung). Kein CAT062-/Wire-Bezug.
  **End-to-end am echten Server verifiziert** (Start mit gesetzter Env → Datei
  mit `FFPLOTS\0`-Header angelegt). 2 neue Tests + bestehender
  `recorder_captures_every_ingested_plot`; TECHNICAL §6.2 + INSTALLATION §7 +
  Register (FR-OPS-006 „verifiziert", FR-OPS-007 präzisiert). Milestone
  `QW4-PlotRecorder-Live-Wiring.md`.
- **✅ Quick-Win-Block (AP-QW) komplett** — QW.1…QW.4. Roadmap-Stand **33,5 %**.
- **Nächstes Paket: AP-REG (Sensor-Registrierung/Bias-Schätzung, S5)** — der
  anspruchsvollste offene Punkt, Voraussetzung für Fusion echter Radare ohne
  Doppelbilder. REG.1 (ADR + Bias-Modell + Offline-Schätzer) ankündigen.

## 🎯 Stand 2026-07-10 (QW.3 — I062/080 Vertrauens-Flags MON + SPI)

- **Zuletzt aktualisiert:** 2026-07-10 (spät)
- **QW.3 — Track-Status-Ausbau (FR-TRK-036, ICD 3.2.0, additiv):** I062/080
  trägt jetzt die ARTAS-Vertrauens-Flags. **MON** (Oktett 1, `0x80`):
  monosensor — der `Track` bucht je distinktem Sensor die letzte
  Treffer-Datenzeit (`sensor_hits`, gefenstert über `PROVENANCE_FRESH_S` =
  30 s statt des flatternden pro-Scan-Sets); ≤ 1 frischer Sensor ⇒ MON.
  **SPI** (Oktett 1, `0x40`): „Ident"-Puls **end-to-end** — CAT048-Decoder
  liest I048/020 Bit 3, `radar_asterix` reicht durch (`ModeAC.spi`), am Track
  bewusst transient (jede Meldung überschreibt). **SIM**-Slot dokumentiert,
  immer 0. Kein Wire-Bruch (Multisensor-Track ohne SPI byte-identisch zu
  3.1.x); Wayfinder-Folge additiv ohne Lockstep (`from-firefly`-Issue).
  **Zuschnitt:** I062/295 bewusst weggelassen (dupliziert I062/290,
  Betreiber-Freigabe). 7 neue Tests; Milestone `QW3-Track-Status_MON-SPI.md`.
  Gates grün. Roadmap-Stand: **32,5 %**.
- **Nächster Schritt:** QW.4 (PlotRecorder im Live-Pfad verdrahten, S2)
  ankündigen — letztes Quick-Win-Häppchen vor AP-REG (Sensor-Registrierung).

## 🎯 Stand 2026-07-10 (QW.2 Fuzzing — echter FSPEC-Bug gefunden & gefixt)

- **Zuletzt aktualisiert:** 2026-07-10 (Abend)
- **QW.2 — Coverage-geführtes Fuzzing der Vertrauensgrenzen (NFR-SAFE-002):**
  Neues `fuzz/`-Workspace (cargo-fuzz/libFuzzer, bewusst außerhalb des
  stabilen Workspace) mit fünf Targets: CAT048/062/063/065-Decoder +
  `FIREFLY_SOURCES`-Parser; Seed-Korpus aus den Referenz-Dumps; zeitgeboxter
  CI-Job „Fuzz" (60 s je Target, Crash-Artefakt-Upload). Bedienung:
  `fuzz/README.md`.
- **Erster Ertrag — echter Bug in Sekunden gefunden:** u8-Überlauf in der
  gemeinsamen FSPEC-FRN-Arithmetik (`fspec::parse`) — eine feindliche
  FX-Kette > 36 Oktette panickte (Debug) bzw. las stillschweigend falsche
  FRNs (Release), in **allen vier** ASTERIX-Decodern. Fix: Kette hart auf
  `MAX_FSPEC_OCTETS` = 36 begrenzt (FRN ≤ 252, jenseits jeder realen UAP),
  Überlänge ⇒ neue Fehler-Variante `FspecTooLong` je Decoder. 6 eingefrorene
  Regressionstests; Original-Crash-Eingaben verifiziert sauber; frischer
  Fuzz-Lauf ohne Funde; `sources_parse` > 5 Mio. Läufe ohne Befund. **Kein
  Wire-Bruch** (nur ohnehin undekodierbare Eingaben werden abgelehnt), ICD
  unverändert. **Wayfinder-Folge:** gleiche FSPEC-Härtung + Fuzzing für den
  Go-Decoder empfohlen (`from-firefly`-Issue). Roadmap-Stand: **31,5 %**.
- **Nächster Schritt:** QW.3 (I062/295 + I062/080-Bit-Ausbau, S2) ankündigen.

## 🎯 Stand 2026-07-10 (ARTAS-Gap-Roadmap + QW.1 Track-Nummern-Pool)

- **Zuletzt aktualisiert:** 2026-07-10
- **ARTAS-Gap-Analyse & Roadmap (`docs/design/artas-gap-roadmap.md`):** Firefly
  wurde vollständig (Code + Doku) gegen EUROCONTROL **ARTAS** als vollwertiges
  SDPS inventarisiert. Ergebnis: **≈ 30 % Fähigkeits-Abdeckung** (gewichtetes
  Modell im Dokument); die fünf größten Abstände sind Sensoreingang
  (CAT034/021/020, Mode-S-DAPs), **Sensor-Registrierung/Bias** (kritischster
  Punkt vor echten Radaren), 2-D-Tracker (Höhe/RoCD/QNH/MoM fehlen),
  Flugplan-Korrelation (I062/390 — bisher nirgends im Backlog!) und HA/
  Kapazitätsnachweis. Roadmap mit 10 Arbeitspaketen (AP-QW … AP-ASSUR) und
  kumulierten Prozent je Häppchen bis 100 %.
- **QW.1 — Track-Nummern-Pool für I062/040 (FR-TRK-035, ICD 3.1.1):** Erster
  Roadmap-Punkt umgesetzt. Die Draht-Track-Nummer war eine stille
  `u32→u16`-Trunkierung der internen `TrackId` (`cat062.rs`) — nach 65 536
  Track-Geburten drohten Draht-Kollisionen (zwei Flieger unter einer Nummer,
  TSE löscht beim Konsumenten den falschen Track). Jetzt: verwalteter Pool
  (`firefly-track::track_number::TrackNumberPool`) — frische Nummern ab 1
  (`0` nie), bei Löschung **60 s Datenzeit-Quarantäne** vor FIFO-
  Wiederverwendung, bei Erschöpfung (> 65 535 gleichzeitige Tracks) wird die
  Initiierung mit Warn-Log abgelehnt (ehrliche Grenze, TECHNICAL §11).
  `Track.number`/`SystemTrack.track_number` additiv; Encoder nutzt nie mehr
  die ID. Pool ist Teil des serialisierbaren Tracker-Zustands (ADR 0007,
  HA-Vorbau). **Kein Wire-Bruch** (u16 BE unverändert, ICD 3.1.1 rein
  dokumentarisch, Abschnitt 4.6 mit Konsumenten-Garantie); Wayfinder muss
  nichts nachziehen. 7 neue Tests (Pool, Tracker-Lebenszyklus, Encoder-
  Regression); Milestone `Track-Number-Pool_I062-040.md`. Gates grün
  (`cargo test --workspace`, clippy, fmt).
- **Nächster Schritt:** Roadmap-Reihenfolge — **QW.2** (echtes Fuzzing für
  CAT048/`FIREFLY_SOURCES`, S2–S3) ankündigen, nach „Go" umsetzen.

## 🎯 Stand 2026-07-06 (Nachmittag)

- **Zuletzt aktualisiert:** 2026-07-06
- **ADR 0033 — CAT063 per-Quelle-Fehlergrund (`SRC-REASON` im I063/RE, ICD 3.1.0,
  additiv):** Aufbauend auf ADR 0032 trägt ein **degradierter** Sensor mit
  bekanntem Grund den Ausfallgrund im **Reserved Expansion Field** (FRN 13, FSPEC
  dann `0xB9 0x04`): Vendor-Subfeld `SRC-REASON` (`1=unreachable`/`2=auth`/
  `3=rate_limited`), Layout `[LEN=0x03][0x80][code]`. **Nur** bei Degradierung
  mit Grund gesendet — operationelle Records bleiben 9 Oktette (additiv, kein
  Wire-Bruch; RE ist selbst-begrenzend). `SensorReason`/`SensorReport` in
  `firefly-asterix`; `SensorHealthMonitor::record_failure`/`record_activity`
  führen bzw. löschen den Grund pro Sensor; Klassifikation über die neuen
  `PollError::is_auth()` (OpenSky/adsbagg, HTTP 401/403) + bestehendes
  `is_rate_limited()`; sonst `unreachable`. FLARM/Radar liefern keinen Grund
  (ehrliche Grenze). Antwort auf Wayfinder #197 (Firefly #55, H3). Byte-genaue
  Referenz-Vektoren + Monitor-Tests; ICD Abschnitt 9 + Changelog 3.1.0; ADR 0033;
  FR-IO-007 erweitert. **Wayfinder-Folge H4:** RE-Reason dekodieren + Feed-Health-
  Chip → **Fixes #197** (rein additiv, kein Lockstep-Zwang).

## 🎯 Stand 2026-07-06

- **Zuletzt aktualisiert:** 2026-07-06
- **ADR 0032 — CAT063-UAP-Standardisierung (ICD 3.0.0, BREAKING):** Die
  CAT063-Sensor-Status-Records folgen jetzt den **echten EUROCONTROL-FRN-Slots**
  (spiegelt die CAT062-Korrektur aus ADR 0015). (1) I063/010 trägt die
  **SDPS**-Identität (SAC/SIC = `FIREFLY_CAT062_SAC`/`_SIC`, Default 25/2), nicht
  mehr den Sensor. (2) Neues I063/050 (FRN 4) trägt die **Sensor**-Identität
  (SAC 0, SIC = `sensor_id`). (3) I063/030 → FRN 3, I063/060 → FRN 5. FSPEC
  `0xE0` → **`0xB8`**, Record 7 → 9 Oktette; CON-Werte auf Standard korrigiert
  (`0` op / `1` degradiert / `2` init / `3` not-connected). Anlass: sauberes
  Fundament für den Grund-Code je ausgefallener Quelle (#197 → ADR 0033, RE-Feld,
  additiv). `Cat063Encoder::new(data_source, sensor_sac)`; `DecodedSensorStatus`
  trennt `data_source` (SDPS) und `sensor` (I063/050). **Wayfinder zieht in
  lockstep nach (H2)** — Firefly-first mergen+deployen, Wayfinder unmittelbar
  danach; Cross-Project via Firefly #55 (`from-wayfinder`). Byte-Referenz-Dumps
  + ICD-Abschnitt 9 auf 3.0.0-Form; FR-IO-007 erweitert.

## 🎯 Stand 2026-07-05

- **Zuletzt aktualisiert:** 2026-07-05
- **ADR 0031 — Community-Aggregator-ADS-B-Adapter (`adsb_aggregator`, #53):**
  Vierter Live-Quell-Adapter, Crate `firefly-adsbagg` — auth-freier ADS-B-Bezug
  über adsb.lol (Default) / adsb.fi (ADSBEx-v2-kompatibles API). Anlass: OpenSky
  verwirft Datacenter-IPs (Codespaces-Diagnose 2026-07-05); OpenSky bleibt
  vollwertig daneben (Anbieterwahl pro Quelle, kein Ersatz). BBox→Umkreis-Query
  (max 250 NM, Clamp mit WARN) + Rückfilter auf die BBox; `"ground"`/Staleness/
  `~`-Hex-Robustheit; 429-Backoff (Muster #49); Sensor-Default 230; Metriken
  `firefly_adsbagg_*`/`firefly_sources_adsbagg`. Kontrakt v1.5.0 (additiv,
  neues Feld `provider`; `cred_env` ignoriert). airplanes.live zurückgestellt
  (Radius-Einheit unverifiziert, ADR 0031). **Wayfinder zieht nach (#201):**
  Store-Vokabular + Orchestrator-Pass-through (`provider`) + UI-Typ
  „ADS-B (Community-Aggregator)" ohne Credential-Block.

## 🎯 Stand 2026-07-04

- **Zuletzt aktualisiert:** 2026-07-04
- **ADR 0030 — Replay-/Szenen-Modus ausgebaut:** Der Server läuft nur noch als
  quellen-getriebener Live-Tracker (`FIREFLY_SOURCES`/Opt-in-Adapter-Envs);
  `FIREFLY_MODE`/`FIREFLY_SCENE`/`FIREFLY_SPEED` werden ignoriert (Warn-Log).
  Ohne Quellen: leerer Himmel + CAT065-Heartbeat, `/ready` sofort bereit.
  OpenSky im Standalone-Fallback jetzt Opt-in (`FIREFLY_OPENSKY_ENABLED`) —
  kein Überraschungs-Egress beim nackten Start. Frankfurt-Regressionstests als
  Fixture nach `firefly-player/tests/frankfurt_regression.rs` umgezogen
  (Nachweise FR-TRK-018…023 lückenlos); `.ffplots`-Replay-Engine und
  `firefly_multicast::run` (Wire-Level-Tests) bewusst unangetastet. ICD 2.6.1
  (rein dokumentarisch, kein Wire-Bruch). **Wayfinder zieht nach** (eigener
  PR: `WAYFINDER_FIREFLY_SCENE`-Platzhalter + `docker-compose.bridge.yml`
  entfallen; Feed ohne Quellen → leerer Himmel statt Fake-Szene).

## 🎯 Stand 2026-07-03

- **Zuletzt aktualisiert:** 2026-07-03
- **Ist-/Gap-Analyse Service-Orientierung & HA (repo-übergreifend, Doku im
  Wayfinder-Repo):** `docs/design/gap-analyse-service-orientierung-ha.md`
  (Wayfinder) analysiert beide Systeme: System-Ebene bereits service-orientiert
  (CAT062-Vertrag, 1 Instanz pro Feed), Binnen-Ebene modulare Monolithen.
  **Firefly-relevante Befunde:** (a) 1 Instanz pro Feed = Single Point of
  Failure → **SDPS-002** (HA/State-Sync) bleibt die wichtigste betriebliche
  Lücke; (b) der `PlotRecorder` (ADR 0020, `.ffplots`-Replay als
  Wiederherstellungs-Weg) ist im Live-Pfad **nicht verdrahtet**
  (`crates/firefly-server/src/main.rs:329`, `LiveTracker::new(tracker, None)`)
  — als SDPS-002-Vorstufe einplanen (S3–S4); (c) Tracker-Strukturen sind
  serialisierbar, aber kein Snapshot/Restore-Codepfad existiert; (d) keine
  K8s-Manifeste (Probes/SIGTERM/12-Factor sind fertig vorbereitet). Empfohlene
  Reihenfolge und Backlog-Anker (WF2-52/53, ORCH-6, SDPS-002) im Dokument.
  Reine Doku, kein Code.

## 🎯 Stand 2026-07-02

- **Zuletzt aktualisiert:** 2026-07-02
- **OpenSky 429-Backoff (Issue #49, Branch `claude/wayfinder-tenant-radius-bug-w99r8q`):**
  Folge-Härtung zu ADR 0029 aus dem Wayfinder-E2E — ein rate-limitierter Feed wurde
  im festen Takt weitergepollt und provozierte weitere 429. Jetzt: `HTTP 429` als
  eigener `PollError::RateLimited` (erkannt vor `error_for_status`, `is_rate_limited()`,
  testbar); `OpenSkyPoller::run` nutzt eine kleine, reine `Backoff`-Zustandsmaschine
  (base=`poll_interval_secs`; bei Fehler ×2 wachsend, Cap 300 s bzw. ≥ base; Reset
  bei Erfolg); 429 bekommt eigenen Warn-Log + Metrik `firefly_opensky_rate_limited_total`
  (Teilmenge der Poll-Fehler, in der `on_error`-Closure gebumpt). **Rein
  Firefly-intern** — kein Wire-/Kontrakt-Change, kein ADR nötig. FR-NET-004 +
  FR-OBS-003 + TECHNICAL.md aktualisiert. Gates: `cargo test -p firefly-opensky`
  (22, +7) + `-p firefly-server metrics`, `clippy`/`fmt` grün.
- **Konfigurierbares OpenSky-Poll-Intervall (ADR 0029, Kontrakt v1.4.0, Branch
  `claude/wayfinder-tenant-radius-bug-w99r8q`):** Antwort auf Wayfinder-Wunsch #3
  (Poll-Schutz) — der E2E-Lauf lief anonym in **HTTP 429**, weil das Poll-Intervall
  fix bei 10 s lag und über `FIREFLY_SOURCES` nicht steuerbar war. Jetzt trägt
  `adsb_opensky` ein optionales **`poll_interval_secs`** (ganze Sekunden):
  `SourceSpec.poll_interval_secs: Option<u64>` (`#[serde(default)]`, additiv),
  `opensky_config_from_spec` übernimmt nur `> 0` (sonst Default 10 s — kein
  Heiß-Lauf, spiegelt `OpenSkyConfig::from_env`); die Ausgabe-Kadenz zieht via
  `representative_config` automatisch nach. Nur für `adsb_opensky` (FLARM ist Push,
  Radar hat eigene Scan-Periode). Kontrakt-Doku v1.4.0 + Changelog, ADR 0029,
  FR-NET-011 + Cross-Project-Todo aktualisiert. **Additiv & bidirektional
  kompatibel** (kein `deny_unknown_fields`) → Merge-Reihenfolge zu Wayfinder
  entkoppelt. Gates: `cargo test -p firefly-server` (26 sources-Tests, +3),
  `clippy`/`fmt` grün.
- **Hotfix (2026-07-02) — FLARM-Epoch-Zeitstempel (Wayfinder #120):** Ein
  **kombinierter ADS-B+FLARM-Live-Feed** lieferte keine Tracks, obwohl beide
  Quellen einzeln laufen. Root Cause: OpenSky stempelt Plot-Zeit als
  **Unix-Epoch** (`resp.time`), FLARM stempelte **Sekunden-seit-Mitternacht** —
  der gemeinsame monotone Datenzeit-Wasserstand des Multi-Source-Trackers verwarf
  daraufhin alle FLARM-Plots als „out-of-order". Fix in `firefly-flarm`
  (`position_to_plot`/`aprsis`): FLARM stempelt jetzt **Epoch-UTC** (OGN-Tageszeit
  an den Empfangstag verankert, Tageswechsel-Korrektur, Fallback Empfangszeit).
  Kein CAT062-Wire-Change. Doku: `docs/milestones/FLARM-Epoch-Time_Multi-Source-Fusion.md`,
  FR-NET-012. Alle Gates grün (`cargo test --workspace`, clippy, fmt).

## 🎯 Stand 2026-06-30

- **Zuletzt aktualisiert:** 2026-06-30
- **Großes Bild:** Die **Firefly-Seite des Quell-Eingangs-Kontrakts (#35)** ist
  **vollständig** — **alle drei** Vokabular-Typen haben Adapter: `adsb_opensky`
  (ADR 0019/0024), `flarm_aprs` (ADR 0026) und jetzt **`radar_asterix`** (ADR 0028,
  CAT048/UDP). Zusätzlich ist die **Per-Track-Provenienz** (#30, ADR 0027, CAT062
  I062/290 per-Technologie-Alter, ICD **v2.6.0**) geliefert und der erste
  **Betriebs-Härtung**-Block (Live-Pipeline-Observability). **#35 und #30 sind
  geschlossen.** Alles auf `main`, alle Gates grün (44 Test-Suites, clippy sauber).

- **Letzte Arbeit (2026-06-30, Vier-Themen-Batch):**
  1. **ADR 0027 — Per-Track-Provenienz** (#30, PR #43): `SourceKind` am Plot,
     `SystemTrack.source_ages` + abgeleitete `Provenance`; CAT062 I062/290 additiv
     um SSR/Mode-S/FLARM-Alter (ICD v2.6.0); JSON-Pfad führt `provenance`+`source_ages`.
     Bugfix: Treffer-Buchung fehlte an JPDA-Best/Track-Geburt. FR-TRK-034.
     Wayfinder-Folge #90.
  2. **ADR 0028 — `radar_asterix`-Adapter** (#35, PR #44): CAT048-Decoder
     (`firefly-asterix::cat048`, robust/fuzz-getestet, FR-IO-005) + Crate
     `firefly-radar` (FR-NET-013) + Verdrahtung (Radar-Sensor mit eigenem
     Standort-Frame). Kontrakt **v1.3.0** (`lat`/`lon` Pflicht). Wayfinder-Folge #91.
  3. **Wayfinder #57** (Wayfinder PR #92): View-Config-Formular-Captions
     (Zentrum/Zoom, AOI als harte Grenze, FL-Einheit + fail-open), FR-UI-013.
  4. **Betriebs-Härtung — Live-Pipeline-Observability** (NFR-OBS-003): Counter
     `firefly_live_plot_batches_dropped_total` (Back-Pressure-Verlust) + Gauges
     `firefly_sources_{opensky,flarm,radar}` (konfigurierter Quell-Mix).

- **Nächste Schritte:**
  1. **Zero-Touch-/Komplett-Setup-Abnahme** durch den Betreiber (steht an).
  2. **Wayfinder-Folge-Issues** #90 (I062/290-Decoder/Provenienz) und #91
     (Docker-Backend serialisiert `radar_asterix` lat/lon/listen) drüben umsetzen.
  3. **Betriebs-Härtung** weiter ausbauen (Lastfestigkeit/Deployment) nach Bedarf.

> 🗺️ Roadmap zentral im **Wayfinder-Repo** (`docs/ROADMAP.md`). Cross-Project:
> `docs/cross-project/todo-for-wayfinder.md`; offene `from-firefly`-Issues bei
> Wayfinder: #90 (Provenienz-Decoder), #91 (Radar-Quell-Serialisierung).

---

## ✅ Abgeschlossene Meilensteine

| Meilenstein | Inhalt | Status |
|---|---|---|
| **M1** | Simulator (ASTERIX-Szenarien, Track-Injection) | ✅ |
| **M2** | Single-Radar-Tracker (Kalman, Gate, JPDA, Lebenszyklus) | ✅ |
| **M3** | WebSocket-Server + JSON-Ausgabe (Live-Karte) | ✅ |
| **M4** | Multi-Radar-Fusion (Mess-Fusion, Sensormodell) | ✅ |
| **M5** | IMM/JPDA (Bewegungsmodelle, Assoziationen) | ✅ |
| **M6** | Showcase + Container (Deployment-ready) | ✅ |

---

## 📦 Produktions-Phase (laufend, ADR 0014)

### ✅ Fertig

| Feature | Status | Verweis |
|---|---|---|
| **UTC Time-of-Day** | ✅ I062/070 echte UTC-Tageszeit | Issue #9, geschlossen |
| **Multicast-Feed-Sicherheit** | ✅ ADR 0017 + WebSocket-Auth `/ws` | PR #27 |
| **System-Referenzpunkt** | ✅ I062/100 konfigurierbar via `FIREFLY_SYSTEM_REF_*` | ADR 0021 |
| **CAT062-ICD versioniert** | ✅ `docs/ICD-CAT062.md` v2.5.0 | Schnittstellen-Vertrag |
| **ADR 0013** | ✅ Asynchrone Pro-Plot + periodischer Ausgabetakt | 13.1–13.7 erledigt |
| **ADR 0015** | ✅ CAT062 Vertikallage I062/136 + UAP-Standard (FRN 27) | ICD 2.0.0 |
| **AP7/AP8** | ✅ CAT062 Callsign I062/245 | ICD 2.1.0, PR #15 |
| **ADR 0016** | ✅ CAT062 Track-Ende (I062/080 TSE) | ICD 2.2.0, PR #16 |
| **ADR 0018** | ✅ CAT065 SDPS-Heartbeat | ICD 2.3.0 |
| **ADR 0022** | ✅ CAT063 Sensor-Status (Per-Sensor-Liveness) | ICD 2.5.0, #32 |

### 🚧 Offen

Siehe zentrale **Wayfinder `ROADMAP.md`** für aktuelle Priorisierung (Prio 1 / Prio 2).

---

## 📋 Cross-Project-Abhängigkeiten (zu Wayfinder)

Siehe `docs/cross-project/todo-for-firefly.md`:

- **ORCH-5 (Live-Quell-Ingestion)** — generische Input-Adapter, Firefly-Arbeit
- **Per-Track-Sensor-Provenienz** — erfordert CAT062-ICD-Änderung
- **SWIM-Integration** — Abhängigkeit von Wayfinder EFS/IMS (Prio 2)
- **Ende-zu-Ende-HA** — Wayfinder WF2-52/53 ↔ Firefly SDPS-002

---

## 🔧 Technologie-Stack (ratifiziert)

- **Sprache:** Rust (ADR 0001)
- **Tracking:** Kalman-Filter + IMM/JPDA
- **Ausgabe:** CAT062 über UDP-Multicast (ADR 0006)
- **Deployment:** Docker + Kubernetes-ready (ADR 0003)

---

## 📚 Wichtige Dateien

- `docs/ICD-CAT062.md` — Schnittstellen-Vertrag mit Wayfinder (maßgeblich, versioniert)
- `docs/decisions/` — ADRs (0001–0022)
- `CLAUDE.md` — Arbeitsregeln
