# Functional Hazard Analysis (FHA) — Firefly SDPS

> **Anforderung:** NFR-SAFE-004 · **Stand:** 2026-07-16 (ASSUR.1) ·
> **Methodik:** qualitative FHA, orientiert an ED-153 / ED-109A und der
> EUROCONTROL-SAM-Systematik (FHA-Schritt) ·
> **Pflege-Regel:** siehe §7 — dieses Dokument wird bei jeder
> architektur-relevanten Änderung fortgeschrieben, nicht nur einmal erstellt.

## 0. Ehrliche Grenzen — zuerst

Dieses Dokument ist eine **qualitative Analyse, erstellt vom
KI-Assistenten und geprüft vom Projektverantwortlichen**. Es ist:

- **keine** unabhängige Sicherheitsbewertung durch einen Dritten,
- **keine** quantitative Risikoanalyse (keine Ausfallraten, keine
  Wahrscheinlichkeits-Ziele je Schwereklasse),
- **kein** Regulator-Nachweis — die verbindliche Schwere-Einstufung hängt
  vom **Betriebs-Kontext** ab (Luftraum-Klasse, Verkehrsdichte, verfügbare
  Rückfall-Verfahren wie prozedurale Staffelung), den nur der Betreiber /
  ANSP festlegen kann.

Was es leistet: die Funktionen und Versagensarten **systematisch und
vollständig benennen**, jede bereits gebaute **Barriere rückverfolgbar**
zu ADR/Anforderung/Test katalogisieren und die **Lücken sichtbar** machen
(§6) — die Vorstufe, auf der ein echtes Safety Assessment aufsetzen würde.

## 1. Systemabgrenzung

Betrachtet wird **Firefly** als SDPS: von der Annahme externer
Sensordaten (ADS-B/FLARM/Radar/WAM-Adapter) über Tracking/Fusion bis zur
Aussendung von CAT062/063/065 auf dem Multicast und der Kommando-Kante
(`/correlation`, `/sensors`). **Nicht** betrachtet: die Darstellung am
CWP (Wayfinder, eigene Verantwortung — dessen Staleness-Erkennung wird
hier aber als **externe Barriere** mitgeführt), die Sensoren selbst und
die Netz-Infrastruktur (Vertrauensgrenze: ADR 0017).

Leitunterscheidung der Fehlerbedingungs-**Typen** (klassisch für
Überwachungssysteme):

