# VERT.3 — Mode of Movement + Beschleunigung → I062/200/210

> **Anforderung:** FR-TRK-043 · **ICD:** 3.6.0 (additiv) ·
> **Einstufung:** S4–S5 · umgesetzt auf Fable 5

## Fachlich: Warum?

Ein Lotse liest aus dem Label nicht nur *wo* ein Ziel ist, sondern *was es
gerade tut*: dreht es, beschleunigt es, steigt oder sinkt es? ARTAS führt
dafür zwei Items:

- **I062/200 (Mode of Movement):** der qualitative Bewegungszustand in drei
  unabhängigen Achsen — **TRANS** (Kurs: konstant / Rechtskurve /
  Linkskurve), **LONG** (Grundgeschwindigkeit: konstant / zunehmend /
  abnehmend), **VERT** (Level / Steigen / Sinken). Grundlage für
  Kurven-Indikatoren im Label und jede Konfliktlogik, die „dreht bereits
  ein" von „hält Kurs" unterscheiden muss.
- **I062/210 (Calculated Acceleration):** die quantitative horizontale
  Beschleunigung (Ax/Ay) — Eingangsgröße für Trajektorien-Prädiktion
  jenseits der Konstant-Geschwindigkeits-Annahme.

## Technik

**Bewusste Architektur-Entscheidung — kein CA-Modell in der IMM-Bank.**
Der angekündigte Weg (Constant-Acceleration-Modell als drittes
IMM-Bank-Mitglied) hätte einen **6-D-Zustand** erfordert; der gesamte
Fusionskern ist aber auf den 4-D-Zustand festgelegt (LinearKalman auf
`Matrix4`, Gating, JPDA-Assoziation, Registrierungs-Schätzer). Ein
6-D-Umbau schneidet durch all das — für Größen, die eine geglättete
Ableitung der **bereits Kalman-gefilterten** IMM-Kombinationsgeschwindigkeit
mit weit geringerem Risiko liefert. Die CA-Bank-Erweiterung bleibt ein
**explizit zurückgestelltes Folge-Häppchen** (dann mit eigenem ADR, weil
sie den Fusionskern berührt).

**Beschleunigungs-Schätzer** (`firefly-track::acceleration`, je Track):
differenziert konsekutive IMM-Kombinationsschätzungen der Geschwindigkeit
über die **Datenzeit** und glättet per EWMA (α = 0,3). Samples mit Abstand
< 0,5 s werden **übersprungen** — der Differenzenquotient über eine
nahe-Null-Basis verstärkt Rest-Jitter zu Phantom-Beschleunigungen
(mehrere Sensoren treffen einen Track binnen Millisekunden). Datenzeit-
getrieben (ADR 0003), serialisierbar (Snapshot/Restore, QW.4).

**Trend-Ableitung** (`Track::mode_of_movement`), je Achse eigene Quelle:

- **TRANS (Kurs):** aus den **vorhandenen CT-Modellwahrscheinlichkeiten**
  der IMM-Bank — Σµ der Modelle mit positiver Drehrate (mathematisch
  positiv = **links**) gegen Σµ der negativen; Drehung wird erst behauptet,
  wenn die Dreh-Summe **µ > 0,5** trägt. Eine Bank ohne Dreh-Modelle
  (Single-Modell-Konfiguration) meldet ehrlich `Undetermined`.
- **LONG (Speed):** Projektion der Beschleunigung auf den Geschwindigkeits-
  Einheitsvektor (along-track); Schwelle 0,2 m/s², und erst ab 5 m/s
  Grundgeschwindigkeit — darunter ist „along-track" nicht wohldefiniert.
- **VERT:** Vorzeichen der Vertikal-Filter-Rate (VERT.2) mit Schwelle
  ±300 ft/min (unterhalb: Level).

**Frische-Disziplin:** jede Achse ohne frische Basis (30 s,
`PROVENANCE_FRESH_S`) meldet `Undetermined`; ein I062/200 wird nur
gesendet, wenn **mindestens eine** Achse bestimmt ist
(`ModeOfMovement::is_determined`) — ein All-Undetermined-Oktett wäre eine
leere Behauptung. I062/210 nur bei frischem Schätzwert.

**Draht (ICD 3.6.0, additiv):** I062/200 (FRN 15, 1 Oktett: TRANS Bits
8–7, LONG 6–5, VERT 4–3, **ADF Bit 2 immer 0** — Firefly bewertet keine
Altitude-Discrepancy); I062/210 (FRN 8, Ax/Ay als i8 × 0,25 m/s²,
**Sättigung statt Wrap** an den ±31,75-m/s²-Grenzen). Absenz statt Null;
ein Track ohne beides bleibt **byte-identisch** zur Vor-3.6.0-Form.
Byte-genaue Referenz-Vektoren in ICD §4.9.

## Schnittstellen-Wirkung

- **ICD 3.6.0, additiv** — zwei neue optionale Items an Standard-UAP-
  Positionen (FRN 8 im 2., FRN 15 im 3. FSPEC-Oktett — beide Oktette
  existieren schon, kein FSPEC-Wachstum); Wayfinder-Nachzug ohne Lockstep
  (Issue `from-firefly`: Decoder + Kurven-/Trend-Indikator im Label).
- Quell-Kontrakt unverändert; keine neuen Env-Variablen, keine neuen
  Metriken.

## Ehrliche Grenzen (VERT.3)

- **CA-Modell zurückgestellt** (siehe oben): die Beschleunigung ist eine
  geglättete Ableitung, kein Filter-Zustand — sie verbessert die
  IMM-Prädiktion selbst (noch) nicht.
- **Keine Hysterese an den Trend-Schwellen:** ein Wert, der um die
  Schwelle pendelt, kann den Trend flattern lassen — die EWMA-/Kalman-
  Glättung der zugrundeliegenden Schätzer übernimmt die Entprellung;
  eine explizite Hysterese wäre eine spätere Verfeinerung, falls das
  ASD-Label in der Praxis flackert.
- **ADF immer 0:** der Vergleich gemeldete vs. getrackte Höhe
  (Altitude-Discrepancy) ist nicht implementiert.
- **LONG unter 5 m/s stumm:** bei quasi-stehenden Zielen (Rollverkehr,
  Hubschrauber im Hover) wird kein Speed-Trend behauptet.
