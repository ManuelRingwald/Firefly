# REG.1 — Sensor-Registrierung: Fehlermodell + Offline-Bias-Schätzer

> **Anforderung:** FR-TRK-037 · **ADR:** 0034 ·
> **Einstufung:** S5 · umgesetzt auf Fable 5

## Fachlich: Warum?

Der Schwellenpunkt von Demo-Qualität zu betriebstauglich. Echte Radare tragen
**systematische** Messfehler (Range-/Azimut-Offset); unkorrigiert sieht die
zentrale Mess-Fusion (ADR 0010) dasselbe Flugzeug an zwei leicht verschobenen
Orten und baut Doppelbilder. ARTAS schätzt diese Biases laufend aus den Daten
und rechnet sie vor der Fusion heraus („Registrierung"). REG.1 legt Fireflys
Fundament dafür: das Fehlermodell und einen **offline** arbeitenden Schätzer —
bewusst ohne Live-Eingriff (REG.2) und ohne Draht-Wirkung (REG.3).

## Mathematik (Kurzfassung; Details im Modul-Doc + ADR 0034)

`gemessen = wahr + Bias` je Radar, `b = (Δr, Δθ)`. Für den Lift
`lift_S(r, θ)` (Sensor-Frame → WGS84 → gemeinsamer Frame, derselbe Weg wie im
Tracker) gilt in erster Ordnung `wahr ≈ lift(Messung) − J·b` mit der
2×2-Jacobi-Matrix `J = ∂lift/∂(r, θ)`. Zwei Sichtungen desselben Flugzeugs
(Paarung über die eindeutige ICAO-Adresse) müssen auf dieselbe wahre Position
zeigen ⇒ pro Korrespondenz `d = J_a·b_a − J_b·b_b`. Alle Korrespondenzen
gestapelt: überbestimmtes `H·x = d` → **SVD-Kleinste-Quadrate**.

Entwurfs-Feinheiten:

- **Numerische Jacobi** (zentrale Differenzen auf dem exakten Lift): korrekt
  inkl. Frame-Rotation zwischen entfernten Standorten; gegen die analytische
  Flachgeometrie-Form getestet.
- **Geodätische Referenz:** ADS-B-Selbstreports sind (praktisch) bias-frei —
  eine Korrespondenz Radar↔ADS-B verankert die Schätzung absolut. Fireflys
  vorhandene ADS-B-Quellen machen das zum Normalfall.
- **Beobachtbarkeit:** Das Singulärwert-Spektrum diagnostiziert Rangdefizite.
  Getestetes Beispiel: zwei **ko-lokierte** Radare, die nur einander sehen —
  ein Gleichtakt-Bias kürzt sich aus jedem Residuum → `observable=false`,
  Minimum-Norm-Lösung, nicht operationell anwenden. Zwei Radare an
  **verschiedenen** Standorten sind dagegen voll beobachtbar (im Test: beide
  Bias-Paare absolut zurückgewonnen, ganz ohne Referenz).
- **Diagnose:** Residuen-RMS vor/nach Korrektur — der „Doppelbild-Abstand"
  und was nach Abzug der Schätzung übrig bleibt (idealerweise nur Rauschen).

## Ehrliche Grenzen (REG.1)

- **Offline** — Korrespondenzen sammeln und schätzen; die Rückkopplung in die
  Live-Fusion (Akkumulations-Fenster, Anwendungs-Politik, Metriken) ist REG.2.
- **Kein Zeit-Offset** — ein systematischer Zeitstempel-Versatz verschiebt
  Ziele entlang der Flugrichtung (`v·Δt`) und würde als Schein-Bias erscheinen;
  darum enges Pairing-Fenster und eigenes Folge-Häppchen.
- **Ungewichtet** — alle Korrespondenzen zählen gleich.

## Tests (Ground-Truth-Nachweis)

9 Tests in `firefly-track::registration`, u. a.: injizierte Biases
(150 m / 0,3°) werden **unter Messrauschen** innerhalb enger Toleranz
zurückgewonnen; Zwei-Radar-Fall ohne Referenz; Null-Bias → Null-Schätzung;
Ko-Lokations-Degenerierung geflaggt; Pairing (Identität, Zeitfenster,
keine Doppelzählung). Gates: `cargo test --workspace` (47 Suiten),
`clippy`, `fmt` grün. Keine neuen Env-Variablen; kein Wire-/Wayfinder-Bezug.
