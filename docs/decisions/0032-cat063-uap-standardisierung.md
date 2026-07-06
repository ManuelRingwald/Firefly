# ADR 0032 — CAT063 UAP-Standardisierung (I063/010 SDPS, I063/050 Sensor)

- **Status:** akzeptiert
- **Datum:** 2026-07-06
- **Schnittstellen-relevant:** ja (CAT063-Ausgabe-Vertrag, ICD → 3.0.0, **breaking**)

## Kontext

CAT063 (Sensor Status Messages, ADR 0022) trägt seit ICD 2.5.0 den
Per-Sensor-Liveness-Bericht des SDPS. Die erste Implementierung nutzte eine
**kompaktierte, nicht-standardkonforme UAP-Nummerierung**:

| FRN (alt) | Item | Inhalt (alt) |
|-----------|------|--------------|
| 1 | I063/010 | Data Source Identifier — trug die **Sensor**-Identität |
| 2 | I063/030 | Time of Day |
| 3 | I063/060 | Sensor Configuration & Status |

→ FSPEC `0xE0`, 7-Oktett-Records.

Das weicht in **zwei** Punkten von der echten EUROCONTROL-CAT063-UAP
(SUR.ET1.ST05.2000-STD-04-01) ab:

1. **Falsche FRN-Slots.** Im Standard liegt I063/030 auf **FRN 3** (nicht 2) und
   I063/060 auf **FRN 5** (nicht 3). FRN 2 ist I063/015 (Service Identification),
   FRN 4 ist I063/050 (Sensor Identifier). Firefly hatte die Items
   „linksbündig" durchnummeriert — genau der Fehler, den ADR 0015 für die
   CAT062-UAP schon einmal korrigiert hat.
2. **Semantisch falsches I063/010.** In CAT063 identifiziert I063/010 das
   **meldende SDPS** (dieselbe SAC/SIC wie I062/010 und I065/010) — es sagt
   *wer* meldet. *Welcher* Sensor gemeint ist, steht im separaten I063/050
   (Sensor Identifier). Firefly stopfte die Sensor-Identität in I063/010 und ließ
   I063/050 ganz weg — ein konformer Fremd-Decoder hätte jeden Record demselben
   „Sensor" (= dem SDPS) zugeordnet.

Zusätzlich war der Doc-Kommentar der CON-Werte vertauscht (`0x80`/`0xC0`).

Der Anlass, das jetzt zu korrigieren: Wayfinder-Issue #197 wünscht einen
**Grund-Code** je ausgefallener Quelle (unreachable / auth / rate-limited). Der
saubere Träger dafür ist das **Reserved Expansion Field (RE)** der CAT063-UAP
(ADR 0033, additiv). Ein RE-Feld auf einer bereits verbogenen UAP aufzusetzen
würde den Standard-Verstoß zementieren. Also zuerst die UAP geradeziehen
(dieser ADR), dann additiv erweitern (ADR 0033).

## Entscheidung

Die CAT063-Records folgen den **echten EUROCONTROL-FRN-Positionen**:

| FRN | Item | Länge | Inhalt |
|-----|------|-------|--------|
| 1 | I063/010 | 2 | Data Source Identifier — die **SDPS**-Identität (SAC/SIC = `FIREFLY_CAT062_SAC`/`_SIC`, Default 25/2). |
| 3 | I063/030 | 3 | Time of Day (1/128 s, wall-clock). |
| 4 | I063/050 | 2 | Sensor Identifier — die **Sensor**-Identität (SAC 0, SIC = `sensor_id`). |
| 5 | I063/060 | 1+ | Sensor Configuration & Status (CON + GO/NOGO, variabel via FX). |

→ FSPEC `0xB8`, 9-Oktett-Records.

Die CON-Werte (I063/060, Bits 8–7) werden auf die Standard-Kodierung gebracht:
`0` = operationell, `1` = degradiert, `2` = Initialisierung, `3` = nicht
verbunden. Firefly sendet weiterhin nur `0x00` (operationell) und `0x40`
(degradiert); `INITIALISATION`/`NOT_CONNECTED` sind für die spätere
Grund-Code-Ableitung (ADR 0033) definiert.

Der `Cat063Encoder` bekommt die SDPS-Identität (`DataSourceId`) und die
Sensor-SAC im Konstruktor; das Multicast-Config-Wiring reicht die bereits
vorhandene `data_source()` (I062/010-Identität) durch. `DecodedSensorStatus`
trägt jetzt `data_source` (SDPS) **und** `sensor` (I063/050) getrennt.

## Begründung

- **Standardtreue.** Beide Enden (Firefly + Wayfinder) sind kontrolliert und
  vor-produktiv — der günstigste Zeitpunkt, den UAP-Fehler zu korrigieren,
  statt ihn einzufrieren. Ein konformes ASD/Recording-Tool liest den Strom ohne
  privates Profil (erklärtes Ziel, ADR 0006/0014).
- **Semantische Korrektheit.** SDPS-Identität (wer meldet) und Sensor-Identität
  (worüber) sind jetzt getrennte Felder — genau wie im Standard. Das ist die
  Voraussetzung dafür, dass mehrere SDPS-Instanzen denselben Strom speisen
  könnten, ohne dass Sensor-IDs kollidieren.
- **Fundament für ADR 0033.** Erst auf der geraden UAP ergibt das additive
  RE-Feld (Grund-Code) Sinn; es kommt auf einem echten hohen FRN mit
  Längen-Präfix (RE ist selbst-begrenzend).

## Konsequenzen

- **Breaking Wire-Change (ICD 3.0.0).** FSPEC `0xE0` → `0xB8`, Record 7 → 9
  Oktette, Sensor-Identität wandert von I063/010 nach I063/050. **Wayfinders
  CAT063-Decoder muss in lockstep nachziehen** (H2): Sensor aus I063/050 lesen,
  FSPEC `0xB8` erwarten, RE/SP-Präsenzbits längen-tolerant überspringen. Firefly
  wird **zuerst** gemergt und deployt, Wayfinder unmittelbar danach —
  dazwischen dekodiert der alte Wayfinder-Decoder die neuen Blöcke nicht
  (Sensor-Degradierungs-Banner fällt kurz aus; Tracks/CAT062 unberührt).
  Cross-Project via Firefly-Issue #55 (`from-wayfinder`).
- **Kein Eingriff in CAT062/CAT065.** Nur CAT063 ändert sich.
- `Cat063Encoder::new` bekommt die Signatur `(data_source: DataSourceId,
  sensor_sac: u8)`; Aufrufer (`MulticastConfig::cat063_encoder`) reicht
  `self.data_source()` durch.
- Anforderungen: **FR-IO-007** erweitert (Standard-UAP, I063/050, FSPEC `0xB8`).
- Byte-genaue Referenz-Dumps neu berechnet (`0xB8 0x19 0x02 … 0x00 0x01 0x00`,
  LEN 12 statt 10); ICD-Abschnitt 9 auf die 3.0.0-Form gebracht.

## Ehrliche Grenze

Firefly emittiert weiterhin nur ein bewusstes **Subset** der CAT063-UAP
(I063/010/030/050/060). I063/015 (Service Identification), die Bias-/
Statistik-Items (I063/070–092) und die volle GO/NOGO-Bitmaske von I063/060
bleiben ungenutzt. Das ist standardkonform — die FSPEC markiert, welche Items
präsent sind.
