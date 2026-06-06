# Arbeitsstand (Handover-Notiz)

> **Zweck:** Diese Datei ist der schnelle Wiedereinstieg — egal ob am PC oder
> Handy. Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

- **Zuletzt aktualisiert:** 2026-06-06
- **Branch:** `claude/radar-track-calculator-BoaU8`
- **Letzter Commit:** Querschnitts-Prinzipien (ADR 0003/0004, Requirements,
  Glossar) verankert.

---

## 1. Wo wir gerade stehen

- **M1 (Simulator) ist fertig** und gepusht: Workspace + drei Crates
  (`firefly-geo`, `firefly-core`, `firefly-sim`).
- Qualität: **24 Tests + 1 Doctest grün**, Clippy sauber, `cargo fmt` ok.
- Die **Arbeitsregeln** stehen (`CLAUDE.md`): *erst erklären, dann bauen*;
  keine unerklärten Begriffe; Doku ist Teil der Leistung.
- **Dokumentation** aufgebaut: Glossar, M1-Erklärung, ADRs 0001–0004,
  Anforderungs-Register mit Rückverfolgbarkeit.

## 2. Gesetzte Entscheidungen (Fundament, nicht mehr offen)

| Thema | Entscheidung | Quelle |
|-------|--------------|--------|
| Engine-Sprache | **Rust** (Frontend später JS) | ADR 0001 |
| Datenformat | **ASTERIX** (CAT048/021/062) | ADR 0001 |
| Erster Umfang | Simulator (M1) + Single-Radar-Tracker (M2) | ADR 0001 |
| Darstellung | **2D-Karte** | ADR 0001 |
| Sprache | Code Englisch, Doku/Chat Deutsch | ADR 0002 |
| Architektur | **Cloud-nativ**, Kubernetes, anbieter-neutral | ADR 0003 |
| Assurance | **Zertifizierungs-fähig**, ED-153 + ED-109A/DO-278A | ADR 0004 |

## 3. Nächster Schritt (hier geht es weiter!)

➡️ **Wir stehen am Start von M2 (der eigentliche Tracker).** Claude wartet auf
das **Go**, um *Häppchen 2.1 zu erklären* (noch kein Code):

> **Häppchen 2.1 — Vom Plot zur kartesischen Messung mit Unsicherheit.**
> Fachlich: Warum muss der Tracker die polare Radarmessung „begradigen", und
> warum ist die Unsicherheit zigarrenförmig? Technisch: Converted-Measurement-
> Ansatz, Kovarianzmatrix, Einstieg von `nalgebra`.

Erst Erklärung → Rückfragen/Go → dann kleine, testbare Umsetzung.

## 4. M2-Plan in Häppchen (so zerlegen wir den Tracker)

- [ ] **2.1** Plot → kartesische Messung + Mess-Kovarianz (Converted Measurement)
- [ ] **2.2** Kalman-Filter mit Constant-Velocity-Modell (Prädiktion + Update); `nalgebra`
- [ ] **2.3** Gating: Validierungsregion über Mahalanobis-/χ²-Distanz
- [ ] **2.4** Datenassoziation: GNN (global beste Zuordnung mehrerer Plots↔Tracks)
- [ ] **2.5** Track-Lifecycle: Initiierung (M-aus-N), Bestätigung, Coasting, Löschung
- [ ] **2.6** Tracker als reine, deterministische Funktion + serialisierbarer Zustand
       (erfüllt NFR-CLOUD-001/002/003)
- [ ] **2.7** Güte-Metriken gegen die Ground Truth (RMSE, Track-Kontinuität)

Jeder Haken wird erst gesetzt, wenn die Qualitäts-Gates (CLAUDE.md §5) erfüllt
sind und die Anforderung im Register rückverfolgbar steht.

## 5. Offene Punkte / später entscheiden

- **Message-Bus-Technologie** (z. B. NATS/Kafka) — erst relevant ab M3, dann ADR.
- **Coverage-Werkzeug** (z. B. `cargo llvm-cov`) — einführen, sobald V&V-Nachweise
  greifbar werden.
- **Sicherheitsanalyse (FHA/Hazards)** — sinnvoll, sobald Tracker-Funktionen
  stehen, gegen die man Gefährdungen bewerten kann.
- **Frontend-Kartenbibliothek** (Leaflet vs. MapLibre) — Entscheidung in M3.

## 6. So steige ich wieder ein (Kurzbefehle)

```bash
cargo test --workspace                     # alles grün?
cargo run --example demo -p firefly-sim    # M1-Simulator live sehen
```

Doku-Einstieg: `docs/README.md` → Glossar, Meilensteine, ADRs, Requirements.
