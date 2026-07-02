# CI & Branch-Schutz — Firefly

Diese Datei erklärt, was die automatische Prüfung (CI) tut und wie `main`
geschützt wird. Sie ist bewusst einsteiger-freundlich.

## Was die CI tut

Die CI (`.github/workflows/ci.yml`) läuft **automatisch** bei jedem Push auf
`main` und bei jedem Pull Request gegen `main`. Sie führt dieselben
Qualitäts-Gates aus wie der Charter (CLAUDE.md §5):

| Schritt | Befehl | Zweck |
|---------|--------|-------|
| Format | `cargo fmt --all --check` | Einheitliche Formatierung |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | Warnungen = Fehler |
| Tests | `cargo test --workspace --locked` | Alle Tests grün |

`--locked` erzwingt die committete `Cargo.lock` → reproduzierbare Builds; eine
veraltete Lockfile lässt die CI **laut** fehlschlagen (statt still andere
Versionen zu ziehen).

Der CI-Status erscheint an jedem PR als **grüner Haken** oder **rotes Kreuz**.

## `main` schützen (einmalig im GitHub-Web-UI)

> Wichtig: Der Branch-Schutz lässt sich erst als „Pflicht" verlangen, **nachdem
> die CI mindestens einmal gelaufen ist** (GitHub muss den Check-Namen einmal
> gesehen haben). Also: erst diesen CI-PR mergen, CI einmal laufen lassen, dann
> die Regel anlegen.

1. **Settings → Branches → Add branch ruleset** (oder „Add rule" im klassischen
   Branch-Protection-Dialog).
2. **Branch name pattern:** `main`.
3. Aktivieren:
   - ☑ **Require a pull request before merging** (kein direkter Push auf `main`).
     Approvals: **0** (Solo — du mergst deine eigenen PRs).
   - ☑ **Require status checks to pass before merging** →
     ☑ **Require branches to be up to date** → Check **`Rust (fmt · clippy · test)`** auswählen.
   - ☑ **Block force pushes** und **Restrict deletions** (kein Überschreiben/Löschen von `main`).
4. Speichern.

Damit ist die GitHub-Warnung „main is not protected" weg, und nichts landet mehr
ohne grüne CI auf `main`.

### Später verschärfen (wenn ein Team dazukommt)
- Approvals auf **1** setzen (Vier-Augen-Prinzip).
- **Require conversation resolution** + **Require linear history** aktivieren.
