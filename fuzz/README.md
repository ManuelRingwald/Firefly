# Fuzzing (QW.2 der ARTAS-Roadmap)

Coverage-geführtes Fuzzing (cargo-fuzz/libFuzzer) der **Vertrauensgrenzen-Parser**
— Charter §8: „kein Panic auf Eingabe-Daten", Nachweis für den
Assurance-Block (`docs/design/artas-gap-roadmap.md`).

| Target | Prüfling | Grenze |
|---|---|---|
| `cat048_decode` | `firefly_asterix::decode_target_reports` | rohe Radar-UDP-Datagramme (ADR 0017/0028) |
| `cat062_decode` | `firefly_asterix::decode_data_block` | Konsum-Seite des eigenen Stroms (Recorder/Replay) |
| `cat063_decode` | `firefly_asterix::decode_sensor_block` | dito, inkl. RE-Feld-Walk (ICD 3.1.0) |
| `cat065_decode` | `firefly_asterix::decode_status_block` | dito |
| `sources_parse` | `firefly_server::sources::parse_sources` | Orchestrator-Konfiguration (ADR 0023) |

**Invariante:** beliebige Eingabe ⇒ `Ok`/`Err`, nie Panic.

## Lokal ausführen

```bash
rustup toolchain install nightly   # einmalig
cargo install cargo-fuzz           # einmalig

# Ein Target, zeitbegrenzt (Sekunden), mit Seed-Korpus:
cargo +nightly fuzz run cat048_decode fuzz/corpus/cat048_decode fuzz/seeds/cat048_decode -- -max_total_time=300

# Alle Targets nacheinander:
for t in $(cargo +nightly fuzz list); do
  cargo +nightly fuzz run "$t" "fuzz/corpus/$t" "fuzz/seeds/$t" -- -max_total_time=300
done
```

- `fuzz/seeds/<target>/` — **kuratierte, committete** Start-Eingaben (byte-genaue
  Referenz-Dumps der Encoder-Tests bzw. ein voll belegtes `FIREFLY_SOURCES`).
- `fuzz/corpus/<target>/` — der lokal **wachsende** Arbeits-Korpus (gitignored);
  neue interessante Eingaben landen hier.
- **Fund:** libFuzzer schreibt die auslösende Eingabe nach `fuzz/artifacts/…`.
  Reproduzieren mit `cargo +nightly fuzz run <target> <artifact-datei>`, fixen,
  und die Eingabe als Regressionstest im betroffenen Crate einfrieren.

## CI

Der Job **Fuzz (time-boxed, nightly)** in `.github/workflows/ci.yml` lässt jedes
Target 60 s laufen — ein Regressionsnetz, kein Ersatz für gelegentliche lange
lokale Läufe.

Dieses Verzeichnis ist bewusst **kein** Workspace-Member (Root-`Cargo.toml`
`exclude`): die Fuzz-Targets brauchen Nightly/libFuzzer, die stabilen
Qualitäts-Gates bleiben davon unabhängig.
