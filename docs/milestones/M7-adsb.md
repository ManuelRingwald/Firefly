# M7 — ADS-B-Integration (AP9)

> **Stand:** AP9.1–AP9.6 implementiert und in Branch
> `claude/beautiful-dijkstra-e7ityj` (Firefly) verfügbar.
> AP9.4 (opensky-Crate), AP9.7 (weitere Tests), AP9.8 (Doku)
> und AP9.9 (Wayfinder-Badge) stehen noch aus.

---

## 1. Fachliche Motivation

Firefly verarbeitet bisher ausschließlich simulierte Radar-Plots (PSR/SSR). In
der Realität betreiben Flugzeuge **Mode-S-Transponder**, die sich selbst in 1–2-s-
Intervallen mit ihrer GPS-Position, Geschwindigkeit und ihrer weltweit eindeutigen
**ICAO-24-Bit-Adresse** melden — unabhängig vom Radar. Diese Selbstberichte
(*ADS-B, Automatic Dependent Surveillance–Broadcast*) sind präziser als Radar
(Genauigkeitsklasse NACp 8–10, < 75 m) und liefern ohne Scan-Lücken.

Für den Tracker bedeutet ADS-B:

| Eigenschaft | Radar | ADS-B |
|-------------|-------|-------|
| Aktualisierungsrate | 4–12 s (Antennenumlauf) | 1–2 s |
| Positions-σ | 200–500 m (PSR) | < 75 m (NACp ≥ 8) |
| Identität | Mode 3/A (Squawk, nicht eindeutig) | ICAO-24-Bit (fahrzeugindividuell) |
| Quelle | Bodenanlage | Bordtransponder |

Die Fusion beider Quellen verbessert die Track-Stabilität und eröffnet die
ICAO-basierte Identitäts-Vorsortierung (kein kinematisches Gating mehr nötig,
wenn die Identität gesichert ist).

---

## 2. Architektur-Überblick

```
OpenSky REST API
   │  HTTP poll (~10 s, Bounding Box)
   ▼
firefly-opensky (AP9.4, in Arbeit)
   ┌─────────────────────────────────────┐
   │ OpenSkyPoller                       │
   │  WGS84 → Plot (Measurement::Geodetic│
   │  + ICAO-Adresse in ModeAC)          │
   └─────────────────────────────────────┘
   │  Plot (Geodetic + ICAO)
   ▼
firefly-track (AP9.1–AP9.3)
   ┌────────────────────────────────────────┐
   │ Tracker::fuse_simultaneous_plots       │
   │  1. ICAO-Vorsortierung (pre-JPDA)      │
   │  2. JPDA für verbleibende Plots        │
   │  3. tracking_measurement dispatcht auf │
   │     Geodetic-Pfad (WGS84→ENU, iso. R)  │
   └────────────────────────────────────────┘
   │  SystemTrack (+ adsb_age_s, AP9.6)
   ▼
firefly-asterix (AP9.5)
   ┌─────────────────────────────────────┐
   │ I062/290 ES-Age-Subfeld (ICD 2.4.0) │
   └─────────────────────────────────────┘
   │  CAT062 UDP Multicast
   ▼
Wayfinder (AP9.9 — ADS-B-Badge)
```

---

## 3. Implementierte Teile (AP9.1–AP9.6)

### AP9.1 — `Measurement::Geodetic` Enum

`firefly-core::Measurement` ist ein Enum:
```rust
pub enum Measurement {
    Polar(Polar),
    Geodetic { position: Wgs84, sigma_pos_m: f64 },
}
```
`Plot::adsb(position, sigma_pos_m, icao, callsign, time)` erzeugt einen
ADS-B-Plot mit `Measurement::Geodetic`.

### AP9.2 — `tracking_measurement` Dispatcher

`firefly-track::measurement::tracking_measurement` dispatcht auf die Variante:
- **Polar:** alter Pfad (`convert_plot` + `horizontal_from`).
- **Geodetic:** `LocalFrame::geodetic_to_enu(position)` → `z = [east, north]`,
  isotrope Kovarianz `R = σ² · I₂` (kein Radar-Geometrie-Term).

### AP9.3 — ICAO-Adress-Vorsortierung

Vor dem JPDA-Gate: Ist die ICAO-Adresse eines Plots identisch mit der eines
lebenden Tracks, wird der Plot **direkt** diesem Track zugeordnet (β=1). Nur
Plots ohne Treffer gehen in den JPDA-Pool.

Wichtig: Die gefrorene Referenz (ADR 0011, Ghost-Suppression) wird **vor** der
Vorsortierung erstellt.

### AP9.6 — ADS-B-Trefferzeit propagieren

- `Track.adsb_last_hit_time: Option<f64>` — gesetzt im ICAO-Pre-Sort-Trefferpfad.
- `SystemTrack.adsb_age_s: Option<f64>` — `(time - hit).max(0.0)` in
  `system_track_from`.

### AP9.5 — I062/290 ES-Age-Subfeld (ICD 2.4.0)

Wenn `SystemTrack.adsb_age_s` `Some` ist:
- Bit `0x08` in das primäre Subfeld-Oktett von I062/290 setzen.
- ES-Age-Byte anhängen (gleiche 1/4-s-Kodierung wie PSR-Age).

Das Wire-Format für Tracks ohne ADS-B ist unverändert (additiv).

---

## 4. NACp → Kovarianz (OpenSky)

| `position_source` | Bedeutung | σ_pos [m] |
|------------------|-----------|-----------|
| 0 | ADS-B (NACp ≥ 8 typisch) | 75 |
| 1 | ASTERIX | 200 |
| 2 | MLAT | 200 |
| Default | unbekannt | 300 |

---

## 5. Sicherheitshinweis (Spoofing)

ICAO-Adressen sind **nicht kryptografisch authentifiziert**. Die Vorsortierung
(AP9.3) vertraut der ICAO-Identität. ADR 0017 (Netz-Isolation des
Multicast-Pfads) ist die primäre Schutzmaßnahme.

Für operative Systeme empfiehlt sich eine Kreuzvalidierung: Ist die
ADS-B-Position kinematisch konsistent mit dem bisherigen Track-Verlauf? Eine
solche Plausibilitätsprüfung ist für ADR 0019 als optionale Erweiterung notiert.

---

## 6. ICD-Auswirkung (Wayfinder)

**ICD 2.4.0** (additiv, kein Breaking Change):

- I062/290 ist variabel lang: Ohne ADS-B-Treffer 2 Byte (wie bisher), mit
  ES-Age-Subfeld 3 Byte.
- Wayfinder muss I062/290 robust als variabel langes Item dekodieren.
- Präsenz von ES-Age → ADS-B-Badge im Track-Label.

Cross-Project-Issue: Wayfinder#[wird erstellt in AP9.8].

---

## 7. Noch ausstehend (AP9.4, AP9.7, AP9.9)

| Arbeitspaket | Inhalt | Status |
|-------------|--------|--------|
| AP9.4 | `firefly-opensky` Crate: HTTP-Poller, JSON-Deserialisierung, NACp-Mapping | offen |
| AP9.7 | Zusätzliche Tests (ICD 2.4.0 Referenz-Dump-Update) | offen |
| AP9.9 | Wayfinder: ES-Age-Decoder + ADS-B-Badge im Track-Label | offen |
