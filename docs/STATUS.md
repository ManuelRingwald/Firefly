# Arbeitsstand (Handover-Notiz)

> **Zweck:** Diese Datei ist der schnelle Wiedereinstieg — egal ob am PC oder
> Handy. Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

- **Zuletzt aktualisiert:** 2026-06-10
- **Branch:** `claude/radar-track-calculator-BoaU8`
- **Letzter Commit:** Härtungs-Häppchen — Timing-Robustheit (NFR-CLOUD-004),
  `tests/timing.rs`. **M2 inkl. Veredelungen komplett.**
- **PR:** #1 (offen).

---

## 1. Wo wir gerade stehen

- **M1 (Simulator) ist fertig** und gepusht: Workspace + drei Crates
  (`firefly-geo`, `firefly-core`, `firefly-sim`).
- **M2 läuft:** Häppchen **2.1–2.8 erledigt (M2 abgeschlossen)** — Crate `firefly-track` mit
  Converted-Measurement, Kalman-Filter (CV, Joseph-Form), Gating (Mahalanobis/χ²),
  Datenassoziation (GNN/Ungarische Methode) und **Track-Lebenszyklus** (`Tracker`,
  Pro-Scan-Orchestrierung: Geburt/Bestätigung/Coasting/Löschung). Der
  Single-Radar-Tracker steht — inkl. End-to-End-Test mit zwei kreuzenden Zielen.
  **2.6**: serialisierbarer Zustand mit Snapshot/Replay (serde, ADR 0007).
  **2.7**: neutraler `SystemTrack`-Output in WGS84 (`firefly-core`) + Projektion
  `Tracker::system_tracks(&LocalFrame)` — der ASD-Port Richtung CAT062.
  **2.8**: Güte-Metriken (`Rmse`, `TrackContinuity`) gegen Ground Truth; E2E-Test
  mit Positions-RMSE < 40 m, 0 ID-Wechsel, Coverage > 90 %.
  **Nachtrag (ADR 0008, FR-TRK-008):** der `SystemTrack` trägt jetzt den
  safety-relevanten Status — `coasting`, `update_age`, `position_uncertainty`
  (1σ-Halbachse aus `P`) → bereitet CAT062 I062/080, /290, /500 vor.
  **Härtung (NFR-CLOUD-004):** `tests/timing.rs` beweist — lange Scan-Lücke mit
  Daten erhält Identität; Löschung nach Fehltreffer-*Anzahl*, nicht nach Zeit.
  Externe Abhängigkeiten `nalgebra` (ADR 0005), `serde` (ADR 0007).
- Qualität: **69 Tests + 1 Doctest grün**, Clippy sauber, `cargo fmt` ok.
- Die **Arbeitsregeln** stehen (`CLAUDE.md`): *erst erklären, dann bauen*;
  keine unerklärten Begriffe; Doku ist Teil der Leistung.
- **Dokumentation** aufgebaut: Glossar, M1-/M2-Erklärungen, ADRs 0001–0008,
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
| Integration | Andocken an **Phoenix ASD**; Ausgabe **ASTERIX CAT062**; Kern neutral via Ports & Adapters | ADR 0006 |

## 3. Nächster Schritt (hier geht es weiter!)

✅ **M2 ist abgeschlossen** (inkl. Nachtrag: safety-relevanter `SystemTrack`-Status,
ADR 0008). Der Single-Radar-Tracker steht vollständig: Messung → Filter → Gate →
Zuordnung → Lebenszyklus → Snapshot/Replay → neutraler WGS84-Output mit
Safety-Status → Güte-Metriken.

✅ **Timing-Härtung (NFR-CLOUD-004) erledigt** — `tests/timing.rs` beweist beide
Eigenschaften. Damit ist M2 inkl. aller Veredelungen abgeschlossen.

➡️ **Als Nächstes: Start von M3** — Web-Frontend mit Live-2D-Karte über WebSocket;
hier wird auch die Ein-Befehl-Demo (NFR-OPS-001) konkret, samt einer Szene, die die
Timing-Robustheit *sichtbar* macht. M3 ist groß — Claude wird es **in Häppchen
zerlegen und vor dem Bau erklären** (WebSocket-Server S4 · Opus/Fable 5,
Map-Frontend S3 · Sonnet, CAT062-Encoder S4 — siehe §4). Erste offene Frage für M3:
Frontend-Kartenbibliothek (Leaflet vs. MapLibre, ADR fällig).

Erst Erklärung → Rückfragen/Go → dann kleine, testbare Umsetzung.

## 4. M2-Plan in Häppchen (mit Komplexität / Modell)

