# ADR 0002 — Sprache von Code und Dokumentation

- **Status:** akzeptiert
- **Datum:** 2026-06-06

## Kontext

Der Projektverantwortliche arbeitet auf Deutsch und nutzt das Projekt zum Lernen.
Software-Quellcode wird üblicherweise auf Englisch geschrieben. Wir müssen
festlegen, was in welcher Sprache entsteht, damit es konsistent bleibt.

## Entscheidung

- **Deutsch:** Chat-Erklärungen, die gesamte `docs/`-Dokumentation, das Glossar,
  diese ADRs und die `CLAUDE.md`.
- **Englisch:** Quellcode — also Namen von Funktionen/Typen/Variablen und die
  Kommentare *im* Code.

## Begründung

- Englischer Code ist internationaler Standard, bleibt portabel und
  anschlussfähig (Bibliotheken, Fehlermeldungen, spätere Mitwirkende sind
  englisch).
- Das *Verständnis* entsteht ohnehin nicht durch Code-Kommentare allein, sondern
  durch die deutschen Erklärungen in `docs/` und im Chat — dort ist die
  natürliche Sprache am wichtigsten.

## Konsequenzen

- Beim Erklären eines Code-Stücks übersetzt/erläutert Claude die englischen
  Bezeichner auf Deutsch.
- Diese Entscheidung ist jederzeit umkehrbar; falls der Projektverantwortliche
  deutschsprachigen Code bevorzugt, schreiben wir einen neuen ADR.
