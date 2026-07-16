# ASSUR.2 — Coverage-Messung + Property-Tests + Genauigkeits-Nachweisdossier

> **Anforderung:** NFR-ASSUR-001 · **ADR:** — · **ICD:** unberührt ·
> **Einstufung:** S3 · umgesetzt auf Fable 5 (Roadmap-Empfehlung: Sonnet) ·
> **Das 100-%-Häppchen der ARTAS-Gap-Roadmap.**

## Fachlich

Das letzte Roadmap-Häppchen macht aus verstreuten Nachweisen ein
**geschlossenes Beweis-Paket** — die drei Fragen jedes Audits:

1. **„Wie viel eures Codes ist getestet?"** — jetzt gemessen statt
   behauptet: **88 % der Zeilen** über den Workspace, Tracker-Kern
   **98,4 %**, Draht-Schicht 93 % (`cargo llvm-cov`).
2. **„Testet ihr Beispiele oder Eigenschaften?"** — jetzt beides: sieben
   **Property-Tests** werfen je Lauf hunderte Zufallsfälle auf die
   Kern-Invarianten (Geodäsie-Roundtrip, LSB-genauer
   CAT062-Encode/Decode-Roundtrip, Decoder-Totalität, Squawk-Oktal über
   alle 4 096 Codes).
3. **„Wo steht die Genauigkeit schwarz auf weiß?"** — im neuen
   **Dossier** (`docs/verification/genauigkeits-dossier.md`): jede
   Behauptung mit Beleg und Reproduktions-Befehl, ehrliche Grenzen
   vorangestellt.

## Die Property-Tests haben sofort geliefert

Zwei Funde beim Einbau — beide gegen die eigenen **Test-Entwürfe**, nicht
gegen den Produktcode, und beide als Kalibrier-Protokoll im
Test-Kommentar dokumentiert:

1. **Präzisionsgrenze gemessen:** Die 1-µm-Wunsch-Toleranz des
   Geodäsie-Roundtrips fiel durch — der reale f64-Restfehler der
   geschlossenen ECEF→geodätisch-Form beträgt ~1,4 µm auf 12,8 km Höhe
   (~3,5 µm auf 20 km). Toleranz jetzt **gemessen** kalibriert
   (0,1 mm — sechs Größenordnungen unter dem 5-m-Draht-LSB).
2. **Antimeridian:** Punkte jenseits ±180° Länge kommen korrekt in den
   Hauptbereich normalisiert zurück (−180,58° ↔ +179,42° — derselbe
   physische Punkt); Längen werden im Test seitdem als Winkel
   (modulo 360°) verglichen.

Genau dafür sind Properties da: Sie fanden in Stunde eins die Fälle, an
die kein Beispiel-Test gedacht hätte.

## Technik

- **Coverage:** `cargo llvm-cov --workspace --summary-only`
  (cargo-llvm-cov 0.8.7 + `llvm-tools-preview` — in dieser Sandbox
  installierbar und **gelaufen**, Zahlen sind gemessen, nicht geschätzt).
  Ausreißer-Analyse im Dossier §1 (Binary-/CLI-Einstiege 0 %, Logik in
  getestete Module extrahiert). **Bewusst kein CI-Schwellwert-Gate.**
- **Property-Tests** (`proptest = "1"`, dev-dependency in `firefly-geo`,
  `firefly-asterix`, `firefly-fpl`): 7 Properties in `tests/properties.rs`
  je Crate; Stabilität über mehrere Zufalls-Seeds verifiziert.
- **Dossier:** §1 Coverage (mit Ausreißer-Analyse), §2
  Tracking-Genauigkeit (HA.4-Zahlen + Instrument-Tests), §3 Draht-/
  Parser-Korrektheit (Referenz-Dumps + Properties + Fuzzing + COMPASS),
  §4 Kapazität (CAP.1/2), §5 Verweise (FHA, Determinismus, unsafe-frei,
  Register), §6 „Was dieses Dossier NICHT belegt".

## Ehrliche Grenzen

- **Coverage ≠ Korrektheit** (Dossier §0) — und die Binary-Einstiege
  (`main.rs`, CLI-Wrapper) bleiben bei 0 %: Verdrahtung testet man im
  Deployment-Smoke, nicht im Unit-Test.
- **Alle Genauigkeits-Zahlen = Simulator-Wahrheit**; Live-Referenz-Messung
  existiert nicht (COMPASS-Betreiber-Lauf weiterhin offen, HA.5).
- Property-Läufe sind **zufalls-getrieben** (jeder CI-Lauf andere Fälle):
  großartig fürs Finden, aber ein grüner Lauf beweist keine
  Vollständigkeit — die Referenz-Dumps bleiben die byte-genaue Verankerung.
