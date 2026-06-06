# ADR 0001 — Technologie-Stack und Vorgehen

- **Status:** akzeptiert
- **Datum:** 2026-06-06

## Kontext

Wir bauen einen web-basierten Radar-Tracker. Vor dem ersten Code waren vier
Weichen zu stellen: Programmiersprache der Rechen-Engine, Datenquelle,
Reihenfolge der Meilensteine und Art der Darstellung.

## Entscheidung

1. **Engine in Rust, Frontend in JavaScript.**
   Begründung: Der Tracker rechnet viel mit Matrizen (Kalman-Filter) und hat
   eine zustandsbehaftete Logik (Track-Lebenszyklus). Rust ist dafür schnell,
   speichersicher und drückt Zustände über sein Typsystem sauber aus; für
   Echtzeit-Streaming an den Browser gibt es einen ausgereiften Stack
   (`tokio`/`axum`). Das Ergebnis ist ein einzelnes, schnelles Programm.
   Alternative Go wurde verworfen (weniger ausgereifte Numerik-Bibliotheken).

2. **Eingabe-/Austauschformat: ASTERIX.**
   Begründung: Das ist der reale Standard echter Radarsysteme und ANSP-Umgebungen
   (CAT048, CAT021, CAT062). Nah an der Praxis, hoher Lernwert.

3. **Erster Umfang: Simulator (M1) + Single-Radar-Tracker (M2).**
   Begründung: Erst ein solides, überschaubares Fundament mit schnell sichtbarem
   Ergebnis; Multi-Radar-Fusion ist anspruchsvoller und kommt später (M4).

4. **Darstellung: 2D-Karte.**
   Begründung: klassische Radar-Scope-Anmutung, einfacher Einstieg; 3D ist später
   denkbar.

## Konsequenzen

- Wir brauchen eine Rust-Toolchain (vorhanden) und später Node.js fürs Frontend
  (vorhanden).
- ASTERIX müssen wir selbst kodieren/dekodieren (eigene Crate `firefly-asterix`,
  geplant für M1.5).
- Multi-Radar-Fähigkeit wird im Design früh mitgedacht (Sensor-IDs, eigene
  Frames je Radar), aber erst in M4 voll genutzt.
