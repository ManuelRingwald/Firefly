# Arbeitsstand (Handover-Notiz)

> **Zweck:** Diese Datei ist der schnelle Wiedereinstieg — egal ob am PC oder
> Handy. Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

- **Zuletzt aktualisiert:** 2026-06-06
- **Branch:** `claude/radar-track-calculator-BoaU8`
- **Letzter Commit:** M2 Häppchen 2.4 — Datenassoziation (GNN / Ungarische
  Methode) in `firefly-track`.
- **PR:** #1 (offen).

---

## 1. Wo wir gerade stehen

- **M1 (Simulator) ist fertig** und gepusht: Workspace + drei Crates
  (`firefly-geo`, `firefly-core`, `firefly-sim`).
- **M2 läuft:** Häppchen **2.1–2.4 erledigt** — Crate `firefly-track` mit
  Converted-Measurement (Plot → kartesisch + Kovarianz), Kalman-Filter
  (Constant-Velocity, Predict/Update, Joseph-Form), Gating (Mahalanobis/χ², 2 DOF)
  und Datenassoziation (GNN via Ungarischer Methode, selbst implementiert).
  Erste externe Abhängigkeit `nalgebra` (ADR 0005).
- Qualität: **43 Tests + 1 Doctest grün**, Clippy sauber, `cargo fmt` ok.
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

➡️ **Häppchen 2.5 — Track-Lebenszyklus (die Pro-Scan-Orchestrierung).** Claude
wartet auf das **Go**, um es zuerst zu *erklären* (noch kein Code):

> Fachlich: Wie entstehen, bestätigen, „coasten" und sterben Tracks? Technisch:
> ein `Tracker`, der pro Scan alle Tracks prädiziert, via 2.4 zuordnet, zugeordnete
> Tracks updatet, unzugeordnete coastet/löscht und aus übrigen Plots neue
> (tentative) Tracks gebärt (M-aus-N-Logik). Zustand explizit & serialisierbar
> (NFR-CLOUD-001/002/003).

**Komplexität: S4 → Opus 4.8 empfohlen** (Integration, Zustandsmaschine, viele
Tests inkl. kreuzende Ziele). Skala: siehe `CLAUDE.md` §2.

Erst Erklärung → Rückfragen/Go → dann kleine, testbare Umsetzung.

## 4. M2-Plan in Häppchen (mit Komplexität / Modell)

- [x] **2.1** Converted Measurement (Plot → kartesisch + Kovarianz) — *S3 · Sonnet*
- [x] **2.2** Kalman-Filter (Constant-Velocity, Predict/Update) — *S4 · Opus*
- [x] **2.3** Gating (Mahalanobis-/χ²-Validierungsregion) — *S3 · Sonnet*
- [x] **2.4** Datenassoziation GNN (Ungarische Methode) — *S4 · Opus*
- [ ] **2.5** Track-Lebenszyklus (M-aus-N, Bestätigung, Coasting, Löschung) — *S4 · Opus*
- [ ] **2.6** Tracker als reine, deterministische Funktion + serialisierbarer Zustand — *S3 · Sonnet*
- [ ] **2.7** Güte-Metriken gegen Ground Truth (RMSE, Track-Kontinuität) — *S3 · Sonnet*

Jeder Haken wird erst gesetzt, wenn die Qualitäts-Gates (CLAUDE.md §5) erfüllt
sind und die Anforderung im Register rückverfolgbar steht.

### Komplexität künftiger Meilensteine (grobe Orientierung)

- **M1.5** ASTERIX CAT048-Codec — *S3 · Sonnet* (viel Code, aber mechanisch).
- **M3** WebSocket-Server/Cloud-Anbindung — *S4 · Opus*; Map-Frontend (JS) — *S3 · Sonnet*.
- **M4** Multi-Radar-Fusion + SSR/ADS-B-Korrelation — *S5 · Opus*.
- **M5** IMM / JPDA — *S5 · Opus*.
- Reine Doku-/Nachbereitungs-Schritte — *S1–S2 · Haiku*.

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
