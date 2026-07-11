# FEP.1 — CAT034-Decoder: Nordmarke, Sektor, gemessene Scan-Periode

> **Anforderungen:** FR-IO-009 (Decoder), FR-NET-014 (Verdrahtung) ·
> **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich: Warum?

Ein echter Radarkopf sendet auf demselben Feed wie seine Zielmeldungen
(CAT048) auch **Servicemeldungen (CAT034)**: eine **Nordmarke** pro
Antennenumdrehung und **Sektor-Meldungen** an den (typisch 32) Sektorgrenzen.
Firefly ignorierte sie bisher — mit zwei operativen Lücken:

1. **Statische Scan-Periode.** An der Scan-Periode hängen sicherheitsrelevante
   Schwellen (CAT063-Liveness = 2,5 × Periode, Frische-Fenster). Real driftet
   die Antennendrehzahl; der Abstand zweier Nordmarken *ist* die echte
   Umlaufzeit — der Datenstrom kalibriert seine eigenen Schwellen.
2. **Liveness nur über Verkehr.** Bisher galt ein Radar als tot, wenn 2,5
   Perioden kein *Plot* kam — bei leerem Himmel ein Fehlalarm, bei echtem
   Ausfall träge. Sektor-Meldungen beweisen die Lebendigkeit 32-mal pro
   Umdrehung, verkehrsunabhängig.

Genau das ist die **FEP-Funktion** (Front End Processor) in ARTAS:
Sensor-Überwachung aus dem Datenstrom selbst.

## Technik

**Decoder** (`firefly-asterix::cat034`, FR-IO-009): dekodiert I034/010
(SAC/SIC), I034/000 (Message Type; unbekannte Codes tolerant als `Other`),
I034/030 (ToD, 1/128 s), I034/020 (Sektornummer, LSB 360/2⁸ °), I034/041
(gemeldete Umlaufzeit). Alle übrigen Standard-Items werden **längen-korrekt
übersprungen** (Pro-FRN-Format-Modell wie beim CAT048-Decoder); die
Compound-Items I034/050/060 über Pro-Position-Subfeld-Längentabellen — ein
gesetztes **Spare**-Bit ist ein harter Fehler statt eines stillen
Fehl-Parses. Untrusted-Pfad: grenzen-geprüfter Cursor, kein Panic; neuer
Fuzz-Target `cat034_decode` (Seed-Korpus, CI; lokal 13 M Läufe ohne Befund).

**`ScanPeriodEstimator`** (`firefly-radar::service`, FR-NET-014; rein,
**datenzeit-getrieben** nach ADR 0003 — Replay reproduziert dieselben
Messungen):

- Plausibilitätsband **1–60 s** je Intervall (Duplikate und Feed-Lücken
  können die Schätzung nicht vergiften).
- **Mitternachts-Wrap** von I034/030 korrigiert (Charta §2: der Sprung ist
  ein Tageswechsel, kein Datenfehler).
- **Verpasste Nordmarke** (verlorenes Datagramm ⇒ verdoppeltes Intervall):
  Abweichung > 50 % vom aktuellen Schätzwert wird verworfen statt
  eingemittelt.
- Akzeptierte Intervalle: exponentielle Glättung (α = 0,25) — eine
  driftende Antenne wird gefolgt, ein Ausreißer ruckt nicht.

**Verdrahtung:** Der UDP-Listener dispatcht am führenden CAT-Oktett
(`0x22` = 34 → Service-Pfad, sonst CAT048 → Plot-Pfad, unverändert). Die
gemessene Periode ersetzt den Nominalwert als **CAT063-Staleness-Basis**
(`SensorHealthMonitor::update_scan_period`, wirksam in beide Richtungen:
eine real langsamere Antenne verhindert Fehlalarme, eine schnellere erkennt
Ausfälle früher); Metriken `firefly_radar_scan_period_seconds{sensor=…}` und
`firefly_radar_north_markers_total`; **jede** Servicemeldung zählt als
Sensor-Aktivität.

## Ehrliche Grenzen (FEP.1)

- Die **Tracker-Löschkadenz** bleibt beim konfigurierten Wert — die
  dynamische Nachführung in den Tracker-Kern ist ein eigenes, sorgfältig zu
  testendes Folge-Häppchen.
- Die vom Radar **selbst gemeldete** Umlaufzeit (I034/041) wird dekodiert,
  aber nicht als Schätzquelle verwendet — gemessen schlägt gemeldet.
- Sendet der Radarkopf keine Servicemeldungen, gilt unverändert das
  konfigurierte Verhalten.

## Schnittstellen-Wirkung

**Keine** — reiner Eingangs-Pfad; Ausgabe-ICD und `FIREFLY_SOURCES`-Kontrakt
unverändert (dieselbe UDP-Quelle, nur ein zusätzlicher Kategorie-Dispatch).

## Tests

10 Decoder-Tests (Referenz-Vektoren, Compound-Skip, Spare-Bit-Ablehnung,
Trunkierungs-/Fuzz-Regression), 5 Estimator-Tests (exakte Messung, sanftes
Folgen, verpasste Marke, Mitternachts-Wrap, Implausibles), 1 Listener-Test
(Dispatch), 2 Health-Monitor-Tests (Override beidseitig, Garbage ignoriert),
Metrik-Rendering. Gates: `cargo test --workspace`, `clippy`, `fmt` grün;
`cargo +nightly fuzz run cat034_decode` 45 s ohne Befund.
