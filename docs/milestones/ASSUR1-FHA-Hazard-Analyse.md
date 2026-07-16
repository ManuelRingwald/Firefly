# ASSUR.1 — Functional Hazard Analysis (FHA)

> **Anforderung:** NFR-SAFE-004 · **ADR:** — (Analyse-Artefakt, ADR-0004-Linie) ·
> **ICD:** unberührt · **Einstufung:** S4 · umgesetzt auf Fable 5
> (Roadmap-Empfehlung: Opus 4.8)

## Fachlich

Wir bauen „zertifizierungs-fähig" (ADR 0004) — und jedes echte
Zulassungsverfahren beginnt mit derselben Frage: **Was kann dieses System
einem Lotsen antun, wenn es falsch arbeitet, und was fängt das ab?**
ASSUR.1 beantwortet sie systematisch: `docs/safety/FHA.md` listet die
sieben Systemfunktionen, deklinieren je Funktion die Versagensarten durch
(nach dem klassischen Schema **Verlust vs. irreführend × erkannt vs.
unerkannt** — die gefährlichste Klasse ist immer „falsch, sieht aber
richtig aus"), stuft die Schwere indikativ ein und stellt jeder
Gefährdung die **bereits gebauten Barrieren** gegenüber — rückverfolgbar
zu ADR, Anforderung und Test statt als Prosa-Behauptung.

Der zweite Nutzen ist nach innen gerichtet: Viele Bausteine der letzten
Monate (TSE, Heartbeat, Restore-Tore, Split-Brain-Schutz, Weeze-Regeln,
Cluster-Kappe, „Absenz statt eingefrorener Höhe") waren immer schon
Sicherheits-Barrieren — jetzt sind sie erstmals **als solche
katalogisiert**, und die verbliebenen Lücken stehen offen im
Lücken-Register statt in niemandes Kopf.

## Technik

- **`docs/safety/FHA.md`** — §1 Systemabgrenzung + Typ-/Schwere-Schema,
  §2 Funktionen F1–F7, §3 Gefährdungstabellen (24 Zeilen H-Fx-yy mit
  Trace-Spalte), §4 Querschnitts-Barrieren (Determinismus/Replay,
  Instrument-Tests, Regression-Gates), §5 Lesart, §6 Lücken-Register,
  §7 Pflege-Regel (Fortschreibung je architektur-relevanter Änderung,
  Audit-Spur für geschlossene Lücken).
- **Register:** NFR-SAFE-004 (Status „umgesetzt (Analyse);
  Betreiber-Review + Kontext-Einstufung ausstehend").
- **Roadmap:** ASSUR.1 ✅ 99 %; **neue Zeile SAFE.4** (Lücke L1).

## Zentrale Befunde

1. **L1 — die eine echte Code-Lücke:** Der CAT065-Heartbeat läuft
   wanduhr-getrieben und **unabhängig vom Tracker-Task**. Hängt der
   Tracker, sendet Firefly weiter „lebendig" — ein eingefrorenes Bild
   sähe für Konsumenten gesund aus (H-F1-02, Klasse I/u). Abgeleitete
   Maßnahme: **SAFE.4 Tracker-Fortschritts-Watchdog** (CAT065 →
   NOGO/degradiert bei ausbleibendem Output-Tick), als Roadmap-Zeile
   eingetragen.
2. **Verfahrens-Lücken, keine Code-Lücken:** konsistent falscher
   Site-Eintrag (L2 → COMPASS-Gegen-Check je Konfig-Änderung, Verfahren
   existiert) und plausibel-falscher QNH-Wert (L3 → Zweitquellen-Check,
   geringe Priorität).
3. **Alarmierung** (L4) war bereits als MON.1 eingeplant — die FHA
   liefert nun die Begründung, *welche* Zähler alarmwürdig sind.
4. **Grundmuster bestätigt:** Firefly entscheidet in jeder kritischen
   Zeile „ehrlich weglassen/ablehnen statt plausibel raten" — das ist
   die richtige Vorzugsrichtung für ein Überwachungssystem und zieht
   sich nachweisbar durch (kein Label statt falsches Label, Absenz statt
   Einfrieren, Restore-Ablehnung statt Fremd-Zustand).

## Ehrliche Grenzen

- **Qualitativ, nicht quantitativ:** keine Ausfallraten, keine
  Wahrscheinlichkeits-Ziele je Schwereklasse.
- **KI-erstellt, Betreiber-geprüft** — keine unabhängige
  Sicherheitsbewertung durch einen Dritten, kein Regulator-Nachweis;
  die verbindliche Schwere-Einstufung braucht den Betriebs-Kontext
  (Luftraum, Verkehrsdichte, Rückfall-Verfahren), den nur der Betreiber
  festlegen kann. Genau so steht es in §0 des Dokuments.
- Die FHA ist ein **lebendes Dokument** — ihr Wert hängt an der
  Pflege-Regel (§7), nicht am Erstellungsdatum.
