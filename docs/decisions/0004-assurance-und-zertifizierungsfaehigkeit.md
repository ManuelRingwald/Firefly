# ADR 0004 — Assurance & Zertifizierungs-Fähigkeit

- **Status:** akzeptiert
- **Datum:** 2026-06-06

## Kontext

Firefly soll perspektivisch bei einem ANSP in Produktion gehen können und die
nötigen Zertifizierungen/Audits bestehen. Der relevante Normenrahmen für
Boden-ATM-/Surveillance-Software:

- **EU 2017/373** — gemeinsame Anforderungen an die Erbringung von Flugsicherung.
- **ED-153** — EUROCONTROL/EUROCAE Software Safety Assurance; definiert
  **SWAL** (Software Assurance Level).
- **ED-109A / DO-278A** — Software-Integritäts-Assurance für CNS/ATM-Bodensysteme;
  definiert **Assurance Levels (AL1–AL6)** und die nötigen Prozess-Nachweise.
- Ergänzend: Surveillance-Performance-Spezifikationen, **Part-IS / ED-205**
  (Information Security), ISO 9001/27001.

## Entscheidung

Wir bauen **zertifizierungs-fähig** (nicht „leichtgewichtig", nicht „volle
formale Artefakte sofort") und orientieren uns an **ED-153 + ED-109A/DO-278A**.

Das heißt: Wir übernehmen ab Tag eins die *Ingenieurs-Disziplinen*, die ein
Audit verlangt, ohne schon die volle formale Nachweis-Bürokratie zu erzeugen:

1. **Rückverfolgbarkeit (Traceability).**
   Ein Anforderungs-Register (`docs/requirements/`) mit stabilen IDs. Code und
   Tests verweisen auf diese IDs, sodass „Anforderung → Design → Code → Test" in
   beide Richtungen nachvollziehbar ist. Das ist das *Herz* von DO-278A.
2. **Verifikation mit Nachweis.**
   Tests (Unit/Integration) mit gemessener Code-Abdeckung; Reviews; statische
   Analyse (Clippy, `deny(unsafe)` wo möglich).
3. **Konfigurationsmanagement.**
   Git, getaggte Baselines pro Meilenstein, ADRs für jede wichtige Entscheidung,
   nachvollziehbare Commit-Historie.
4. **Analysierbarkeit & Determinismus.**
   Rusts Speicher-/Thread-Sicherheit ist ein starkes Assurance-Argument
   (kein undefiniertes Verhalten, keine Daten-Races). Determinismus (ADR 0003)
   macht Verhalten vorhersag- und prüfbar.
5. **Dokumentierter Lebenszyklus.**
   Die `CLAUDE.md`-Charta + Meilenstein-Doku bilden die Grundlage eines späteren
   formalen Plans (PSAC-ähnlich).

## Ehrliche Abgrenzung (wichtig)

Ein Code-/Lernprojekt kann **nicht selbst „einen ANSP-Audit bestehen".**
Zertifizierung ist organisatorisch und regulatorisch: Safety Case, Safety
Management System, akkreditierter Entwicklungsprozess, *unabhängige*
Verifikation, Einbindung der Aufsichtsbehörde, und das System als Teil eines
zugelassenen Betriebs. Das leistet diese Codebasis nicht und verspricht es nicht.

Was sie leistet: so gebaut und dokumentiert zu sein, dass sie in ein solches
Programm **hineingehen** kann, statt dafür neu geschrieben werden zu müssen —
das Gegenteil des „Legacy-im-Container"-Problems.

## Konsequenzen

- Neue Qualitäts-Gates (in `CLAUDE.md` §5 ergänzt): kein unbegründetes `unsafe`,
  Anforderungen rückverfolgbar eintragen.
- Wir führen ein Anforderungs-Register und eine einfache Traceability-Konvention
  ein (`docs/requirements/`).
- Sicherheits-Aspekte (FHA/Hazard-Betrachtung) werden später ergänzt, sobald die
  Funktionen stehen, gegen die man Gefährdungen bewerten kann.
