# SPEC.1 — Duplikat-ICAO-Auflösung + Koaleszenz-Behandlung

> **Anforderung:** FR-TRK-045 (+ Revision FR-TRK-031) · **ADR:** 0036 ·
> **ICD:** unberührt · **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich: Warum?

Identität ist im Feld **nicht verlässlich eindeutig**: ORCAM-Squawks
duplizieren an Grenzen (Lektion Weeze), Conspicuity 1000 ist per Design
mehrdeutig, und auch ICAO-Adressen duplizieren durch Fehlkonfiguration.
Gleichzeitig hat JPDA eine strukturelle Schwäche: ein dauerhaft
unauflösbares Track-Paar **koalesziert** auf den Mittelpunkt — genau in
den engen Situationen, in denen der Lotse Trennung am dringendsten
braucht.

## Technik

1. **ICAO-Fastpath gegated:** harte Assoziation nur im kinematischen
   Gate; ein Match weit außerhalb ist Duplikat-Evidenz und gründet einen
   eigenen Track (vorher: Teleport des Träger-Tracks — der
   „springende Misch-Track").
2. **Duplikat-Scan:** je Fusions-Gelegenheit werden lebende Tracks nach
   ICAO und Mode 3/A gruppiert; Gruppen > 1 flaggen alle Träger
   (`identity_conflict`, WARN, automatisch löschend). Getrennte Führung,
   nie Merge.
3. **Koaleszenz-Wächter** (`jpda::decouple_coalescing_pairs`): bei
   d² < 4 (2σ, kombinierte Positions-Kovarianz) behält jede geteilte
   Messung nur der stärkere Anwärter; β-Masse wandert ins β₀ des
   Schwächeren (Zeilensumme 1, deterministischer Tie-Break). Im
   Normalverkehr No-op.
4. **Registrierungs-Deckel:** Identitäts-Korrespondenzen mit > 5 km
   Sichtungs-Abstand werden verworfen (Duplikat-Kontamination, keine
   Bias-Evidenz). Erste, zeitfenster-basierte Idee verworfen — sie kann
   Duplikat nicht von Scan-Wiederbesuch unterscheiden.

**Messung** (End-to-End, 150-m-Parallel-Paar, 50 km, σ_az 0,08°):
ohne Wächter oszilliert das Paar auf ≤ 113 m zusammen; mit Wächter hält
es 148–150 m. Der Negativ-Check (Wächter deaktiviert ⇒ Test rot) ist
dokumentiert — der Test beißt.

## Ehrliche Grenzen

- **Kein Split/Merge-Manager:** SPEC.1 verhindert falsches
  Zusammenziehen; aktives Erkennen historischer Fehl-Merges mit
  Track-Historien-Chirurgie wäre ein eigenes Häppchen.
- **Flag nur intern** (Log-Sichtbarkeit): Draht-/WS-Ausleitung bewusst
  bis AP-FPL aufgeschoben (dort entsteht der Konsument).
- **Wächter-Schwelle fix** (2σ): keine Adaptivität nach Verkehrsdichte.
- Manöver-Ausreißer eines ICAO-Trägers außerhalb des Gates re-assoziiert
  jetzt kinematisch statt hart — bewusster Trade (ADR 0036).