| Typ | Bedeutung | Grundsatz |
|-----|-----------|-----------|
| **V/e** | Verlust, **erkannt** (Bild weg, Konsument weiß es) | unangenehm — Rückfall-Verfahren greifen |
| **V/u** | Verlust, **unerkannt** (Bild weg, sieht aber „normal leer" aus) | gefährlich |
| **I/e** | irreführende Daten, **erkannt/erkennbar** (Konsument kann misstrauen) | beherrschbar |
| **I/u** | irreführende Daten, **unerkannt** (falsch, sieht aber richtig aus) | **gefährlichste Klasse** — der Lotse handelt auf falscher Grundlage |

Schwere-Klassen (angelehnt an SAM/ESARR-4-Sprachgebrauch, **vorbehaltlich
Betriebs-Kontext**, s. §0): **SK1** (Unfall begünstigend) · **SK2**
(ernste Störung) · **SK3** (erhebliche Störung) · **SK4** (geringe
Wirkung). Faustregel dieses Systems: unerkannt-irreführende Positions-/
Identitäts-Fehler tendieren zu SK1–SK2, erkannte Verluste zu SK3.

## 2. Systemfunktionen

| ID | Funktion | Kern-Bausteine |
|----|----------|----------------|
| **F1** | Luftlagebild berechnen und als CAT062 verteilen (Position, Geschwindigkeit, Track-Kontinuität) | Tracker (IMM/JPDA/Fusion), CAT062-Encoder, Multicast-Sender |
| **F2** | Identität führen (Mode 3/A, Callsign, ICAO, Flugplan-Korrelation) | Identitäts-Verwaltung, ADR 0038/0039, I062/060/245/380/390 |
| **F3** | Vertikallage liefern (Flugfläche, geometrische/barometrische Höhe, RoCD, QNH-Korrektur) | VERT-Kette, Meteo/QNH-Dienst, I062/136/130/135/220 |
| **F4** | Eigen-Status ehrlich melden (Dienst lebt/degradiert, Sensor-Liveness, Ausfallgründe) | CAT065-Heartbeat, CAT063 + SRC-REASON, `/ready`, Metriken |
| **F5** | Verfügbarkeit sichern (Wiederanlauf, Failover) | HA.1 Snapshot/Restore, HA.2 Main/Standby + Split-Brain-Schutz |
| **F6** | Eingangsdaten annehmen und dekodieren (unauthentifiziertes Netz!) | Quell-Adapter, ASTERIX-Decoder, Vertrauensgrenze ADR 0017 |
| **F7** | Betriebs-Eingriffe ausführen (Korrelations-Pins, Sensor-Gate) | Kommando-API, FR-TRK-048, FR-OPS-008 |

## 3. Gefährdungstabellen

Spalten: **Typ** (s. §1) · **SK** (indikativ) · **Barrieren** (gebaut,
mit Trace) · **Rest** (Restrisiko/Verweis §6).

### F1 — Luftlagebild

| ID | Fehlerbedingung | Typ | SK | Barrieren (Trace) | Rest |
|----|-----------------|-----|----|-------------------|------|
| H-F1-01 | Bild fällt komplett aus, Konsument **merkt es** | V/e | SK3 | CAT065-Heartbeat unabhängig vom Bildinhalt (ADR 0018); Wayfinder-Staleness + Feed-Banner + `/ready`-Kopplung (externe Barriere); K8s-Restart-Policy + Standby-Übernahme (ADR 0041, NFR-OPS-002) | akzeptiert (Rückfall-Verfahren = Betreiber) |
| H-F1-02 | **Eingefrorenes Bild**: Tracker-Task hängt, Heartbeat-Task lebt weiter — Konsument sieht „Dienst ok" + stehende Tracks | I/u | **SK1–2** | **Tracker-Fortschritts-Watchdog (SAFE.4, FR-OPS-009):** der CAT065-Heartbeat prüft vor jedem Senden den Output-Tick-Fortschritt und meldet **NOGO/degradiert**, wenn > 3 Output-Perioden kein Tick kam (`tracker_progress_stalled`, ERROR-Log + `firefly_heartbeat_degraded`; ICD 3.7.1); zusätzlich altert die Datenzeit im Strom (I062/070) sichtbar, Wayfinder-Track-Alterung | geschlossen (L1 ✅) |
| H-F1-03 | Systematisch **falsche Positionen** (Sensor-Bias, falscher Site-Standort in `FIREFLY_SOURCES`, falscher Referenzpunkt) | I/u | **SK1–2** | Registrierungs-Monitor + gegatete, geglättete Korrektur (REG.2a/b, ADR 0034); Konfig-Fingerprint verhindert Restore auf fremde Konfiguration (ADR 0040); Mess-Harness gegen Simulator-Wahrheit (FR-TRK-051); COMPASS-Fremd-Decoder-Gegen-Check als Verfahren (NFR-SAFE-003) | Verfahren statt Automatik: falscher, in sich konsistenter Site-Eintrag bleibt unerkannt bis zum Gegen-Check → **L2** |
| H-F1-04 | **Geister-Tracks** (Clutter, Multipath-Reflexionen, Duplikate) | I/e–I/u | SK2–3 | Bestätigungslogik (M-of-N, ADR 0012-Lebenszyklus); räumliche Clutter-Karte + Reflexions-Heuristik (ADR 0037, FR-TRK-046); Duplikat-/Koaleszenz-Wächter (ADR 0036); Falsch-Track-Metrik im Harness (FR-TRK-051, Instrument-Test) | akzeptiert, überwacht |
| H-F1-05 | **Track-Verlust einzelner Ziele** ohne Ende-Meldung (Ziel fliegt weiter, Track verschwindet still) | V/u | SK2 | TSE-Ende-Signalisierung — jeder gelöschte Track meldet sein Ende explizit (ADR 0016); adaptiver Lebenszyklus verhindert vorschnelles Löschen zwischen langsamen Scans (ADR 0012/0013) | akzeptiert |
| H-F1-06 | **Echtzeit-Bruch**: Rechenzeit explodiert im dichten Pulk, Bild hinkt Minuten hinterher | I/u → V/e | SK2 | JPDA-Cluster-Kappe: Worst Case gemessen und begrenzt (27,8 s → 0,75 ms), Zähler + WARN (FR-TRK-052); Durchsatz-Baseline > 1500× Echtzeit (NFR-CAP-001); Back-Pressure-Verlust gezählt statt blockiert (`firefly_live_plot_batches_dropped_total`) | akzeptiert; Alarmierung auf Zähler = MON.1 |
| H-F1-07 | **Falsche Zeitstempel** (I062/070): Bild wirkt aktueller/älter als es ist | I/u | SK2 | Echtes UTC-ToD (Issue #9, byte-genau referenz-getestet); Mitternachts-Sprung beidseitig als Normalfall dokumentiert (ICD); deterministische Datenzeit-Verarbeitung (NFR-REPRO-001, ADR 0003/0013) | akzeptiert |

### F2 — Identität

| ID | Fehlerbedingung | Typ | SK | Barrieren (Trace) | Rest |
|----|-----------------|-----|----|-------------------|------|
| H-F2-01 | **Identitätstausch** (Track Swap) bei Kreuzung/Formation | I/u | **SK1–2** | JPDA trägt Identität über den Geschwindigkeitszustand durch die Mehrdeutigkeit (M5-Showcase-Test); Identität als **weicher** Schlüssel + Koaleszenz-Wächter (ADR 0036); oberhalb der Cluster-Kappe gröber, aber gezählt + WARN (FR-TRK-052) | Restrisiko in extremen Pulks — sichtbar über Kappen-Zähler; Alarm = MON.1 |
| H-F2-02 | **Falsches Flugplan-Label** am Track (falscher Callsign → falsche Freigabe-Grundlage) | I/u | **SK1** | Weeze-Regelwerk (ADR 0038): Callsign-first; Squawk-Fallback **nur** eindeutig + nicht Conspicuity 1000 + kein `identity_conflict` + Zeitfenster; jede Verweigerung sichtbar (`firefly_correlation_refused`); I062/390 **nur** bei korreliertem Track (FR-TRK-047/048) | akzeptiert — bewusst „fehlendes Label vor falschem Label" |
| H-F2-03 | Manueller Pin **überlebt** seinen Track und landet auf dem nächsten Flugzeug (Draht-Nummern werden wiederverwendet) | I/u | SK1 | Pins sterben mit TSE (GC im Snapshot-Pfad, FR-TRK-048); Track-Nummern-Pool mit 60-s-Quarantäne (FR-TRK-035) | akzeptiert |
| H-F2-04 | Mode-3/A-**Duplikat** im Luftraum falsch zugeordnet | I/u | SK2 | Squawk-Eindeutigkeits-Regel (H-F2-02); Identitätskonflikt-Flag am Track (`identity_conflict`) | akzeptiert |

### F3 — Vertikallage

| ID | Fehlerbedingung | Typ | SK | Barrieren (Trace) | Rest |
|----|-----------------|-----|----|-------------------|------|
| H-F3-01 | **Stille Standardatmosphären-Behauptung**: Druckhöhe wird als QNH-korrigiert ausgegeben | I/u | SK2 | QNH-Bit in I062/135 **nur** bei beobachtetem regionalem QNH, sonst ehrlich Druckhöhe mit Bit 0 (VERT.1/2); konfigurierte-aber-kaputte QNH-Werte = Start-Abbruch | akzeptiert |
| H-F3-02 | **Falscher QNH-Wert** vom Betreiber konfiguriert (plausibel, aber falsch) | I/u | SK2 | Nur Plausibilitäts-Validierung beim Start; kein Abgleich gegen zweite Quelle | **L3 (§6)** — Verfahren |
| H-F3-03 | Veraltete Vertikaldaten wirken frisch | I/u | SK2–3 | 30-s-Frische-Fenster: I062/130/135/220 werden bei altem Schätzwert **weggelassen** statt eingefroren gesendet (VERT.2/3; Absenz = kein Anspruch) | akzeptiert |

### F4 — Eigen-Status

| ID | Fehlerbedingung | Typ | SK | Barrieren (Trace) | Rest |
|----|-----------------|-----|----|-------------------|------|
| H-F4-01 | „Leerer Himmel" nicht von „totem Feed" unterscheidbar | V/u | SK2 | Genau dafür gebaut: CAT065-Heartbeat (ADR 0018) + Wayfinder-Staleness | akzeptiert |
| H-F4-02 | Einzelner **Sensor-Ausfall unbemerkt** (Bild wird still dünner) | V/u | SK2–3 | CAT063 je Sensor (ADR 0022/0032, FR-IO-007) inkl. Ausfallgrund SRC-REASON (ADR 0033); `firefly_sensors_active` vs. `_total`; gemessene Scan-Periode speist die Staleness-Schwelle (FEP.1) | Alarm auf die Metrik = MON.1 |
| H-F4-03 | Heartbeat lügt (behauptet Leben trotz defektem Kern) | I/u | SK1–2 | = H-F1-02 (SAFE.4-Watchdog) | geschlossen (L1 ✅) |

### F5 — Verfügbarkeit

| ID | Fehlerbedingung | Typ | SK | Barrieren (Trace) | Rest |
|----|-----------------|-----|----|-------------------|------|
| H-F5-01 | Restore eines **fremden/veralteten** Zustands (falsche Sensorik, altes Bild als aktuell) | I/u | SK1–2 | Drei Restore-Tore: Format-Version, **Konfig-Fingerprint**, Alter — jede Ablehnung laut, Korruption = Ablehnung statt Panik (ADR 0040) | akzeptiert |
| H-F5-02 | **Split Brain**: zwei aktive Sender derselben Identität (springende/doppelte Tracks) | I/e | SK2–3 | Startup-Arbitrierung + Laufzeit-Demotion (deterministischer Tie-Break, Exit 3, Supervisor-Re-Arbitrierung, ADR 0041/HA.2b); ein Service, Readiness-Routing (NFR-OPS-002) | **ehrlich dokumentiert:** während einer echten Netz-Partition senden beide, bis sie heilt; Alarm auf `firefly_failovers_total` = MON.1 |
| H-F5-03 | Failover-Bild veraltet (Verlustfenster) | I/e | SK3 | Verlustfenster ≤ Snapshot-Periode + Failover-Timeout, dokumentiert (ADR 0040/0041); Datenzeit im Strom zeigt das Alter | akzeptiert |

### F6 — Eingang & Dekodierung

| ID | Fehlerbedingung | Typ | SK | Barrieren (Trace) | Rest |
|----|-----------------|-----|----|-------------------|------|
| H-F6-01 | **Feindliches/kaputtes Datagramm** stürzt den Dienst ab oder korrumpiert Zustand | V/e–I/u | SK2 | Kein Panic auf Eingabedaten, längen-geprüfte Decoder, FSPEC-Ketten-Obergrenze; **Fuzzing** mit echtem Befund + Fix (NFR-SAFE-002); Rust-Speichersicherheit ohne `unsafe` (NFR-SAFE-001) | akzeptiert |
| H-F6-02 | **Eingespeister falscher Verkehr** (Multicast hat keine Authentizität) | I/u | **SK1** | Vertrauensgrenze explizit: dediziertes, abgeschottetes Segment, Zutritt = Deployment-Pflicht (ADR 0017, NFR-SEC-001); `hostNetwork`-Rezept ohne Internet-Exposition (NFR-OPS-002) | Barriere ist **organisatorisch** (Netz), nicht kryptografisch — bewusste, dokumentierte Grenze |
| H-F6-03 | Externe Quelle liefert **plausiblen Müll** (z. B. ADS-B-Spoofing im Community-Feed) | I/u | SK2 | Multi-Sensor-Fusion dämpft Einzelquellen; Provenienz je Track (ADR 0027) macht die Quelle sichtbar; Sensor-Gate erlaubt sofortiges Herausnehmen (FR-OPS-008) | Erkennen bleibt beim Lotsen/Betreiber; ADS-B-Validierung gegen unabhängige Sensorik = möglicher späterer Baustein |

### F7 — Betriebs-Eingriffe

| ID | Fehlerbedingung | Typ | SK | Barrieren (Trace) | Rest |
|----|-----------------|-----|----|-------------------|------|
| H-F7-01 | **Unbefugtes Kommando** (Pin/Gate von außen) | I/u | SK1–2 | Bearer-Token-Pflicht (Header-only, kein Query-Leak), Origin-Check; `/status` ebenfalls gated (NFR-SEC-001, FR-TRK-048, FR-OPS-008) | Token-Verteilung/TLS = Deployment-Pflicht (ADR 0017) |
| H-F7-02 | **Vergessenes Sensor-Gate** dünnt das Bild dauerhaft aus | V/u | SK2 | Bewusst **flüchtig** (Neustart = alles aktiv, fail-open Richtung „mehr Daten"); WARN-Log + Gauge + `/status`-Liste (FR-OPS-008) | akzeptiert per Design |
| H-F7-03 | Falscher manueller Pin (Lotsen-Irrtum) | I/u | SK2 | 422 bei unbekanntem Plan; Pin sichtbar (`GET /correlation`, Metrik, `mode:"manual"` im Strom-Label-Kontext); Pin stirbt mit dem Track | menschlicher Faktor — CWP-Darstellungs-Pflicht (Wayfinder) |

## 4. Querschnitts-Barrieren

Nicht an eine einzelne Gefährdung gebunden, aber tragend für viele:

- **Determinismus nach Datenzeit** (NFR-REPRO-001, ADR 0003/0013): jeder
  Vorfall ist mit `.ffplots`/`.ffrec` (FR-OPS-005/006) **bit-genau
  nachstellbar** — die Voraussetzung, um jede der obigen Bedingungen im
  Nachhinein zu untersuchen.
- **Mess- statt Meinungs-Kultur:** Auswertungs-Harness mit
  Instrument-Tests („beißt die Metrik?", FR-TRK-051), Benchmarks mit
  dokumentierten Auslegungsgrenzen (NFR-CAP-001, FR-TRK-052, TECHNICAL §11).
- **Property-Tests auf den Kern-Invarianten** (NFR-ASSUR-001, ASSUR.2):
  Geodäsie-Roundtrip, LSB-genauer Draht-Roundtrip, Decoder-Totalität und
  Squawk-Parser werden je Testlauf gegen hunderte Zufallsfälle geprüft —
  wertgenau, nicht nur absturzfrei (Komplement zum Fuzzing NFR-SAFE-002).
  Deckt insbesondere die I/u-Zeilen H-F1-03/H-F1-07 zusätzlich ab
  (falsche Werte durch Encode/Decode-Fehler sind so praktisch
  ausgeschlossen; Coverage-Stand im Dossier §1).
- **Regression-Gates in CI:** PD/RMSE/Kontinuität als harte Asserts —
  eine Verschlechterung des Trackings fällt beim Commit auf, nicht im
  Betrieb.

## 5. Zusammenfassende Lesart

Die Klasse **„irreführend unerkannt" (I/u)** dominiert die kritischen
Zeilen — wie bei jedem Überwachungssystem. Fireflys Grundmuster dagegen
ist konsequent: **lieber ehrlich weglassen/ablehnen als plausibel
raten** (kein Label statt falsches Label, Absenz statt eingefrorener
Höhe, Restore-Ablehnung statt fremder Zustand, Verweigerung gezählt
statt still). Die verbleibenden echten Lücken stehen in §6 — keine ist
verdeckt.

## 6. Lücken-Register (abgeleitete Maßnahmen)

| ID | Lücke | Abgeleitete Maßnahme | Nachverfolgung |
|----|-------|----------------------|----------------|
| **L1** | Heartbeat ist vom Tracker-Fortschritt entkoppelt: ein hängender Tracker-Task sendet weiter „lebendig" (H-F1-02/H-F4-03) | **Tracker-Fortschritts-Watchdog:** CAT065 auf NOGO/degradiert schalten, wenn der Output-Tick ausbleibt | **✅ geschlossen (2026-07-16, SAFE.4/FR-OPS-009):** `tracker_progress_stalled` (> 3 Output-Perioden Stille, min 3 s, scharf erst nach dem ersten Tick), Draht-Wirkung per Test belegt (`degraded_answer_sets_nogo_on_the_wire`), ERROR-/Recovery-Log, `firefly_heartbeat_degraded`; ICD 3.7.1 (dokumentarisch) |
| **L2** | Konsistent falscher Site-/Referenz-Eintrag bleibt bis zum Gegen-Check unerkannt (H-F1-03) | Verfahren: COMPASS-Gegen-Check je Konfigurations-Änderung wiederholen (steht in `docs/verification/compass-gegen-check.md`); erster Betreiber-Lauf weiterhin offen | HA.5-Abnahme (Betreiber) |
| **L3** | Falscher, plausibler QNH-Wert (H-F3-02) | Verfahren: QNH-Eingabe gegen zweite Quelle (METAR) prüfen — als Betriebs-Hinweis in INSTALLATION §4f ergänzen, sobald eine Live-Meteo-Anbindung (eigener ADR) kommt | offen, geringe Priorität |
| **L4** | Detektions-Barrieren (Zähler/Metriken) alarmieren niemanden aktiv | Alertmanager-Regeln + Runbooks | **✅ geschlossen (2026-07-16, MON.1/NFR-OBS-004):** 12 Regeln in 3 Schweregraden (`monitoring/prometheus/alerts.yaml`), je Alarm ein Runbook; deckt u. a. H-F1-02 (FireflyTrackerStalled), H-F4-02 (FireflySensorOutage), H-F5-02/03 (FireflyFailover/SnapshotStale) und H-F1-06 (BackpressureLoss/JpdaCap) ab. *Restschritt beim Betreiber: Regeln in den Prometheus-Stack laden (Deployment-Schritt, kein Code).* |

## 7. Pflege-Regel

1. Jeder neue Baustein mit Sicherheits- oder Schnittstellen-Wirkung
   prüft **vor dem Merge**, ob er eine Zeile dieses Dokuments ändert
   (neue Funktion → §2, neue Fehlerbedingung/Barriere → §3, geschlossene
   Lücke → §6) — analog zur INSTALLATION-/TECHNICAL-Pflicht in den
   Qualitäts-Gates (CLAUDE.md §5).
2. Einstufungen (SK) bleiben **indikativ**, bis der Betreiber den
   Betriebs-Kontext festlegt; Änderungen daran sind Betreiber-Entscheid.
3. Geschlossene Lücken wandern nicht aus dem Dokument, sondern werden in
   §6 als erledigt markiert (Audit-Spur).
