# FPL.1 — Flugplan-Eingang + automatische Korrelation

> **Anforderung:** FR-TRK-047 · **ADR:** 0038 · **ICD:** unberührt
> (I062/390 = FPL.2) · **Einstufung:** S5 · umgesetzt auf Fable 5

## Fachlich

Firefly weiß jetzt, **welcher Flugplan zu welchem Track gehört**. Der
Lotse sieht damit nicht mehr nur „ein Ziel mit Callsign", sondern „der
Flug DLH123 von EDDL nach EDDF" — die Grundlage für elektronische
Streifen (EFS) und jede Freigabe-Logik. Die Zuordnung passiert **einmal
zentral im SDPS** (ADR 0038): alle Konsumenten sehen dieselbe
Korrelation, statt dass jedes Display selbst rät (ARTAS-konsistent).

Die Regeln sind bewusst vorsichtig — Lektion Weeze: ein **falsches**
Label am Track ist operationell gefährlicher als ein fehlendes. Im
Zweifel korreliert Firefly **nicht** und macht die Verweigerung sichtbar
(Metrik), statt zu raten.

## Technik

- **Eingang** (`firefly-fpl::FplConfig`): `FIREFLY_FLIGHT_PLANS`
  (JSON-Array) nach dem Meteo-Muster — kaputte Konfiguration ist ein
  **Start-Fehler**, unset heißt leere Planliste (INFO), nie stiller
  Teilbetrieb. Harte Fehler: leeres Callsign, Squawk > 0o7777,
  nicht-endliche Erwartungszeit, doppeltes normalisiertes Callsign.
- **Korrelation** (`firefly-fpl::CorrelationService`), Regeln verbindlich
  aus ADR 0038:
  1. **Callsign zuerst** (normalisiert: trim + uppercase). Greift auch
     bei Identitätskonflikt — der Konflikt sperrt nur den Code-Fallback.
  2. **Squawk nur als Fallback**, und nur wenn der Code eindeutig unter
     allen Plänen ist **und** der Track nicht `identity_conflict` trägt
     **und** der Code nie Conspicuity 0o1000 ist **und** die
     Erwartungszeit plausibel liegt (±45 min).
  3. Jede Verweigerung ist ein sichtbares Ergebnis (`SquawkRefused`).
- **Anwendung am Ausgabe-Rand** (`live::apply_correlation`): zustandslos
  je Output-Tick, nach der QNH-Korrektur. Der Tracker-Kern bleibt
  flugplan-frei — Korrelation ist eine Ausgabe-Stufen-Funktion.
- **Draht:** `SystemTrack` bekommt **additive** WS-JSON-Felder
  `identity_conflict: bool` (SPEC.1-Flag, jetzt exportiert) und
  `flight_plan: Option<FlightPlanRef>` (Callsign/ADEP/ADES). CAT062
  unberührt.
- **Metriken:** `firefly_flight_plans`, `firefly_tracks_correlated`,
  `firefly_correlation_refused` (Gauges, On-Tick-Kette).

## Ehrliche Grenzen

- **Zustandslos je Tick:** Die Zuordnung wird jeden Output-Tick neu
  berechnet (akzeptabel, weil Callsign/Squawk auf Tracks klebrig sind).
  Gehaltener Korrelations-Zustand und **manuelle
  Übersteuerung/Entkopplung** durch den Lotsen sind FPL.2.
- **Nur Zeitfenster, kein Raum:** Räumliche Plausibilität (Track nahe
  der Route?) braucht Routen-Geometrie, die der minimale Plan-Feldsatz
  nicht trägt — ehrlich benannt in ADR 0038, Folgearbeit.
- **Env-Provider:** `FIREFLY_FLIGHT_PLANS` ist der erste Provider; eine
  Live-FDPS-Anbindung (Datei/Netz, Reload) ist ein eigener Folge-ADR.
- **Feldsatz minimal:** Callsign + optional Squawk/ADEP/ADES/
  Erwartungszeit; wächst **additiv** nach Wayfinder-#244-Feedback
  (EFS-Bedarf). ADR 0038 bleibt „vorgeschlagen", bis Wayfinder
  bestätigt.
- **Kein I062/390:** Der Flugplan steht nur im WS-JSON; die
  CAT062-Ausleitung (ICD-Bump) ist FPL.2.
