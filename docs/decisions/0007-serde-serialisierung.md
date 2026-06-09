# ADR 0007 — `serde` für die Zustands-Serialisierung

- **Status:** akzeptiert
- **Datum:** 2026-06-09

## Kontext

Für die Cloud-Härtung (ADR 0003, NFR-CLOUD-003) muss der Tracker-Zustand
**serialisierbar** sein: Snapshot speichern, später wiederherstellen (Replay).
Wir brauchen dafür einen Standard-Mechanismus, der das *Format* nicht erzwingt
(der Kern soll format-neutral bleiben — passt zur Ports-&-Adapters-Linie aus
ADR 0006).

## Entscheidung

- Wir nutzen **`serde`** und leiten `Serialize`/`Deserialize` auf den
  Zustandstypen ab (`Tracker`, `Track`, `LinearKalman`, `Gate`, `ProcessNoise`,
  `SensorErrorModel`, die ID-Typen). Für die nalgebra-Matrizen aktivieren wir
  das Feature `serde-serialize`.
- **Format-neutral:** `serde` legt das Format nicht fest. Im Kern gibt es keine
  feste Format-Wahl. Für **Tests** nutzen wir `serde_json` als reine
  **Dev-Abhängigkeit**.

## Ehrliche Erkenntnis zum Format

JSON ist ein **Text**-Format; der `f64`-Round-Trip ist nicht garantiert
bit-genau bis auf das letzte Bit (wir haben eine Abweichung von 1 ULP
beobachtet). Konsequenzen, sauber getrennt:

- **Determinismus** (NFR-CLOUD-001/002) testen wir **ohne** Serialisierung
  (zwei identische Läufe → bit-genau gleich).
- **Wiederherstellbarkeit** (NFR-CLOUD-003) testen wir mit enger Toleranz
  (Restore stellt den Zustand auf volle Zahlengenauigkeit her und läuft
  äquivalent weiter).
- Für **byte-genaue** Produktions-Snapshots wäre ein **binäres** Codec
  (z. B. bincode/CBOR) die richtige Wahl — eine reine Rand-/Adapter-Entscheidung,
  später bei Bedarf.

## Konsequenzen

- `serde` wird Abhängigkeit von `firefly-core` (ID-Typen) und `firefly-track`
  (Zustandstypen); `serde_json` nur als Dev-Dependency.
- Der Kern bleibt formatfrei; das konkrete Snapshot-Format entscheidet der Rand
  (M3), wenn Persistenz/Bus dazukommen.
