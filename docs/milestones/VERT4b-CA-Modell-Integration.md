# VERT.4b — CA-Modell in der IMM-Bank (Integration)

> **Anforderung:** FR-TRK-044 (verifiziert) · **ADR:** 0035 (Weg A) ·
> **ICD:** unberührt · **Einstufung:** S5 (Häppchen 2 von 2) · Fable 5

## Was jetzt anders ist

Die IMM-Bank läuft vollständig auf dem 6-D-Zustand (`LinearKalman6`):
Mischung, Kombination und PDA-Update dimensionsrein in 6-D; nach außen
liefert `combined_estimate()` unverändert die **exakte 4-D-Marginale** —
Gating/JPDA/Registrierung unberührt (Weg-A-Versprechen eingehalten).
Die **Default-Bank ist `cv_turns_and_ca`** (CV + 2×CT + CA). **I062/210
kommt aus dem Filterzustand** (`Imm::combined_acceleration`): zentripetal
in der Kurve (CT-Zeilen), längs beim Beschleunigen (CA), null im Reiseflug
(CV). Der VERT.3-Ableiter bleibt als deterministischer **Frische-Zeuge**
und Erst-Schätzungs-Gate; sein Glättungswert ist abgelöst (auch der
LONG-Trend projiziert jetzt den Filterzustand — der Zentripetal-Anteil
steht ⊥ v und kontaminiert ihn nicht).

## Tuning (ehrlich dokumentiert)

Ein viertes Modell besteuert die Geradeaus-Genauigkeit (klassischer
IMM-Trade): der Szenario-Test riss zunächst (RMSE 40,3 m > 40,0 m).
Antwort war **Tuning statt Schwellen-Aufweichung**: CV klebriger (0,94),
CA-Einstieg sparsam (0,02–0,03), Prior 0,88/0,045/0,045/0,03 —
Schwelle wieder gehalten. Jerk-PSD-Default 0,1 m²/s⁵ (`q ≈ Δa²/Δt`).

## Nachweise

- **Startlauf** (2,5 m/s², σ = 30 m, 4-s-Scans): µ_CA > 0,7, CA-Zustand
  trifft die Wahrheit (±0,3), MMSE-Blend trägt > 75 %.
- **Stationäre Kurve:** |a| ≈ ω·v (Zentripetalwert, ±15 %) — ohne die
  CT-Beschleunigungs-Zeilen wäre das fälschlich ~0.

## Ehrliche Grenzen

- **MMSE-Schrumpfung:** bei mehrdeutiger Evidenz (sanfte 1 m/s² ⇒ nur
  ~0,27σ CV-Lag/Scan) bleibt das Posterior gemischt und der Blend unter
  der Wahrheit — kein Bug, sondern die korrekte Erwartung unter
  Modell-Unsicherheit; der VERT.3-Ableiter zeigte dafür volle Magnitude
  bei mehr Rauschen/Lag.
- **Snapshot-Layout gebrochen** (6-D-Filter): vor HA.1 ohne produktives
  Restore-Format — bewusst jetzt (ADR 0035).
- Jerk-PSD/Prior sind Erst-Kalibrierung; Feinschliff gegen reale
  Aufzeichnungen fällt in HA.4 (Auswertungs-Harness).
