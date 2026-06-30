# Arbeitsstand (Handover-Notiz) — Firefly

> **Zweck:** Diese Datei beschreibt den **aktuellen IST-Stand** von Firefly.
> Sie wird am Ende jeder Arbeitssitzung aktualisiert und committet.
> Claude liest sie zu Sitzungsbeginn (siehe `CLAUDE.md`).

> 🗺️ **Roadmap & Arbeitspakete:** siehe `docs/ROADMAP.md` im **Wayfinder-Repo**
> (zentrale Quelle für beide Repos). Cross-Project-Abhängigkeiten in
> `docs/cross-project/todo-for-firefly.md`.

---

## 🎯 Stand 2026-06-30

- **Zuletzt aktualisiert:** 2026-06-30
- **Großes Bild:** Die **Firefly-Seite des Quell-Eingangs-Kontrakts (#35)** ist für
  **`adsb_opensky` *und* `flarm_aprs`** fertig — Kontrakt (ADR 0023, jetzt v1.2.0) +
  Live-Verdrahtung + OpenSky OAuth2 (ADR 0024) + **FLARM/OGN-Adapter (ADR 0026)**.
  Von #35 ist auf Firefly-Seite nur noch `radar_asterix` offen. Die Wayfinder-Auto-
  Orchestrierung ist drüben **komplett** (ORCH-1…5c + E2E-Harness); „Feed zuweisen ⇒
  Firefly-Instanz startet ⇒ Tracks im ASD" ist end-to-end fahrbar (realer Abnahme-
  Lauf steht beim Betreiber an). Alles auf `main`, alle Gates grün.

- **Letzte Arbeit (2026-06-30):** **ADR 0026 — FLARM/OGN-Eingangs-Adapter
  (`flarm_aprs` via APRS-IS).** Schritt A (ADR) · B (neues Crate `firefly-flarm`:
  `config`/`ogn`-Parser/`plot`/`aprsis`; robuster OGN-Parser ohne Panic, gegen echte
  Beispielzeilen + adversarisch geprüft; APRS-IS-Stream + Reconnect; ICAO-Adresse nur
  bei echtem ICAO-Adresstyp → bereitet #30 vor) · C (Verdrahtung: `sources.rs`
  `flarm_config_from_spec` + `ResolvedSources.flarm`, `build_live_tracker_multi`
  registriert **alle** Quell-Sensoren, `spawn_flarm_listener_live`, Metrik
  `firefly_flarm_plots_received_total`; Kontrakt **v1.2.0** additiv; Doku/Register
  FR-NET-012). 20 Crate-Tests + Server-Tests grün. Standalone via `FIREFLY_FLARM_*`,
  orchestriert via `flarm_aprs` in `FIREFLY_SOURCES`. PR #42.

- **Nächste Schritte (für die frische Session):**
  1. **Letzter Adapter aus #35 — `radar_asterix`** (ASTERIX-Eingang CAT048/CAT001
     eines realen Radars, SAC/SIC-identifiziert; SDPS-001 #19): eigener ADR +
     Meilenstein, Ports & Adapters. (`flarm_aprs` ✅ erledigt, ADR 0026.)
  2. **Offenes Issue #30** (`from-wayfinder`) — CAT062-ICD **v2.5.0** explizite
     Per-Track-Provenienz (`provenance`-Enum + `source_ages`), ersetzt die
     Frontend-Heuristik; additiv, byte-genaue Encoder-Vektoren liefern.
  3. **Betriebs-Härtung** (Roadmap-Block ⏳) — Observability-Ausbau, Lastfestigkeit,
     Deployment.

> 🗺️ Roadmap zentral im **Wayfinder-Repo** (`docs/ROADMAP.md`). Cross-Project:
> `docs/cross-project/todo-for-wayfinder.md`; offene `from-wayfinder`-Issues: #35
> (Reststand FLARM/Radar), #30 (Provenienz).

---

## ✅ Abgeschlossene Meilensteine

| Meilenstein | Inhalt | Status |
|---|---|---|
| **M1** | Simulator (ASTERIX-Szenarien, Track-Injection) | ✅ |
| **M2** | Single-Radar-Tracker (Kalman, Gate, JPDA, Lebenszyklus) | ✅ |
| **M3** | WebSocket-Server + JSON-Ausgabe (Live-Karte) | ✅ |
| **M4** | Multi-Radar-Fusion (Mess-Fusion, Sensormodell) | ✅ |
| **M5** | IMM/JPDA (Bewegungsmodelle, Assoziationen) | ✅ |
| **M6** | Showcase + Container (Deployment-ready) | ✅ |

---

## 📦 Produktions-Phase (laufend, ADR 0014)

### ✅ Fertig

| Feature | Status | Verweis |
|---|---|---|
| **UTC Time-of-Day** | ✅ I062/070 echte UTC-Tageszeit | Issue #9, geschlossen |
| **Multicast-Feed-Sicherheit** | ✅ ADR 0017 + WebSocket-Auth `/ws` | PR #27 |
| **System-Referenzpunkt** | ✅ I062/100 konfigurierbar via `FIREFLY_SYSTEM_REF_*` | ADR 0021 |
| **CAT062-ICD versioniert** | ✅ `docs/ICD-CAT062.md` v2.5.0 | Schnittstellen-Vertrag |
| **ADR 0013** | ✅ Asynchrone Pro-Plot + periodischer Ausgabetakt | 13.1–13.7 erledigt |
| **ADR 0015** | ✅ CAT062 Vertikallage I062/136 + UAP-Standard (FRN 27) | ICD 2.0.0 |
| **AP7/AP8** | ✅ CAT062 Callsign I062/245 | ICD 2.1.0, PR #15 |
| **ADR 0016** | ✅ CAT062 Track-Ende (I062/080 TSE) | ICD 2.2.0, PR #16 |
| **ADR 0018** | ✅ CAT065 SDPS-Heartbeat | ICD 2.3.0 |
| **ADR 0022** | ✅ CAT063 Sensor-Status (Per-Sensor-Liveness) | ICD 2.5.0, #32 |

### 🚧 Offen

Siehe zentrale **Wayfinder `ROADMAP.md`** für aktuelle Priorisierung (Prio 1 / Prio 2).

---

## 📋 Cross-Project-Abhängigkeiten (zu Wayfinder)

Siehe `docs/cross-project/todo-for-firefly.md`:

- **ORCH-5 (Live-Quell-Ingestion)** — generische Input-Adapter, Firefly-Arbeit
- **Per-Track-Sensor-Provenienz** — erfordert CAT062-ICD-Änderung
- **SWIM-Integration** — Abhängigkeit von Wayfinder EFS/IMS (Prio 2)
- **Ende-zu-Ende-HA** — Wayfinder WF2-52/53 ↔ Firefly SDPS-002

---

## 🔧 Technologie-Stack (ratifiziert)

- **Sprache:** Rust (ADR 0001)
- **Tracking:** Kalman-Filter + IMM/JPDA
- **Ausgabe:** CAT062 über UDP-Multicast (ADR 0006)
- **Deployment:** Docker + Kubernetes-ready (ADR 0003)

---

## 📚 Wichtige Dateien

- `docs/ICD-CAT062.md` — Schnittstellen-Vertrag mit Wayfinder (maßgeblich, versioniert)
- `docs/decisions/` — ADRs (0001–0022)
- `CLAUDE.md` — Arbeitsregeln
