# Roadmap — Arbeitspakete & offene Punkte (Firefly + Wayfinder)

> **Zweck:** Lebende Übersicht über das Backlog beider Projekte mit
> Aufwandseinschätzung (Komplexitätsstufe S1–S5, siehe `CLAUDE.md` Abschnitt 2)
> und empfohlener Modell-Zuordnung. Wird aktualisiert, sobald sich aus der
> Arbeit neue Findings/Pakete ergeben. **Stichwort „Roadmap" im Chat zeigt
> diese Liste.**
>
> Stand: 2026-06-15 (nach Merge von PR #16 (Firefly) / #8 (Wayfinder), TSE/ADR
> 0016 abgeschlossen).

## Empfohlene Reihenfolge

| # | Paket | Repo(s) | Inhalt | Stufe/Modell |
|---|-------|---------|--------|--------------|
| 1 | **Multicast-Feed-Sicherheit** ⏳ *in Arbeit* | Firefly + Wayfinder | Netz-Isolation/Authentizität des CAT062-Eingangspfads dokumentieren + ggf. absichern (ADR), Wayfinder-Browser-Rand (TLS/Auth, ehem. Issue #7). **Häppchen 1.1 ✅ erledigt** (Firefly ADR 0017, NFR-SEC-001); **Häppchen 1.2 ✅ erledigt** (Wayfinder ADR 0003, Empfangspfad-Pendant + Browser-Rand-Entscheidung: TLS/Auth primär am Reverse-Proxy, fail-closed Origin-Check/Token/TLS in Wayfinder, NFR-SEC-001); **1.3** Browser-Rand-Implementierung (Origin-Check, Token-Middleware, optionales TLS in `pkg/ws`/`cmd/wayfinder`, offen, **S4 · Opus 4.8**); **1.4** optional Sender-Härtung Firefly (offen) | **S4 · Opus 4.8** |
| 2 | **Observability-Grundgerüst** | Firefly + Wayfinder | Wayfinder: `internal/config` tot-Code entfernen oder anschließen, Log-Level konfigurierbar machen, Eviction-Logging. Firefly: `tracing` in `firefly-multicast`/`firefly-asterix` einziehen (Decode-Fehler, Socket-Fehler). Danach gemeinsam: `/metrics`-Endpoint (Prometheus) für Track-Zahlen, Decode-Fehler, Drops, Client-Counts | **S3 · Sonnet 4.6** |
| 3 | **AP5/AP6 — CAT065 Heartbeat** | Firefly (Encoder) + Wayfinder (Decoder) | Service-Status-Reports (Feed-Health) — wichtig für Readiness/Staleness-Erkennung, ergänzt Observability | **S3–S4 · Sonnet/Opus** |
| 4 | **Konfigurierbarer System-Referenzpunkt** | Firefly | I062/100-Referenzpunkt jenseits Demo-Ursprung Frankfurt, ADR-0006-Folgeentscheidung | **S3 · Sonnet 4.6** |
| 5 | **Out-of-Order-Eingang (Robustheit)** | Firefly | Tracker-Härtung gegen verspätete/umsortierte Plots | **S3 · Sonnet 4.6** |
| 6 | **Coverage-Werkzeug** | Firefly | Visualisierung Sensor-Abdeckung | **S3 · Sonnet 4.6** |
| 7 | **FHA / Hazard-Analyse** | Firefly + Wayfinder | Sicherheits-Analyse-Dokument | **S4 · Opus 4.8** |
| 8 | **Sensor-Registrierung/Bias-Korrektur** | Firefly | M4-Nachtrag, größere Mess-Fusions-Erweiterung | **S5 · Fable 5 / Opus 4.8** |
| 9 | **Live-OpenAIP-Integration** | Firefly | Statische Airspace-GeoJSON → Live-API | **S3 · Sonnet 4.6** |

**Begründung der Reihenfolge:** Sicherheit (1) zuerst, da ASD sicherheitsrelevant
und bisher nur als ADR-Lücke dokumentiert. Observability (2) direkt danach —
klein, gut umrissen, schließt die im Logging-Audit (2026-06-15) gefundenen
Lücken und macht alle Folgepakete (inkl. Heartbeat) beobachtbar. CAT065
Heartbeat (3) baut darauf auf. Danach die unabhängigen S3-Pakete (4–6) je nach
operativer Priorität. FHA (7) und die großen S5-Themen (8–9) zuletzt, auf
stabilisierter Basis.

## Findings (Logging/Observability-Audit, 2026-06-15)

**Firefly:**
- `tracing` + `tracing-subscriber` nur in `firefly-server` (Log-Level via
  `RUST_LOG`, Default `info`, reines Text-Format, kein JSON).
- `firefly-multicast` und `firefly-asterix` haben **kein Logging** —
  UDP-Send/Receive-Fehler und CAT062-Decode-Fehler/verworfene Records sind
  unsichtbar.
- Keine Metriken (kein Prometheus/`/metrics`), kein OpenTelemetry/Tracing,
  kein `#[instrument]`.

**Wayfinder:**
- `slog` mit JSON-Handler durchgängig verdrahtet (Receiver, Broadcaster,
  WS-Handler) — Decode-Fehler werden bereits mit Kontext geloggt.
- Log-Level ist hartkodiert (`LevelInfo`); `internal/config`
  (`WAYFINDER_LOG_FORMAT`) ist **toter Code** — `main.go` nutzt eine eigene,
  parallele Config.
- Keine Metriken, kein Tracing. Client-Eviction bei vollem Channel
  (`broadcast.go:179`) wird nicht geloggt.

## Erledigt (Referenz)

- ✅ AP7/AP8 — CAT062 I062/245 Callsign, ICD 2.1.0 (PR #15 Firefly / #7 Wayfinder)
- ✅ ADR 0016/TSE — CAT062 I062/080 Track-Ende, ICD 2.2.0 (PR #16 Firefly / #8 Wayfinder)
- ✅ AP1/AP2 — CAT062 I062/136 Vertikallage + UAP-Standardtreue, ICD 2.0.0 (ADR 0015)
- ✅ ADR 0013 — asynchrone Pro-Plot-Verarbeitung + periodischer Ausgabetakt (13.1–13.7)

## Pflege-Hinweis

Neue Findings (z. B. aus Doku-Checks, Audits, Cross-Project-Issues) werden hier
ergänzt — neue Zeile in der Tabelle oder neuer Abschnitt. Erledigte Pakete
wandern nach „Erledigt". Diese Datei existiert identisch in beiden Repos
(`Firefly/docs/ROADMAP.md` und `Wayfinder/docs/ROADMAP.md`), damit sie aus
beiden Sitzungen heraus abrufbar ist.
