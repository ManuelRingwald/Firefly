# FEP.2 — Mode-S-DAPs: I048/250 (BDS 4,0/5,0/6,0) → I062/380-Ausbau

> **Anforderung:** FR-TRK-040 · **ICD:** 3.4.0 (additiv) ·
> **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich: Warum?

Ein Mode-S-EHS-Radar fragt beim Roll-Call die BDS-Register des Transponders
ab und liefert sie in **I048/250** mit — die **Downlink Aircraft Parameters**:
was das Flugzeug selbst über Zustand und **Absicht** meldet. Firefly übersprang
das Item bisher komplett. Der operative Kern:

- **BDS 4,0 — Selected Altitude:** die im Autopiloten **eingedrehte**
  Zielhöhe. Der Vergleich mit der Freigabe ist die Grundlage der
  **Level-Bust-Erkennung** — der Lotse sieht die falsche Absicht, *bevor* das
  Flugzeug die Höhe verlässt. Eines der wirksamsten Safety-Netze der letzten
  20 Jahre.
- **BDS 5,0 — Track and Turn:** Rollwinkel (verrät den Kurvenflug Sekunden
  vor der Positionshistorie), Kurs über Grund, Ground Speed, TAS.
- **BDS 6,0 — Heading and Speed:** magnetisches Heading, IAS, Mach,
  Baro-Vertikalrate.

ARTAS reicht genau diese DAPs im **I062/380 (Aircraft Derived Data)** weiter —
das tut Firefly jetzt auch (MHG/SAL/IAR/MAC).

## Technik

**BDS-Decoder** (`firefly-asterix::bds`, bit-genau nach ICAO Doc 9871):
reine Bit-Arithmetik über dem festen 7-Byte-MB-Feld (1-basierte
ICAO-Bit-Nummerierung, MSB zuerst), Zweierkomplement für vorzeichenbehaftete
Felder, Winkel nach [0, 360) gefaltet. **Die Korrektheits-Regel ist die
Status-Bit-Disziplin:** jedes DAP-Feld trägt ein eigenes Gültigkeits-Bit —
nur markierte Felder werden übernommen, ein gelöschtes Bit ergibt `None`,
nie 0. So injiziert eine teil-bestückte oder degradierte Avionik keine
falschen Nullen ins Lagebild. Unbekannte Register ⇒ leeres Ergebnis.

**Datenfluss:** CAT048 dekodiert je I048/250-Repetition (7 Byte MB + 1 Byte
BDS-Nummer) und **merged mehrere Register per Feld** → `ModeAC.daps`
(`firefly_core::Daps`, 9 Option-Felder, serde-default hält alte `.ffplots`
lesbar) → der `Track` merged per Feld über die Meldungen (BDS 4,0 und 6,0
kommen selten in derselben) und führt die Zeit der letzten DAP-Meldung →
`SystemTrack.daps` trägt die Werte **nur solange frisch** (30-s-Fenster wie
die Provenienz): Eine veraltete Selected Altitude als aktuell zu zeigen wäre
gefährlicher als keine — **Absenz statt Stale-Behauptung**, konsistent zur
REG.3-Philosophie.

**I062/380-Ausbau (additiv):** Das Item ist jetzt echt compound —
FX-verkettete Subfeld-Spec, Daten in aufsteigender Subfeld-Nummer. Gesendet:
**MHG** (#3, LSB 360/2¹⁶ °), **SAL** (#6, SAS=1/Source=MCP + 13-Bit-
Zweierkomplement, LSB 25 ft), **IAR** (#26, LSB 1 kt), **MAC** (#27,
LSB 0,008). **Kein Wire-Bruch:** Ein DAP-loser Track bleibt byte-identisch
zur alten Form (`0x80` + Adresse); erst IAR/MAC verlängern die Spec via FX
auf vier Oktette. Der Decoder liest subfeld-getrieben zurück und lehnt
unbekannte Subfelder ab statt sie zu erraten (begrenzte Spec-Kette).

## Ehrliche Grenzen (FEP.2)

- BDS-5,0-Roll/Track/GS werden dekodiert und am Track **geführt, aber noch
  nicht genutzt** — die Einspeisung in die IMM-Kurvenerkennung ist ein
  eigenes Folge-Häppchen; auf den Draht gehen sie noch nicht.
- Kein Konsistenz-Check zwischen DAPs und Tracker-Schätzung (z. B. gemeldete
  GS vs. geschätzte) — Kandidat für die Betriebs-Härtung.
- FMS Selected Altitude und QNH (BDS 4,0) werden noch nicht übernommen.

## Schnittstellen-Wirkung

**ICD 3.3.0 → 3.4.0, additiv** — kein Lockstep. Wayfinder-Nachzug (Decoder
subfeld-getrieben + SEL-Anzeige im Label): **Issue #238** (`from-firefly`).

## Tests

`firefly-asterix` (8 neu): drei bit-genaue BDS-Referenz-Vektoren (inkl.
negativer Winkel/Raten via Zweierkomplement), Status-Bit-Gating, unbekannte
Register, I048/250-Merge über drei Register, byte-genauer I062/380-Dump,
Encode→Decode-Round-Trip inkl. negativer SAL, Alt-Form byte-identisch.
`firefly-track` (1 neu): Merge über Meldungen + Freshness-Rückhaltung.
Fuzzing: bestehende `cat048_decode`/`cat062_decode`-Targets decken die neuen
Pfade (Smoke 9,7 M Läufe ohne Befund). Gates: `cargo test --workspace`
(47 Suiten), `clippy`, `fmt` grün.
