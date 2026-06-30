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
- **Großes Bild:** Die **Firefly-Seite des Quell-Eingangs-Kontrakts (#35) ist für
  `adsb_opensky` fertig** — Kontrakt (ADR 0023) + Live-Verdrahtung + **OpenSky
  OAuth2 Client-Credentials** (ADR 0024). Die Wayfinder-Auto-Orchestrierung ist
  drüben **komplett** (ORCH-1…5c + E2E-Harness, gehärtet/reviewed); damit kann
  „Feed zuweisen ⇒ Firefly-Instanz startet ⇒ Tracks im ASD" end-to-end gefahren
  werden (realer Abnahme-Lauf steht beim Betreiber an). Alles auf `main`, alle
  Gates grün (`cargo test/clippy/fmt --workspace`).

- **Letzte Arbeit (2026-06-29/30):** **ADR 0024** — OpenSky-Auth Basic → OAuth2
  Client-Credentials. `auth.rs`-Token-Manager (`TokenCache`: Reuse bis Skew-vor-
  Ablauf, proaktiver Refresh, 401-Recovery; reine `needs_refresh` + injizierter
  Fetch → ohne Netz/Uhr testbar). `config.rs`: `client_id`/`client_secret`/
  `token_url` (`FIREFLY_OPENSKY_CLIENT_ID`/`_CLIENT_SECRET`/`_TOKEN_URL`).
  `poller.rs`: Bearer + Einmal-Retry bei 401; anonym unverändert. Cred-Wert jetzt
  `client_id:client_secret` (Wire-Vertrag unverändert, `source-input-contract.md`
  v1.1.0). Davor: Schritt 2b (Live-Verdrahtung `FIREFLY_SOURCES`), ADR 0023.

- **Nächste Schritte (für die frische Session):**
  1. **Live-Input-Adapter aus #35** — je eigener ADR + Meilenstein, Ports &
     Adapters (Tracker-Kern format-neutral). Vokabular im Kontrakt bereits
     reserviert, Wayfinder rendert beide schon in `FIREFLY_SOURCES`:
     - **`flarm_aprs`** (OGN/APRS-IS, BBox-gefiltert),
     - **`radar_asterix`** (ASTERIX-Eingang CAT048/CAT001 eines realen Radars,
       SAC/SIC-identifiziert; SDPS-001 #19).
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
