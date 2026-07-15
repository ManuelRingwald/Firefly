# FPL.2 — Flugplan aufs CAT062-Kabel + manuelle Korrelation

> **Anforderung:** FR-TRK-048 · **ADR:** 0039 · **ICD:** 3.7.0 (additiv) ·
> **Einstufung:** S3–S4 · umgesetzt auf Fable 5

## Fachlich

FPL.1 hat die Flugplan-Zuordnung berechnet, aber nur Fireflys eigene
Showcase-Ansicht sah sie. Jetzt steht sie **auf dem Produktions-Vertrag**:
Das ASD (Wayfinder) und jeder andere CAT062-Konsument lesen dieselbe
Zuordnung als Standard-Item **I062/390** — erst damit ist ADR 0038 („eine
Zuordnung für alle") wirklich eingelöst.

Zweitens bekommt der Lotse die **Hand aufs Ergebnis**: Die Automatik
verweigert in Zweifelsfällen bewusst (Weeze-Duplikate, Conspicuity 1000,
Identitätskonflikt) — ohne manuellen Eingriff bliebe so ein Track dauerhaft
unbeschriftet. Per Kommando lässt sich ein Plan auf einen Track **pinnen**,
eine Zuordnung **lösen** (und gegen die Automatik sperren) oder ein Pin
wieder **freigeben**. Manuell schlägt Automatik, immer.

## Technik

- **I062/390** (FRN 21, Compound; `firefly-asterix`): CSN (Plan-Callsign,
  7 Oktette ASCII), DEP/DST (je 4 Oktette ICAO-Locator); Primary Subfield
  1–2 Oktette (FX nur bei DST). Nur bei korreliertem Track; unkorreliert
  byte-identisch alt (FRN 21 liegt im vorhandenen 3. FSPEC-Oktett — kein
  FSPEC-Wachstum, kein Wire-Bruch). Byte-genaue Referenz-Vektoren +
  Decoder-Rückweg (ICD 4.10).
- **Kommando-API** (`firefly-server::app`): `POST /correlation`
  (`{track_number, callsign?}` — Callsign gesetzt = Plan-Pin, 422 bei
  unbekanntem Plan; Callsign fehlt = Pin auf **unkorreliert**),
  `DELETE /correlation/{track}` (zurück zur Automatik, idempotent),
  `GET /correlation` (Liste der Pins). 409 ohne konfigurierte Flugpläne.
- **Override-Karte** (`live::ManualOverrides`): geteilt zwischen HTTP-Rand
  und Live-Task; der Snapshot wendet Pins **vor** der Automatik an. **Pins
  sterben mit dem TSE-Record ihres Tracks** — Draht-Nummern sind
  pool-verwaltet und wiederverwendet (FR-TRK-035); ein stehengebliebener
  Pin würde sonst das nächste Luftfahrzeug mit derselben Nummer labeln.
- **Auth:** `FIREFLY_WS_TOKEN` als `Authorization: Bearer` (kein
  Query-Fallback für zustandsändernde Requests); Origin-Check nur bei
  mitgesendetem `Origin`-Header (Browser), Server-zu-Server passiert
  übers Token.
- **Metrik:** `firefly_correlation_manual` (Pins in Kraft je Tick); der
  Render-Test prüft jetzt auch die FPL.1-Gauges und `firefly_clutter_cells`
  explizit (Lücke aus FPL.1/SPEC.2b geschlossen).

## Ehrliche Grenzen

- **Pins sind flüchtig** (Prozess-Zustand): ein Neustart verliert sie.
  Persistenz gehört zu HA.1 (Snapshot/Restore).
- **Kein Benutzer-Audit** am Rand (wer hat gepinnt?) — kein Benutzerkonzept
  in Firefly; Attribution ist Wayfinder-Sache.
- **I062/390 minimal** (CSN/DEP/DST); weitere Subfelder additiv nach
  Wayfinder-#244-Feedback (EFS-Bedarf).
- I062/245 (gesendete Identität) und I062/390-CSN können abweichen —
  bewusst so: die Differenz ist die Anzeige-Information
  „Callsign-Mismatch".
