# ADR 0005 — `nalgebra` als Bibliothek für lineare Algebra

- **Status:** akzeptiert
- **Datum:** 2026-06-06

## Kontext

Ab M2 (Tracker) rechnen wir laufend mit Vektoren und Matrizen: Mess-Kovarianzen,
Kalman-Filter, Gating. M1 war bewusst **ohne** externe Abhängigkeiten gebaut
(siehe ADR 0001/Reproduzierbarkeit). Für lineare Algebra selbst eine Bibliothek
zu schreiben wäre fehleranfällig und am Lernziel vorbei — die Mathematik des
Trackers ist der Inhalt, nicht das Matrix-Handwerk.

## Entscheidung

Wir nutzen **`nalgebra`** (Version 0.33) als lineare-Algebra-Bibliothek, ab der
Crate `firefly-track`. Es ist damit die **erste externe Abhängigkeit** des
Projekts.

## Begründung

- **Reines Rust**, keine C/Fortran-Bindungen → einfacher, portabler, besser
  analysierbar (passt zu Cloud-/Assurance-Zielen, ADR 0003/0004).
- Sehr **verbreitet und ausgereift**, gut getestet, klare API für die festen
  kleinen Dimensionen, die wir brauchen (`Vector2`, `Matrix2`, …).
- Compile-time-feste Dimensionen helfen, Dimensionsfehler früh zu fangen.

## Konsequenzen

- Der Offline-Bau benötigt einmalig einen Netzzugang, um `nalgebra` und seine
  wenigen Abhängigkeiten zu holen; danach ist der Build reproduzierbar
  (versioniert über `Cargo.lock`).
- `firefly-geo`/`firefly-core`/`firefly-sim` bleiben unberührt und
  abhängigkeitsfrei; nur der Tracker zieht `nalgebra`.
- Falls künftig eine Assurance-Bewertung die Abhängigkeit prüft, ist sie hier
  begründet und über `Cargo.lock` eindeutig versioniert.
