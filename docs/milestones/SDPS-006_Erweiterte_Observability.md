# SDPS-006 — Erweiterte Observability

Paket **SDPS-006** aus `docs/ROADMAP.md` (#11). Baut auf Paket #2
(Observability-Grundgerüst, `/metrics`-Endpoint) auf.

## Fachlich

Der Betreiber braucht neben den Feed-Metriken (CAT062-Scans, CAT065-Heartbeats)
die laufende **Track-Zahl** als primäres Vitalzeichen des Trackers: Ein Einbruch
von z. B. 12 auf 0 Tracks signalisiert einen Tracker- oder Feed-Ausfall — anders
als ein leerer Himmel, der durch den CAT065-Heartbeat unterschieden wird.

Ein **Grafana-Dashboard** macht alle Firefly-Metriken ohne manuelle
Prometheus-Queries sofort sichtbar und importierbar.

## Technisch (SDPS-006a — `firefly_tracks_active` Gauge)

### Callback-Erweiterung in `firefly_multicast::run`

`run()` wurde von einer festen Signatur auf eine generische erweitert:

```rust
pub async fn run<F: Fn(usize)>(
    socket: &UdpSocket,
    destination: SocketAddr,
    encoder: &Cat062Encoder,
    scans: &[(Timestamp, Vec<SystemTrack>)],
    speed: f64,
    on_scan: F,       // called with tracks.len() after each successful send
) -> std::io::Result<usize>
```

`on_scan` wird **nach** jedem erfolgreichen UDP-Send aufgerufen. Das hält die
Funktion von `Arc<Metrics>` entkoppelt — sie bleibt format-neutral und testbar
ohne Server-Abhängigkeit.

### Metrik-Feld in `firefly_server::Metrics`

```rust
pub tracks_active: AtomicU64,
```

`render()` exponiert `firefly_tracks_active` als Prometheus-Gauge.

### Verdrahtung in `main.rs`

```rust
let metrics_scan = Arc::clone(&metrics);
firefly_multicast::run(&socket, destination, &encoder, &scans, speed, move |n| {
    metrics_scan.tracks_active.store(n as u64, Ordering::Relaxed);
})
```

### Hinweis zu „Plots/s"

In der aktuellen deterministischen Replay-Architektur (der `Player` berechnet
alle Scans offline beim Programmstart) existiert kein Echtzeit-Plots-Strom. Eine
sinnvolle `plots/s`-Metrik entsteht erst mit **SDPS-001** (Live-FEP-Sensor-
Ingestion über CAT048/CAT001), wenn Plots laufend aus dem Netz ankommen.

## Technisch (SDPS-006b — Grafana-Dashboard)

Datei: `monitoring/grafana/dashboard.json`

Importierbar via Grafana-UI (Dashboards → Import → JSON hochladen oder einfügen).
Das Dashboard verwendet eine parametrisierte `${DS_PROMETHEUS}`-Variable für den
Datasource-Namen, sodass es ohne Anpassung in jede Grafana-Instanz passt.

### Panels

| Panel | Typ | Query |
|-------|-----|-------|
| Tracks Active | Stat (Ampel: rot < 1, gelb ≥ 1, grün ≥ 3) | `firefly_tracks_active` |
| WS Clients Connected | Stat (rot < 1, grün ≥ 1) | `firefly_ws_clients_connected` |
| CAT062 Send Errors | Stat (gesamt, grün = 0) | `firefly_cat062_send_errors_total` |
| Tracks Active Over Time | Time Series | `firefly_tracks_active` |
| CAT062 Scan Rate + CAT065 Heartbeat Rate | Time Series | `rate(...[1m])` beider Feeds |

Auto-Refresh: 10 s. Zeitfenster: letzte 30 Minuten.

## Tests / Verifikation

- `cargo test --workspace` grün (alle 233 Tests) ✅
- `cargo clippy --workspace --all-targets` sauber ✅
- `cargo fmt` sauber ✅
- `firefly-server::metrics::render_includes_all_metrics` prüft `firefly_tracks_active 7`
  und `# TYPE firefly_tracks_active gauge` ✅
- Multicast-Tests (`sender`, `receiver`) kompilieren und laufen mit no-op Callback `|_| {}` ✅
- Dashboard-JSON valide (Grafana-Schema 38) — manuell verifizierbar via Import ✅
