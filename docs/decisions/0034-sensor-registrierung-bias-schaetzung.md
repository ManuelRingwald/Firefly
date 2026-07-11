# ADR 0034: Sensor-Registrierung — systematische Bias-Schätzung vor der Fusion

**Status:** akzeptiert (2026-07-10) · **Bezug:** ADR 0010 (zentrale
Mess-Fusion), ARTAS-Gap-Roadmap AP-REG (`docs/design/artas-gap-roadmap.md`),
Roadmap-Paket #8 („Sensor-Registrierung/Bias-Korrektur", seit M4 zurückgestellt)

## Kontext

Fireflys zentrale Mess-Fusion (ADR 0010) speist die Plots aller Sensoren in
einen gemeinsamen Tracker — unter der stillen Annahme, dass jeder Sensor
fehlerfrei kalibriert ist. Echte Radare sind das nie: Jedes trägt
**systematische** Messfehler — einen konstanten **Range-Offset** (alle Ziele
z. B. 150 m zu weit), einen konstanten **Azimut-Offset** (Nordausrichtung der
Antenne um Zehntelgrad verdreht) und ggf. einen **Zeitstempel-Versatz**.
Zufälliges Rauschen absorbiert der Kalman-Filter; ein systematischer Offset
dagegen verschiebt **alles**, was ein Sensor sieht, kohärent: Zwei Radare sehen
dasselbe Flugzeug an zwei leicht verschiedenen Orten, die Fusion baut daraus
Doppelbilder („Ghosts") oder verschmierte Tracks. Mit dem Simulator ist das
unsichtbar (er hat keine Biases) — mit zwei echten Radaren ist es das Erste,
was schiefgeht. ARTAS behandelt Registrierung deshalb als Kernfunktion; ohne
sie bringt der weitere Radar-Ausbau (AP-FEP: CAT034/CAT021) keinen belastbaren
Mehrwert. Deshalb steht REG in der Roadmap **vor** FEP.

## Entscheidung

### Fehlermodell (REG.1)

Pro Radar ein konstanter Bias-Vektor `b = (Δr, Δθ)` mit
`gemessen = wahr + Bias` (`SensorBias { range_m, azimuth_rad }`, serialisierbar
nach ADR 0007). **Bewusst ausgeklammert in REG.1:** der Zeitstempel-Versatz —
er verschiebt Ziele *entlang der Flugrichtung* (`v·Δt`), koppelt an die
Geschwindigkeit und braucht ein eigenes Schätz-Design (Folge-Häppchen).
Ebenfalls später: Range-Gain (skalierender statt konstanter Range-Fehler),
Rausch-Gewichtung.

### Korrespondenz-Bildung: Identität statt Kinematik (Betreiber-Entscheid)

Paare „gleiches Ziel, zwei Sensoren" entstehen über die **global eindeutige
Mode-S-/ICAO-Adresse** (Option a; wie die ICAO-Vorsortierung FR-TRK-031) —
deterministisch und für den ersten Schätzer am robustesten. Kinematisches
Pairing über das JPDA-Ergebnis (auch für PSR-only-Ziele) ist ein möglicher
späterer Ausbau. Das Pairing (`correspondences_by_identity`) nimmt je
Radar-Plot den zeitlich nächsten Partner **innerhalb eines engen Fensters**
— eng, weil der Schätzer Zeit-Ausrichtung annimmt (ein Airliner legt ~250 m
pro Sekunde Versatz zurück; ein *systematischer* Zeitversatz würde als
scheinbarer Bias erscheinen — genau darum ist er als eigenes Schätzziel
vorgemerkt).

### Schätzverfahren: linearisierte Kleinste Quadrate über den Lift

`lift_S(r, θ)` sei die Bodenposition einer Polar-Messung im gemeinsamen Frame
(Sensor-Frame → WGS84 → Common-Frame — derselbe Weg, den der Tracker geht).
Erste Ordnung in den kleinen Biases:

```
wahre Position ≈ lift_S(Messung) − J_S · b_S,   J_S = ∂lift_S/∂(r, θ)
```

Jede Korrespondenz k (Seiten a, b) liefert zwei lineare Gleichungen

```
d_k := lift_a − lift_b = J_a·b_a − J_b·b_b
```

Eine **geodätische** Seite (ADS-B-Selbstreport — Fireflys vorhandene
ADS-B-Quellen sind hier ein Geschenk: praktisch bias-freie Referenzwahrheit)
trägt nur ihre Position bei, keine Unbekannten. Alle Korrespondenzen gestapelt
ergeben ein überbestimmtes `H·x = d`, gelöst per **SVD** (nalgebra):

- Das Singulärwert-Spektrum ist zugleich die **Beobachtbarkeits-Diagnose**
  (`observable`-Flag): z. B. zwei **ko-lokierte** Radare, die nur einander
  sehen, sind rangdefizient — ein Gleichtakt-Bias kürzt sich aus jedem
  Residuum. Zwei Radare an **verschiedenen** Standorten sind dagegen generisch
  voll beobachtbar (das Residuen-Feld eines Bias hängt von der
  Standort-Geometrie ab) — im Test nachgewiesen.
- Die **Jacobi-Matrix wird numerisch** bestimmt (zentrale Differenzen auf dem
  exakten Lift): exakt bis O(h²) inklusive der Frame-Rotations-Terme zwischen
  entfernten Standorten, und trivial gegen die analytische Flachgeometrie-Form
  testbar — keine handhergeleiteten Rotationsterme, die subtil falsch sein
  können.
- Diagnosen: Residuen-RMS vor/nach Korrektur.

### Gestufter Ausbau (Häppchen nach Charter §2)

| Stufe | Inhalt | Status |
|---|---|---|
| **REG.1** | Fehlermodell + Offline-Schätzer + Identitäts-Pairing (dieses ADR) | ✅ |
| **REG.2** | Online-Schätzung im Live-Pfad + Korrektur **vor** der Fusion, Metriken, Konvergenz-Überwachung | ⏳ |
| **REG.3** | Bias-Statistik auf den Draht: CAT063 I063/070–092, Referenz-Vektoren, ICD-Bump | ⏳ |

REG.1 greift **nicht** in den Live-Pfad ein und ändert **nichts** am
Draht-Vertrag (keine Wayfinder-Wirkung).

## Verworfene Alternativen

- **Track-zu-Track-Registrierung** (Biases aus fertigen Tracks je Sensor
  schätzen): setzt Sensor-lokale Tracks voraus, die ADR 0010 bewusst nicht
  führt; die Mess-Ebene ist direkter und rauschärmer.
- **Kinematisches Pairing zuerst**: verrauschter und assoziationsabhängig;
  Identitäts-Pairing deckt den relevanten Erstfall (Mode-S/ADS-B-Ziele) sauber
  ab. Kinematik bleibt als Erweiterung für PSR-only-Umgebungen.
- **Analytische Jacobi-Matrix**: spart nichts Messbares und riskiert
  Vorzeichen-/Rotationsfehler zwischen Frames; die numerische Form ist gegen
  die analytische Flachgeometrie-Form getestet.

## Konsequenzen

- Neues Modul `firefly-track::registration` (rein, deterministisch, ohne I/O —
  testbar wie der übrige Kern). Anforderung **FR-TRK-037** mit
  Ground-Truth-Tests (injizierte Biases werden zurückgewonnen).
- Die ehrlichen Grenzen (offline; kein Zeit-Offset; ungewichtet) sind im
  Modul-Doc und hier dokumentiert; REG.2 entscheidet Akkumulations-Fenster und
  Anwendungs-Politik (z. B. erst anwenden, wenn `observable` und RMS-Gewinn
  signifikant).
