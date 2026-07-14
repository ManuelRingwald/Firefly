# VERT.4a — 6-D-Fundament für das Constant-Acceleration-Modell

> **Anforderung:** FR-TRK-044 · **ADR:** 0035 (Weg A) · **ICD:** unberührt ·
> **Einstufung:** S5 (Häppchen 1 von 2) · umgesetzt auf Fable 5

## Fachlich: Warum?

Die IMM-Bank (CV + 2×CT auf 4-D-Zustand) behandelt **Längsbeschleunigung als
Prozessrauschen** — beim Startlauf, in der Steigbeschleunigung und beim
Abbremsen im Anflug läuft die Positionsschätzung dem Ziel hinterher
(Prädiktions-Lag, unnötig weites Gate). VERT.3 hat die Beschleunigung für
I062/210 deshalb nur **abgeleitet** (EWMA über d/dt der
Kombinationsgeschwindigkeit) — die Anzeige-Hälfte. VERT.4 liefert die
**Tracking-Hälfte**: ein CA-Modell in der Bank, das diese Phasen sauber
mitprädiziert und die Beschleunigung als **Filterzustand** führt (rausch-
und lag-ärmer als jede Ableitung).

VERT.4a ist davon das **Fundament** — bewusst noch **nicht verdrahtet**
(Integration = VERT.4b): erst das Rechenwerk isoliert beweisen, dann den
empfindlichsten Teil des Trackers anfassen.

## Technik

**Weg A (ADR 0035):** Die Code-Inspektion ergab, dass die Bank nach außen
ausschließlich ihre kombinierte 4-D-Schätzung reicht — Gating, JPDA,
Assoziation und Registrierung konsumieren nur diese. Der 6-D-Zustand
`[E, N, vE, vN, aE, aN]` bleibt daher **in der Bank gekapselt**; am Rand
stehen zwei exakte Abbildungen: Einbettung 4-D → 6-D (a = 0, diagonaler
Prior, keine behauptete Kreuz-Kovarianz) und Projektion 6-D → 4-D (exakte
Gauß-Marginale). Der Fusionskern bleibt unverändert. (Weg B — gemischt-
dimensionale Bank — verworfen: die Mischung über verschiedene Dimensionen
ist der fehleranfälligste Teil, für zwei gesparte f64 je Filter.)

**`LinearKalman6`** (`firefly-track::kalman6`): der Numerik-Zwilling des
4-D-Filters — Position-only-`H`, geteilte Innovation für Update und
Likelihood, `2π·√|S|`-Normierung, **Joseph-Form** fürs Kovarianz-Update
(symmetrisch/PSD unter endlicher Präzision, ADR 0004). `predict(F, Q)`
nimmt Transition und Rauschen als Matrizen — die Modell-Verdrahtung ist
Sache der Bank (4b).

**Die drei Hypothesen im 6-D-Raum** — jede macht eine **ehrliche, eigene**
Aussage über die Beschleunigung:

- **CA:** volle Kopplung `p' = p + v·dt + a·dt²/2`, `v' = v + a·dt`;
  Budget = White-Noise-**Jerk** (`JerkNoise`, CWNA eine Ableitung höher,
  `Q`-Triple pro Achse `q·[dt⁵/20, dt⁴/8, dt³/6; dt⁴/8, dt³/3, dt²/2;
  dt³/6, dt²/2, dt]`).
- **CV:** Beschleunigungs-**Null-Zeilen** — die Hypothese „keine
  Beschleunigung" nullt eine eingemischte Beschleunigung, statt sie als
  totes Gewicht mitzuführen (F bewusst singulär; eine Kalman-Transition
  muss nicht invertierbar sein).
- **CT:** Beschleunigungs-Zeilen = **Zentripetalwert** `a' = ω·J·v'` —
  in 2-D linear im Zustand (`a'E = −ω·v'N`, `a'N = ω·v'E`) und damit Teil
  der linearen Transitionsmatrix. Der entscheidende Kniff: eine stationäre
  Kurve meldet so ihre wahre Querbeschleunigung; ohne ihn ginge die
  kombinierte Beschleunigung der Bank in Kurven fälschlich gegen 0 —
  **schlechter** als der VERT.3-Ableiter, der die Gesamtbeschleunigung
  sieht.

Die (p, v)-Blöcke von CV6/CT6 sind exakt die 4-D-Transitionen
(testverifiziert), inklusive Kleinst-Raten-Guard.

**Kernnachweis** (`filter_estimates_acceleration_as_state`): aus reinen
Positionsmessungen eines gleichmäßig beschleunigenden Ziels (0,5 m/s²)
schätzt der 6-D-Filter Geschwindigkeit **und** Beschleunigung als Zustand —
konvergent auf < 0,05 m/s², ohne Phantom-Querbeschleunigung, ohne
Differenziation.

## Schnittstellen-Wirkung

**Keine.** Kein ICD-Bezug (I062/200/210 existieren seit 3.6.0, nur die
künftige Herkunft der Beschleunigung ändert sich mit 4b), kein
Quell-Kontrakt-Bezug, keine Env-Variablen, keine Metriken, keine
Verhaltens-Änderung — rein additives, ungenutztes Fundament mit Tests.

## Ehrliche Grenzen (VERT.4a)

- **Noch keine Wirkung:** Bank, Tracker und I062/210 nutzen das Fundament
  noch nicht — jede Verhaltens-Verbesserung kommt erst mit VERT.4b.
- **Q für CV/CT im 6-D-Raum offen:** die kleine Beschleunigungs-Diagonale
  zur Konditionierung ist Tuning und fällt bewusst in 4b, wo die
  End-to-End-Testbilder existieren.
- **Jerk-PSD unkalibriert:** der Testwert (0,01 m²/s⁵) ist kein
  Betriebs-Tuning; die Bank-Kalibrierung (Transition/Prior für CA) ist 4b.
- **Snapshot-Wirkung erst mit 4b:** das IMM-Snapshot-Layout ändert sich
  bei der Integration (6-D-Zustände); es gibt noch kein produktives
  Restore-Format (HA.1 offen), der Bruch ist jetzt billig.
