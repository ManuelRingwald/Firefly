# Roadmap — Arbeitspakete & offene Punkte (Firefly + Wayfinder)

> **Zweck:** Lebende Übersicht über das Backlog beider Projekte mit
> Aufwandseinschätzung (Komplexitätsstufe S1–S5, siehe `CLAUDE.md` Abschnitt 2)
> und empfohlener Modell-Zuordnung. Wird aktualisiert, sobald sich aus der
> Arbeit neue Findings/Pakete ergeben. **Stichwort „Roadmap" im Chat zeigt
> diese Liste.**
>
> Stand: 2026-06-15 (Pakete #1 Multicast-Feed-Sicherheit, #2
> Observability-Grundgerüst und #3 CAT065-Heartbeat abgeschlossen; Pakete
> #10–#20 aus den Backlogs „Firefly SDPS Core Features" und „Wayfinder"
> übernommen).

## Empfohlene Reihenfolge

| # | Paket | Repo(s) | Inhalt | Stufe/Modell |
|---|-------|---------|--------|--------------|
| 1 | **Multicast-Feed-Sicherheit** ✅ *inhaltlich abgeschlossen* | Firefly + Wayfinder | Netz-Isolation/Authentizität des CAT062-Eingangspfads dokumentieren + ggf. absichern (ADR), Wayfinder-Browser-Rand (TLS/Auth, ehem. Issue #7). **Häppchen 1.1 ✅ erledigt** (Firefly ADR 0017, NFR-SEC-001); **Häppchen 1.2 ✅ erledigt** (Wayfinder ADR 0003, Empfangspfad-Pendant + Browser-Rand-Entscheidung: TLS/Auth primär am Reverse-Proxy, fail-closed Origin-Check/Token/TLS in Wayfinder, NFR-SEC-001); **Häppchen 1.3 ✅ erledigt** (Browser-Rand-Implementierung: strikter Origin-Check, optionale Token-Middleware, optionales TLS in `pkg/ws/handler.go`/`cmd/wayfinder/main.go`, Tests, NFR-SEC-001 vollständig); **1.4** optional Sender-Härtung Firefly (offen, unabhängiges Nice-to-have) | **S4 · Opus 4.8** |
| 2 | **Observability-Grundgerüst** ✅ *abgeschlossen* | Firefly + Wayfinder | **Häppchen 2.1 ✅ erledigt** (Wayfinder: totes `internal/config` entfernt, `WAYFINDER_LOG_LEVEL` konfigurierbar, Client-Eviction im Broadcaster geloggt, NFR-OBS-001); **Häppchen 2.2 ✅ erledigt** (Firefly: `tracing` in `firefly-multicast` eingezogen — Sender `lib.rs::run` mit `debug!`/`error!` pro Scan, Empfänger `receiver.rs::run` mit `debug!`/`warn!` pro Block; `firefly-asterix` unverändert, NFR-OBS-001); **Häppchen 2.3 ✅ erledigt** (gemeinsamer `/metrics`-Endpoint, Prometheus-Textformat: Wayfinder `pkg/metrics` auf Port `:8080` — Block-/Track-Zahlen, CAT062-Decode-Fehler, aktuelle Track-Zahl, WS-Client-Count/Evictions, NFR-OBS-002; Firefly `firefly-server::metrics` auf `/metrics` — Szene-Frame-Zahl, WS-Client-Count/Total, CAT062-Multicast-Scans/Sendefehler, NFR-OBS-001) | **S3 · Sonnet 4.6** |
| 3 | **AP5/AP6 — CAT065 Heartbeat** ✅ *abgeschlossen* | Firefly (Encoder) + Wayfinder (Decoder) | SDPS-Service-Status (Feed-Health) — unterscheidet „leerer Himmel" von „totem Feed". **Firefly:** `firefly-asterix::cat065` (Encoder+Decoder, byte-genau), `firefly-multicast::run_heartbeat` (wall-clock, gleiche Gruppe wie CAT062, ADR 0018), Metrik `firefly_cat065_heartbeats_sent_total`; ICD → 2.3.0 (additiv, §8), FR-IO-006/FR-NET-003. **Wayfinder:** `pkg/cat065`-Decoder, Receiver-Dispatch am CAT-Oktett, `pkg/health`-Staleness-Tracker, Feed-Banner im Frontend, `/ready`-Integration, Metriken `wayfinder_cat065_heartbeats_received_total`/`wayfinder_feed_stale`; FR-DATA-004/FR-OPS-004/NFR-OBS-003. | **S4 · Opus 4.8** |
| 4 | **Konfigurierbarer System-Referenzpunkt** | Firefly | I062/100-Referenzpunkt jenseits Demo-Ursprung Frankfurt, ADR-0006-Folgeentscheidung | **S3 · Sonnet 4.6** |
| 5 | **Out-of-Order-Eingang (Robustheit)** | Firefly | Tracker-Härtung gegen verspätete/umsortierte Plots | **S3 · Sonnet 4.6** |
| 6 | **Coverage-Werkzeug** | Firefly | Visualisierung Sensor-Abdeckung | **S3 · Sonnet 4.6** |
| 7 | **FHA / Hazard-Analyse** | Firefly + Wayfinder | Sicherheits-Analyse-Dokument | **S4 · Opus 4.8** |
| 8 | **Sensor-Registrierung/Bias-Korrektur** | Firefly | M4-Nachtrag, größere Mess-Fusions-Erweiterung | **S5 · Fable 5 / Opus 4.8** |
| 9 | **Live-OpenAIP-Integration** | Firefly | Statische Airspace-GeoJSON → Live-API | **S3 · Sonnet 4.6** |
| 10 | **SDPS-005 — Legal Recording & Replay** | Firefly | Sidecar zeichnet rohe Sensor-Multicast-Payloads mit Empfangs-Zeitstempel auf; dank Determinismus nach Datenzeit bit-genaue Rekonstruktion möglich | **S2 · Sonnet 4.6** |
| 11 | **SDPS-006 — Erweiterte Observability** | Firefly | Prometheus-Exporter (Plots/s, Track-Count, Latenzen) + Grafana-Dashboard als Code, baut auf Paket #2 auf | **S2 · Sonnet 4.6** |
| 12 | **ASD-001 — Erweiterter Data Block** | Wayfinder | Callsign (I062/245), Flight Level (I062/136, FLnnn), Ground Speed (aus Vx/Vy), Steig-/Sinkflug-Indikator im Track-Label | **S3 · Sonnet 4.6** |
| 13 | **ASD-003 — Aeronautical Map Layer** | Wayfinder | "Radar Dark Mode"-Basistheme, Luftraumstrukturen (Sektoren/FIR), Waypoints/VOR/NDB als Layer | **S3 · Sonnet 4.6** |
| 14 | **ASD-004 — Track-Lebenszyklus & History-Darstellung** | Wayfinder | Konfigurierbare History-Dots, Coasting-Blinken/Abdunkeln, Graceful Fade-Out bei TSE (ADR 0016) | **S3 · Sonnet 4.6** |
| 15 | **ASD-005 — Höhen- und Filter-Tools** | Wayfinder | UI-Panel für Min/Max-FL-Filter, Tracks außerhalb ausblenden/entsättigen | **S2 · Sonnet 4.6** |
| 16 | **ASD-002 — Anti-Garbling (Label-Vermeidung)** | Wayfinder | Algorithmus zur automatischen Label-Umpositionierung bei Überlappung (Leader Line); optional Drag&Drop | **S4 · Opus 4.8** |
| 17 | **SDPS-003 — Environment & Meteo Data Service** | Firefly | Zyklisches QNH für barometrische Höhenkorrektur (I062/136), statische DTM-Daten als Basis für Bodenannäherungswarnungen | **S3 · Sonnet 4.6** |
| 18 | **SDPS-004 + ASD-006 — STCA (gekoppeltes Paar)** | Firefly + Wayfinder | **Firefly:** serverseitige Konflikterkennung im Tracker (Vorausschau, Staffelungsminima), setzt Alarm-Flag in CAT062 (I062/340), ICD-Bump. **Wayfinder:** ASD-006 reformuliert als reiner Flag-Konsum — Data Block blinkt rot, keine eigene Geometrie-Berechnung (kein doppelter Determinismus-Pfad). Abhängigkeit: Wayfinder-Teil erst nach Firefly-ICD-Update | **S4 · Opus 4.8** |
| 19 | **SDPS-001 — FEP Sensor Ingestion** | Firefly | UDP-Receiver für ASTERIX CAT048/CAT001, dynamische Sensor-Konfiguration, Koordinatentransformation Polar→kartesisch; löst Simulator als Eingangsquelle ab | **S5 · Fable 5 / Opus 4.8** |
| 20 | **SDPS-002 — High Availability & State Sync** | Firefly | Main/Standby-Architektur (Leader Election), schnelle Sync des Tracker-States (Kalman-Matrizen, Assoziationen), drop-out-freier Standby-Übernahme im CAT062-Feed | **S5 · Fable 5 / Opus 4.8** |

**Begründung der Reihenfolge:** Sicherheit (1) zuerst, da ASD sicherheitsrelevant
und bisher nur als ADR-Lücke dokumentiert. Observability (2) direkt danach —
klein, gut umrissen, schließt die im Logging-Audit (2026-06-15) gefundenen
Lücken und macht alle Folgepakete (inkl. Heartbeat) beobachtbar. CAT065
Heartbeat (3) baut darauf auf. Danach die unabhängigen S3-Pakete (4–6) je nach
operativer Priorität. FHA (7) und die großen S5-Themen (8–9) zuletzt, auf
stabilisierter Basis.

## Begründung Pakete #10–#20 (Backlog-Übernahme, 2026-06-15)

Reihenfolge: zuerst die kleinen, unabhängigen Pakete (#10–#15) für schnelle
Wertschöpfung bei geringem Risiko; danach #16 (Anti-Garbling, algorithmisch
anspruchsvoll) und #17 (Meteo, klar umrissen); dann das gekoppelte STCA-Paar
(#18); die beiden großen S5-Architektur-Themen FEP-Ingestion (#19) und
HA/State-Sync (#20) zuletzt, jeweils mit eigenem ADR vor Umsetzung.

**Entscheidung SDPS-004/ASD-006:** ASD-006 wird **nicht** als unabhängige,
Wayfinder-seitige STCA-Berechnung umgesetzt, sondern als Konsument des von
Firefly im CAT062-Strom gesetzten Alarm-Flags (I062/340). Das vermeidet einen
zweiten, potenziell abweichenden Determinismus-Pfad und hält die
Konflikterkennung dort, wo sie nach SDPS-004 ohnehin berechnet wird. Das
CAT062-ICD-Update (neues Item/Bit) wird im Rahmen von Paket #18 angekündigt,
abgestimmt und versioniert.

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

- ✅ Paket #3 / AP5/AP6 — CAT065 SDPS-Heartbeat, ICD 2.3.0 (ADR 0018; Firefly Sender + Wayfinder Decoder/Staleness)
- ✅ Paket #2 — Observability-Grundgerüst (Log-Level, `tracing` in firefly-multicast, `/metrics` beidseitig)
- ✅ Paket #1 — Multicast-Feed-Sicherheit (Firefly ADR 0017, Wayfinder ADR 0003, Browser-Rand)
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
