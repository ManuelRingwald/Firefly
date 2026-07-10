# QW.2 — Coverage-geführtes Fuzzing der Vertrauensgrenzen-Parser

> **Anforderung:** NFR-SAFE-002 · **ARTAS-Roadmap:** QW.2 (Assurance-Block) ·
> **Einstufung:** S2–S3 (umgesetzt auf Fable 5)

## Fachlich: Warum?

Der Charter (§8) fordert für Eingabe-Pfade „kein Panic auf Eingabe-Daten" und
sieht Fuzzing des Parsers ausdrücklich vor — vorhanden war aber nur ein
deterministischer Byte-Flip-Test am CAT048-Decoder (~60 Nachbar-Eingaben eines
gültigen Blocks). Ein coverage-geführter Fuzzer prüft Millionen strukturell
verschiedener Eingaben und ertastet sich selbstständig in die tiefsten
Parser-Pfade. Ein Panic an einer Vertrauensgrenze wäre ein
**Denial-of-Service aufs Lagebild per einzelnem Datagramm** (ADR 0017 schützt
den Multicast-Pfad nur per Netz-Isolation). Für den ARTAS-Anspruch ist der
Fuzz-Lauf zugleich ein Assurance-Nachweis (ED-109A-Orientierung).

## Was gebaut wurde

- **`fuzz/`** — eigenständiges cargo-fuzz-Workspace (bewusst **kein**
  Workspace-Member, Root-`Cargo.toml` `exclude`: Fuzzing braucht Nightly/
  libFuzzer, die stabilen Qualitäts-Gates bleiben unabhängig). Fünf Targets:
  `cat048_decode`, `cat062_decode`, `cat063_decode`, `cat065_decode`,
  `sources_parse`. Invariante je Target: beliebige Eingabe ⇒ `Ok`/`Err`,
  nie Panic.
- **Seed-Korpus** (`fuzz/seeds/<target>/`): die byte-genauen Referenz-Dumps
  der Encoder-Tests (CAT048-Referenzblock, CAT062-Referenz-Track,
  CAT063 zwei Sensoren + RE/SRC-REASON, CAT065-Heartbeat) und ein voll
  belegtes `FIREFLY_SOURCES` mit allen vier Quelltypen.
- **CI-Job „Fuzz (time-boxed, nightly)"** (`.github/workflows/ci.yml`):
  60 s je Target auf jedem PR/main-Push; gefundene Crash-Eingaben werden als
  Workflow-Artefakt gesichert. Bedienung/lokale Läufe: `fuzz/README.md`.

## Erster Ertrag: echter Bug in der FSPEC-Maschinerie gefunden & gefixt

Der Fuzzer fand **innerhalb von Sekunden** in **allen vier** ASTERIX-Decodern
denselben Absturz — Wurzel in der gemeinsamen FSPEC-Verarbeitung
(`fspec::parse`):

- **Befund:** Die FRN wurde als `(consumed − 1) * 7 + position + 1` in
  **u8-Arithmetik** berechnet. Der FX-Mechanismus erlaubt aber beliebig lange
  Oktett-Ketten — ein feindliches Datagramm mit > 36 verketteten FSPEC-Oktetten
  ließ die Rechnung überlaufen: **Panic** in Builds mit Debug-Assertions,
  **stilles Wrapping** (= falsch gelesene FRNs, potenziell falsch gerahmte
  Payloads) im Release-Build. Der Byte-Flip-Test konnte das prinzipiell nicht
  finden — er mutiert nur einzelne Bytes eines kurzen gültigen Blocks.
- **Fix:** `fspec::parse` ist jetzt **begrenzt und fehlbar**:
  `MAX_FSPEC_OCTETS` = 36 (deckt FRN 1..=252 — ein Mehrfaches jeder realen
  UAP; CAT062 endet bei FRN 35), FRN-Arithmetik in `usize`, Überlänge ⇒
  `Err(FspecTooLong)`. Alle vier Decoder-Fehlertypen haben eine neue
  `FspecTooLong`-Variante; die vier Aufrufstellen mappen sauber.
- **Eingefroren:** je Decoder ein Regressionstest
  (`overlong_fspec_chain_is_rejected_not_panicked`) plus zwei
  `fspec`-Grenzwert-Tests (36 Oktette ok mit exakter FRN 252; 37+ ⇒ Err).
- **Schnittstellen-Wirkung: keine.** Kein gültiges Wire-Format ist betroffen —
  abgelehnt wird nur, was ohnehin in keinem UAP dekodierbar wäre. ICD
  unverändert; Wayfinders Decoder sollte dieselbe Härtung bekommen
  (Cross-Project-Issue, `from-firefly`).

`sources_parse` lief > 5 Mio. Ausführungen ohne Befund — der
`FIREFLY_SOURCES`-Parser (serde_json-basiert) ist robust.

## Nachweis / Gates

- Original-Crash-Eingaben der vier Funde: reproduziert → nach Fix sauber
  `Err` statt Panic; frischer Fuzz-Lauf (120 s je Target) ohne neue Funde.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets`,
  `cargo fmt` grün. Register: NFR-SAFE-002 mit voller Rückverfolgbarkeit.
- Keine neuen Env-Variablen, keine Betriebsmodus-Änderung (INSTALLATION/
  TECHNICAL geprüft, unverändert; Nightly wird nur fürs Fuzzing gebraucht,
  dokumentiert in `fuzz/README.md`).
