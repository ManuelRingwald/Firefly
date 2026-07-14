# ADR 0035: Constant-Acceleration-Modell — einheitlicher 6-D-Zustand in der IMM-Bank, 4-D-Projektion am Rand

**Status:** akzeptiert (2026-07-14, Betreiber-Freigabe „Weg A, 2 Häppchen") ·
**Bezug:** ARTAS-Gap-Roadmap AP-VERT (VERT.4, `docs/design/artas-gap-roadmap.md`),
VERT.3-Scope-Split (`docs/milestones/VERT3-Mode-of-Movement.md`), FR-TRK-043/044

## Kontext

Fireflys IMM-Bank fährt CV + zwei CT-Modelle auf einem **4-D-Zustand**
`[E, N, vE, vN]`. Längsbeschleunigung (Startlauf, Steigbeschleunigung,
Abbremsen im Anflug) behandeln alle drei Modelle als **Prozessrauschen** —
die Positionsschätzung läuft dem Ziel in diesen Phasen hinterher
(Prädiktions-Lag, unnötig weites Gate). VERT.3 lieferte die Beschleunigung
für I062/210 deshalb nur als **abgeleitete** Größe (EWMA über d/dt der
Kombinationsgeschwindigkeit) — Anzeige-Hälfte. Die Tracking-Hälfte, ein
**CA-Modell in der Bank**, braucht einen Zustand, der die Beschleunigung
trägt: 6-D `[E, N, vE, vN, aE, aN]`.

Die naheliegende Befürchtung (so in VERT.3 formuliert): ein 6-D-Zustand
„schneidet durch den gesamten Fusionskern". Die Code-Inspektion vor dieser
Entscheidung ergab ein besseres Bild: **die Bank reicht nach außen
ausschließlich ihre kombinierte Schätzung als 4-D-`LinearKalman`** —
Gating, PDA/JPDA, Assoziation und Registrierung konsumieren nur diese.
Der 6-D-Zustand kann also **innerhalb der Bank gekapselt** bleiben.

## Entscheidung

### Weg A: einheitlich 6-D innerhalb der Bank, 4-D-Projektion am Rand

Alle Modelle der Bank werden 6-D; die Mischung (IMM-Interaktion) bleibt
trivial, weil alle Zustände dieselbe Dimension haben. Am **Rand** der Bank:

- **Einbettung 4-D → 6-D** (Seeding): Position/Geschwindigkeit samt Kovarianz
  übernehmen, Beschleunigung = 0 mit diagonalem Prior `accel_std²`, **keine**
  behauptete Kreuz-Kovarianz.
- **Projektion 6-D → 4-D** (Ausgabe): die exakte Gauß-Marginale über
  (Position, Geschwindigkeit) — erste vier Zustandseinträge, linker oberer
  4×4-Kovarianzblock. Der nachgelagerte Kern bleibt **unverändert** auf
  seinem 4-D-Vertrag.

**Verworfen — Weg B (gemischt-dimensionale Bank):** CV/CT bleiben 4-D, nur
CA wird 6-D. Kleinster Zustands-Fußabdruck, aber die Mischungsmathematik über
verschiedene Dimensionen (Augmentation/Projektion in jedem IMM-Zyklus) ist
der subtilste und fehleranfälligste Teil — mehr Risiko genau dort, wo
Korrektheit zählt. Bei gerade einmal zwei zusätzlichen f64 je Modellfilter
ist die Speicherersparnis irrelevant.

### Was jede Hypothese über die Beschleunigung aussagt

Der fachliche Kern der 6-D-Transitionen — jede Hypothese macht eine
**ehrliche, eigene** Aussage über den Beschleunigungszustand:

| Modell | Beschleunigungs-Dynamik | Begründung |
|--------|------------------------|------------|
| **CA** | frei, koppelt in v und p (`p' = p + v·dt + a·dt²/2`), White-Noise-**Jerk** als Budget | das eigentliche Beschleunigungs-Modell |
| **CV** | `a' = 0` (Null-Zeilen in F) | „es gibt keine Beschleunigung" — eine eingemischte Beschleunigung wird unter dieser Hypothese genullt statt als totes Gewicht mitgeführt; F darf singulär sein |
| **CT** | `a' = ω·J·v'` (Zentripetalwert, **linear im Zustand**) | das Manöver lebt in der Geschwindigkeits-Rotation; der Beschleunigungszustand meldet die wahre Querbeschleunigung der Kurve statt einer falschen Null |

Die CT-Formel ist der entscheidende Kniff: `a = ω × v` ist in 2-D linear in
`v` (`a'E = -ω·v'N`, `a'N = ω·v'E`) und passt damit in die lineare
Transitionsmatrix — die kombinierte Beschleunigung der Bank ist so in
**allen** Flugphasen ehrlich: Zentripetalwert in der Kurve (aus CT),
Längsbeschleunigung beim Beschleunigen (aus CA), null im Reiseflug (aus CV).
Ohne den CT-Kniff würde I062/210 in stationären Kurven fälschlich gegen 0
gehen — schlechter als der abgeleitete VERT.3-Schätzer, der die
Gesamtbeschleunigung sieht.

### Prozessrauschen

CA erhält das **White-Noise-Jerk-Modell** (CWNA eine Ableitung höher):
`Q`-Triple pro Achse `q·[dt⁵/20, dt⁴/8, dt³/6; dt⁴/8, dt³/3, dt²/2;
dt³/6, dt²/2, dt]`, parametriert über die Jerk-PSD (m²/s⁵). Die Q-Wahl für
CV/CT im 6-D-Raum (kleine Beschleunigungs-Diagonale zur Konditionierung)
ist Tuning und fällt in VERT.4b.

### Häppchen-Schnitt (Betreiber-Freigabe)

- **VERT.4a** — das Fundament, bewusst **nicht verdrahtet**: 6-D-Filter
  (`LinearKalman6`, Numerik-Spiegel des 4-D-Filters: Joseph-Form,
  `2π·√|S|`-Likelihood), die drei 6-D-Transitionen, `JerkNoise`, beide
  Rand-Abbildungen; reine Unit-Tests (Kinematik exakt, Halbgruppe,
  4-D-Einbettungs-Äquivalenz, Zentripetal-Eigenschaften, Q-PSD-Linearität,
  Konvergenz auf beschleunigendem Ziel).
- **VERT.4b** — Integration: Bank auf `LinearKalman6`, Kombination +
  Projektion, CA-Modell in `ImmConfig` (Transition/Prior-Tuning),
  I062/210 aus dem Filterzustand statt aus dem VERT.3-Ableiter,
  End-to-End-Tests. Erst 4b ändert Verhalten.

## Konsequenzen

- **Kein Wire-/ICD-Bezug:** I062/200/210 existieren seit ICD 3.6.0; nur die
  Herkunft der Beschleunigung wird besser. Kein Wayfinder-Lockstep.
- **Snapshot-Format:** Das IMM-Snapshot-Layout ändert sich mit 4b (6-D-
  Zustände). Es gibt noch kein produktives Restore-Format (HA.1 offen) —
  der Bruch ist jetzt billig, später teuer; genau deshalb VERT.4 vor HA.1.
- **VERT.3-Ableiter — Entscheidung aus 4b:** sein Glättungswert ist
  vollständig abgelöst (I062/210 **und** LONG-Trend projizieren den
  Filterzustand); er bleibt als deterministischer Frische-Zeuge der
  Geschwindigkeits-Sample-Kette und als Erst-Schätzungs-Gate erhalten.
- **Ehrliche Grenze:** Weg A rechnet CV/CT auf 6-D mit — zwei zusätzliche
  Zustandsdimensionen je Filter sind bei drei bis vier Modellen pro Track
  vernachlässigbar; sollte die Bank je auf viele Modelle wachsen, wäre
  Weg B neu zu bewerten.
