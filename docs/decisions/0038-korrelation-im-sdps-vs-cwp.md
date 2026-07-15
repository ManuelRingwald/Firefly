# ADR 0038: Flugplan-Korrelation zentral im SDPS (Firefly), nicht am CWP

**Status:** vorgeschlagen (2026-07-15, Betreiber-Go FPL.0) — wird mit der
Wayfinder-Abstimmung ratifiziert · **Bezug:** ARTAS-Gap-Roadmap AP-FPL,
`docs/design/korrelation-code-duplikate-weeze.md` (Betriebs-Lektion Weeze),
ADR 0036 (`identity_conflict`, SPEC.1), Wayfinder EFS-1 (elektronische
Flugstreifen — **wartet auf diese Entscheidung**)

## Kontext — in normaler Sprache

Der Lotse sieht auf dem Schirm, *wo* ein Ziel ist. Die **Korrelation**
beantwortet die Frage, *wer* es ist: „Dieses Radarziel **ist** Flug DLH123
aus dem Flugplan." Erst damit werden Flugstreifen, Freigaben und
Konfliktprüfung möglich. Die Grundsatzfrage dieses ADRs: Führt der
**Rechenkern** (Firefly, das SDPS) diese Ehe **einmal zentral** für alle —
oder jede **Anzeige** (Wayfinder, das CWP) **für sich**?

Beide Systeme sind betroffen: Wayfinder plant elektronische Flugstreifen
(EFS-1) und braucht korrelierte Tracks; Firefly hat mit SPEC.1 bereits die
Identitäts-Disziplin (Duplikat-Erkennung, `identity_conflict`), auf der
eine seriöse Korrelation aufbauen muss. Die Betriebs-Lektion aus Weeze
zeigt, wie eine naive Code-Korrelation scheitert (ORCAM-Squawk-Duplikate
an der Grenze ⇒ ein Flugplan auf zwei Tracks).

## Entscheidung

**Die Korrelation ist eine SDPS-Server-Funktion und läuft zentral in
Firefly.** Wayfinder stellt dar und greift ein, korreliert aber nicht
selbst.

Begründung:

1. **Eine Wahrheit:** Zwei Arbeitsplätze, die dasselbe Ziel verschieden
   korrelieren, sind operativ gefährlich (widersprüchliche Labels bei der
   Übergabe). Zentral heißt: ein Lagebild, eine Zuordnung — für alle
   Konsumenten, auch künftige Dritt-Systeme am selben Multicast.
2. **ARTAS-Konsistenz:** ARTAS ist Tracker **und** Server; die Korrelation
   (und ihr Draht-Format I062/390) ist dort Server-Funktion. Unsere
   Arbeitsteilung „Firefly rechnet, Wayfinder zeigt" (ADR 0014) setzt das
   konsequent fort.
3. **Die Zutaten liegen im SDPS:** Callsign (I062/245), Squawk,
   ICAO-Adresse, `identity_conflict` (SPEC.1), Kinematik fürs
   Plausibilisieren — alles ist im Tracker frisch und historisiert
   vorhanden; am CWP käme es nur gefiltert an.
4. **Multi-Tenant bleibt konsistent:** Wayfinder orchestriert ohnehin
   je Mandant eigene Firefly-Instanzen (Quell-Kontrakt, ADR 0023) — ein
   Mandant mit eigener Flugplan-Quelle bekommt sie an *seiner* Instanz;
   die Anzeige-Mandantierung bleibt unverändert bei Wayfinder.

**Rollen:**

| System | Rolle bei der Korrelation |
|--------|---------------------------|
| **Firefly** | Korreliert automatisch (Regeln unten), führt den Korrelations-Zustand je Track, sendet ihn im Strom (I062/390, FPL.2), nimmt **manuelle Kommandos** entgegen (Korrelation setzen/lösen — API in FPL.2, Ausbau SRV.2) |
| **Wayfinder** | Zeigt die Korrelation im Label/EFS, bietet die manuelle Korrelation als Bedienhandlung an (ruft Fireflys Kommando-API), mandantiert die **Sicht** — nie die Zuordnung selbst |

**Korrelations-Regeln (aus der Weeze-Notiz, verbindlich für FPL.1):**

1. **Callsign-first:** Mode-S Aircraft Identification (I062/245) gegen das
   Flugplan-Callsign — praktisch eindeutig, kein ORCAM-Problem.
2. **Squawk nur bei Eindeutigkeit:** Code-Korrelation ausschließlich, wenn
   der Code im Interessensgebiet eindeutig ist; ein Track mit
   `identity_conflict` (SPEC.1) ist **nie** Auto-Korrelations-Kandidat
   über den Code; Code 1000 ist nie ein Schlüssel. Duplikat ⇒ keine
   Auto-Korrelation + Signal (manuelle Korrelation) — ein doppeltes Label
   ist gefährlicher als ein unkorreliertes.
3. **Räumlich/zeitlich plausibilisiert:** nur innerhalb definierter
   Volumina und im Erwartungsfenster des Flugplans (ETA, Profil).
4. **Beobachtbarkeit:** Korrelations-/Duplikat-Ereignisse als
   Metrik/Log (Meldegrundlage Richtung CCAMS/Nachbar-ANSP).

**Verworfen — Korrelation am CWP (Wayfinder):** je Arbeitsplatz/Mandant
potenziell verschiedene Zuordnungen (inkonsistente Labels), duplizierte
Logik in jedem Client, kein Standard-Draht-Format (I062/390 bliebe leer,
Dritt-Konsumenten außen vor), und die Korrelations-Rohdaten (frische
Identitäts-Historie, Konflikt-Flags) müssten erst zum Client transportiert
werden. **Verworfen — Hybrid** (Basis zentral, Verfeinerung am CWP):
zwei Wahrheiten mit Drift-Garantie.

## Konsequenzen

- **Wayfinder EFS-1 kann gegen den Vertrag planen:** „Korrelation kommt
  aus dem Strom (I062/390) + Kommando-API für manuelle Eingriffe" —
  Abstimmungs-Issue im Wayfinder-Repo (`from-firefly`).
- **FPL.1** (nächstes Häppchen, S5): minimaler FDPS-Eingangs-Kontrakt
  (Flugplan-Quelle je Instanz, env-/API-getrieben — eigenes Design im
  Häppchen) + die Regeln 1–3 als Code mit Testabdeckung.
- **FPL.2:** I062/390-Encoding (ICD-Bump, additiv) + manuelle
  Korrelations-Kommandos (API), Wayfinder-Nachzug.
- **Ehrliche Grenze:** Zentral heißt auch: Fällt Firefly aus, fällt die
  Korrelation aus — die HA-Antwort (Main/Standby, AP-HA) gilt für diese
  Funktion mit. Und ein Mandant, der *andere* Flugpläne sehen will,
  braucht eine eigene Instanz — das ist das bestehende
  Orchestrierungs-Modell, keine neue Einschränkung.
