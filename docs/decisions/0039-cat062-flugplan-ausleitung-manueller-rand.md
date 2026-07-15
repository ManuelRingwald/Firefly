# ADR 0039: CAT062-Flugplan-Ausleitung (I062/390) + manueller Korrelations-Rand

**Status:** akzeptiert (2026-07-15, Betreiber-Go FPL.2) · **Bezug:**
ADR 0038 (Korrelation zentral im SDPS), ARTAS-Gap-Roadmap AP-FPL,
FR-TRK-047 (FPL.1: Eingang + Auto-Korrelation), ICD 3.7.0,
NFR-SEC-001/ADR 0017 (Browser-Rand-Auth)

## Kontext — in normaler Sprache

FPL.1 hat die Flugplan-Zuordnung **berechnet**, aber sie stand nur im
WebSocket-JSON von Fireflys eigener Showcase-Ansicht. Das eigentliche ASD
(Wayfinder) hört auf den CAT062-Multicast-Strom — der Produktions-Vertrag —
und sah davon nichts. Außerdem verweigert die Automatik in Zweifelsfällen
**bewusst** (Weeze-Duplikate, Conspicuity 1000, Identitätskonflikt): genau
dort braucht der Lotse eine Hand aufs Ergebnis, sonst bleibt ein
verweigerter Track dauerhaft unbeschriftet.

## Entscheidung

1. **I062/390 auf den Draht (ICD 3.7.0, additiv).** Das Ergebnis der
   zentralen Korrelation wird als Standard-Item *Flight Plan Related Data*
   (FRN 21, Compound) gesendet — minimal die Subfelder, die unser
   Plan-Feldsatz trägt: **CSN** (Plan-Callsign, 7 Oktette ASCII), **DEP**
   und **DST** (je 4 Oktette ICAO-Locator). Nur bei korreliertem Track;
   ein unkorrelierter Record bleibt byte-identisch (kein Wire-Bruch,
   FRN 21 liegt im ohnehin vorhandenen 3. FSPEC-Oktett). Weitere
   Subfelder wachsen additiv mit dem EFS-Bedarf (Wayfinder #244).
2. **Manuelle Korrelations-Kommandos am HTTP-Rand.** Drei Endpunkte
   (`POST /correlation`, `DELETE /correlation/{track}`,
   `GET /correlation`) verwalten eine **Override-Karte** (Draht-
   Track-Nummer → Pin):
   - **Pin auf Plan** (`callsign` gesetzt): der Track trägt diesen Plan —
     manuell schlägt Automatik. Ein unbekannter Callsign ist **422**
     (nie ein stiller No-op-Pin).
   - **Pin auf unkorreliert** (`callsign` fehlt/null): der Track bleibt
     leer — die Automatik darf ein vom Lotsen entferntes Label **nicht**
     wieder anbringen.
   - **Löschen des Pins**: die Automatik übernimmt wieder.
3. **Lebenszyklus-Kopplung an TSE.** Draht-Track-Nummern sind
   pool-verwaltet und werden **wiederverwendet** (FR-TRK-035). Ein Pin
   stirbt deshalb **mit dem TSE-Record seines Tracks** (deterministisch,
   genau einmal je Track) — sonst würde ein veralteter Pin das nächste
   Luftfahrzeug beschriften, das die Nummer erbt.
4. **Auth am Kommando-Rand.** Dieselbe Token-Hürde wie `/ws`
   (`FIREFLY_WS_TOKEN`), aber **nur** als `Authorization: Bearer`-Header —
   kein Query-Fallback für zustandsändernde Requests (Query-Strings landen
   in Logs). Die Origin-Allowlist greift nur, wenn ein `Origin`-Header
   mitkommt (Browser-Kontext); Server-zu-Server-Clients (Wayfinder-
   Backend) senden keinen und passieren über das Token.

## Begründung

- **Ein Vertrag für alle:** ADR 0038 verlangt *eine* Zuordnung für alle
  Konsumenten — die gibt es erst, wenn sie auf dem Produktions-Vertrag
  (CAT062) liegt, nicht nur in der Demo-Ansicht.
- **Standard statt Vendor-Feld:** I062/390 ist das EUROCONTROL-Item genau
  für diesen Zweck; ein ARTAS-gespeister Konsument liest es ohne privates
  Profil.
- **Manuell schlägt Automatik, sichtbar:** Die Automatik ist bewusst
  konservativ (falsches Label schlimmer als fehlendes); der manuelle Rand
  ist das ehrliche Ventil — und `firefly_correlation_manual` macht den
  Umfang des manuellen Eingriffs beobachtbar.
- **Zustand minimal:** Die Override-Karte ist der einzige neue gehaltene
  Zustand; sie lebt am Ausgabe-Rand (Tracker-Kern bleibt flugplan-frei)
  und räumt sich über TSE deterministisch selbst auf.

## Konsequenzen

- ICD 3.7.0 (additiv); Wayfinder zieht ohne Lockstep nach (Issue folgt,
  gebündelt mit den additiven WS-JSON-Feldern aus FPL.1).
- Der Override ist **flüchtig** (im Prozess): ein Neustart verliert
  manuelle Pins. Persistenz gehört zu HA.1 (Snapshot/Restore) — ehrlich
  benannt, nicht versteckt.
- Kein Audit-Trail *wer* gepinnt hat (kein Benutzerkonzept am Rand);
  Mandanten-/Benutzer-Attribution ist Wayfinder-Sache (dessen ADR 0014).
- I062/245 (gesendete Identität) und I062/390-CSN können abweichen —
  bewusst: die Differenz ist die Anzeige-Information „Callsign-Mismatch".