- [x] **2.1** Converted Measurement (Plot → kartesisch + Kovarianz) — *S3 · Sonnet*
- [x] **2.2** Kalman-Filter (Constant-Velocity, Predict/Update) — *S4 · Opus*
- [x] **2.3** Gating (Mahalanobis-/χ²-Validierungsregion) — *S3 · Sonnet*
- [x] **2.4** Datenassoziation GNN (Ungarische Methode) — *S4 · Opus*
- [x] **2.5** Track-Lebenszyklus (M-aus-N, Bestätigung, Coasting, Löschung) — *S4 · Opus*
- [x] **2.6** Serialisierbarer Zustand (Snapshot/Replay) — *S3 · Sonnet · Effort mittel*
- [x] **2.7** Neutraler `SystemTrack`-Output in WGS84 (ASD-Port → CAT062) — *S3 · Sonnet · Effort mittel*
- [x] **2.8** Güte-Metriken gegen Ground Truth (RMSE, Track-Kontinuität) — *S3 · Sonnet · Effort mittel*
- [x] **Nachtrag** Safety-Status auf `SystemTrack` (ADR 0008, FR-TRK-008) — *S3 · Sonnet · Effort mittel*
- [x] **Härtung** Timing-Robustheit (NFR-CLOUD-004) — *S3 · Sonnet · Effort mittel*

Jeder Haken wird erst gesetzt, wenn die Qualitäts-Gates (CLAUDE.md §5) erfüllt
sind und die Anforderung im Register rückverfolgbar steht.

### Komplexität künftiger Meilensteine (grobe Orientierung, inkl. Effort)

- **M1.5** ASTERIX CAT048-Codec — *S3 · Sonnet · Effort hoch* (viel Code, aber
  bit-genau und fehleranfällig).
- **M3** WebSocket-Server/Cloud-Anbindung — *S4 · Opus 4.8 / Fable 5 · Effort hoch*;
  Map-Frontend (JS) — *S3 · Sonnet · Effort mittel*; CAT062-Encoder + Transport-
  Adapter — *S4 · Opus 4.8 / Fable 5 · Effort hoch*.
- **M4** Multi-Radar-Fusion + SSR/ADS-B-Korrelation — *S5 · Fable 5 / Opus 4.8 · Effort hoch–max*.
- **M5** IMM / JPDA — *S5 · Fable 5 / Opus 4.8 · Effort max*.
- Reine Doku-/Nachbereitungs-Schritte — *S1–S2 · Haiku · Effort niedrig*.

## 5. Offene Punkte / später entscheiden

- **ASD-Integration (ADR 0006):** Transport (UDP-Multicast / Bus / WebSocket)
  und Koordinatenbezug (WGS84 vs. System-Stereografisch) noch offen. **Design-
  Hinweis fürs nächste Häppchen:** Der `Tracker` sollte die geodätische
  Frame-Referenz des Sensors mitführen, damit Tracks später nach **WGS84**
  ausgegeben werden können (neutraler `SystemTrack` → CAT062-Adapter).
- **Message-Bus-Technologie** (z. B. NATS/Kafka) — erst relevant ab M3, dann ADR.
- **Coverage-Werkzeug** (z. B. `cargo llvm-cov`) — einführen, sobald V&V-Nachweise
  greifbar werden.
- **Sicherheitsanalyse (FHA/Hazards)** — sinnvoll, sobald Tracker-Funktionen
  stehen, gegen die man Gefährdungen bewerten kann.
- **Frontend-Kartenbibliothek** (Leaflet vs. MapLibre) — Entscheidung in M3.
- **Out-of-order-Daten (Eingangs-Adapter, M3/M4):** Wenn ein *sehr alter* Plot
  *nach* neueren ankommt, kann man nicht sinnvoll rückwärts vorhersagen. Standard:
  am Eingang nach Datenzeit ordnen, kleines Zeitfenster puffern, zu Spätes
  *verwerfen* (nur den Plot, nicht den Track). Bewusst **kein** „Daten alt → Reset".
- **Frische-/Staleness-Anzeige (Ausgabe-Rand, M3):** Aus `SystemTrack.update_age`
  am Anzeige-Rand eine *weiche* Frische-Markierung ableiten — nie zustands-
  zerstörend (ADR 0008). Die Entscheidung selbst liegt schon im Tracker.
- **Vorführbarkeit (NFR-OPS-001):** Ein-Befehl-Demo ohne Programmierkenntnisse
  für Präsentationen — Umsetzung mit dem Frontend in M3.

## 6. So steige ich wieder ein (Kurzbefehle)

```bash
cargo test --workspace                     # alles grün?
cargo run --example demo -p firefly-sim    # M1-Simulator live sehen
```

Doku-Einstieg: `docs/README.md` → Glossar, Meilensteine, ADRs, Requirements.
