# VERT.2 — Höhen-Tracking + RoCD → I062/135/130/220

> **Anforderung:** FR-TRK-042 · **ICD:** 3.5.0 (additiv) ·
> **Einstufung:** S5 · umgesetzt auf Fable 5

## Fachlich: Warum?

Bisher reichte Firefly die Höhe nur als **letzte gemessene** Flugfläche
durch (I062/136) — ungefiltert, ohne Vertikalgeschwindigkeit, ohne
QNH-Bezug. Ein ASD braucht mehr:

- eine **geglättete Höhe**: Mode-C ist auf 25 ft quantisiert und staircased
  — die Rohwerte springen, die gefilterte Höhe läuft ruhig mit;
- die **Steig-/Sinkrate (RoCD)**: Grundlage der Climb-/Descend-Pfeile im
  Label und jeder Vertikal-Konfliktlogik;
- unterhalb der Transition Altitude die **QNH-korrigierte** Höhe (der
  VERT.1-Dienst liefert das regionale QNH) — sonst ist die Anzeige bei
  kräftigem Tief > 800 ft falsch.

ARTAS führt genau diese Größen in I062/135 (barometrisch, QNH-Flag),
I062/130 (geometrisch) und I062/220 (RoCD).

## Technik

**Vertikal-Filter** (`firefly-track::vertical`, je Track): ein 2-Zustands-
Kalman **im Druckhöhen-Raum** — Zustand (Höhe ft, Rate ft/s), Konstant-
Raten-Modell mit CWNA-Manöverbudget (der Übergang Level ↔ Steigen ist genau
das Manöver, das dieses Budget kauft). Messung = jede Mode-C-/FL-Meldung
eines assoziierten Plots (Mess-σ 12 ft aus der 25-ft-Quantisierung plus
Realitäts-Zuschlag). **Gating** (5σ) verwirft Garbling-Ausreißer; **drei
konsekutive Rejects reinitialisieren** auf das neue Level — die klassische
Notluke des Vertikal-Kanals, ohne die ein echter Level-Sprung (nach langem
Coast oder Track-Wechsel) vom Gate für immer ausgehungert würde.
Datenzeit-getrieben (ADR 0003), serialisierbar (Snapshot/Restore, QW.4).

**Geometrische Höhe strikt getrennt:** `ModeAC.geometric_height_ft` (neu,
additiv) wird nur von Adaptern gesetzt, deren Quelle **echt geometrisch**
misst — ADS-B-Station (I021/140) und MLAT (I020/105). Der Track glättet sie
als eigene EWMA (α = 0,3). Barometrisch und geometrisch werden **nie
gemischt**: verschiedene Referenzen (Druckfläche vs. WGS-84-Ellipsoid),
deren Differenz selbst eine Information ist.

**Frische-Disziplin:** Beide Größen werden nur innerhalb des
30-s-Frische-Fensters berichtet (wie DAPs/Provenienz) — ein lange
gecoasteter Vertikal-Zustand wird zurückgehalten statt als aktuell gemeldet.

**QNH am Ausgang** (`apply_qnh` im Live-Pfad): Der Tracker bleibt im
Druckhöhen-Raum (ein QNH-Wechsel im Filter würde die Rate korrumpieren);
erst vor der Publikation wird je Track das QNH an der Track-Position
nachgeschlagen. **Nur ein beobachtetes regionales QNH** korrigiert (exakte
ICAO-Barometrie aus VERT.1) und setzt das I062/135-QNH-Bit — die
Standardatmosphäre lässt die Druckhöhe unverändert mit Bit 0. Nie eine
stille Schein-Korrektur.

**Draht (ICD 3.5.0, additiv):** I062/130 (FRN 18, i16 × 6,25 ft), I062/135
(FRN 19, QNH-Bit + 15-Bit-Zweierkomplement × 25 ft), I062/220 (FRN 20,
i16 × 6,25 ft/min, positiv = steigen). Absenz statt Null; ein Track ohne
Vertikal-Daten bleibt **byte-identisch** zur Vor-3.5.0-Form; I062/136
(gemessen) bleibt unverändert daneben — gemessen vs. gefiltert sind
verschiedene Aussagen. Byte-genaue Referenz-Vektoren in ICD §4.8.

## Schnittstellen-Wirkung

- **ICD 3.5.0, additiv** — drei neue optionale 2-Oktett-Items an
  Standard-UAP-Positionen; Wayfinder-Nachzug ohne Lockstep
  (Issue `from-firefly`: Decoder + geglättete Höhe/RoCD-Pfeil/
  QNH-Kennzeichnung im Label).
- Quell-Kontrakt unverändert.

## Ehrliche Grenzen (VERT.2)

- **Ein Filter-Satz für alle Quellen:** ADS-B-baro und Mode-C fließen in
  denselben Filter (beides Druckhöhen, gleiche 25-ft-Quantisierung) — eine
  per-Quelle-Gewichtung wäre eine spätere Verfeinerung.
- **Geometrische Höhe nur EWMA**, kein eigener Raten-Zustand — die
  RoCD kommt bewusst aus der barometrischen Kette (dort sitzt die
  operative Semantik).
- **Keine Temperatur-Korrektur** (kalte Atmosphäre ≠ ISA) — QNH ist der
  dominante Effekt; siehe VERT.1-Grenzen.
- **I062/220 aus dem Filter, nicht aus BDS 6,0:** die gemeldete
  Baro-Vertikalrate der Avionik (DAPs) wird geführt, aber bewusst nicht
  als I062/220-Quelle verwendet — eigene Messung schlägt Selbstauskunft;
  eine Fusion beider wäre ein Folge-Häppchen.
