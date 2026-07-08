# Firefly — Dokumentation

Willkommen. Diese Dokumentation beschreibt den Radar-Tracker Firefly für
**Betrieb, Weiterentwicklung und Audit** — mit Betreiber-Handbüchern,
Schnittstellen-Vertrag (ICD), Entscheidungs-Logbuch (ADRs) und
Anforderungs-Register. (Die frühere Lern-/Schritt-für-Schritt-Rahmung wurde mit
ADR 0014 zugunsten des Produktionsbetriebs aufgegeben.)

## Wegweiser

- **[STATUS.md](STATUS.md)** — Arbeitsstand & nächster Schritt. **Hier zuerst
  schauen**, wenn du (oder Claude) wieder einsteigst — geräteübergreifend.
- **[INSTALLATION.md](INSTALLATION.md)** — Installationshandbuch für
  Systembetreiber: von Null bis zum laufenden System (Voraussetzungen, Build,
  Docker, quellen-getriebener Echtbetrieb).
- **[TECHNICAL.md](TECHNICAL.md)** — Technisches Handbuch (Betriebsführung): alle
  Umgebungsvariablen, Metriken, Betriebsmodi, Einschränkungen.
- **[ICD-CAT062.md](ICD-CAT062.md)** — Interface Control Document: maßgebliche,
  versionierte Beschreibung des CAT062/UDP-Multicast-Vertrags mit Wayfinder
  (Transport, FSPEC/UAP, FRN-Items, CAT063/CAT065, Changelog).
- **[source-input-contract.md](source-input-contract.md)** — Eingangs-Kontrakt
  (`FIREFLY_SOURCES`): wie einer Firefly-Instanz ihre Live-Quellen mitgegeben werden.
- **[glossary.md](glossary.md)** — Fachbegriff-Lexikon in einfacher Sprache.
  Wächst mit dem Projekt. Im Zweifel hier zuerst nachschlagen.
- **[milestones/](milestones/)** — Pro Baustein eine verständliche Erklärung:
  fachlicher Hintergrund, technische Umsetzung und die Mathematik *in Worten*.
  - Kern-Meilensteine:
    [M1 — Simulator](milestones/M1-simulator.md) ·
    [M2 — Tracker](milestones/M2-tracker.md) ·
    [M3 — Live-Lagebild](milestones/M3-live-picture.md) ·
    [M3.X — CAT062-Encoder](milestones/M3X-cat062-encoder.md) ·
    [M4 — Multi-Radar-Fusion](milestones/M4-multi-radar-fusion.md) ·
    [M5 — IMM & JPDA](milestones/M5-imm.md) ·
    [M6 — Showcase](milestones/M6-showcase.md)
  - Produktions-Phase (Eingangs-Adapter & Features):
    [M7 — ADS-B (OpenSky)](milestones/M7-adsb.md) ·
    [ADS-B-Aggregator (adsb.lol/adsb.fi)](milestones/ADSB-Aggregator_Adapter.md) ·
    [FLARM/OGN-Adapter](milestones/FLARM-OGN_Adapter.md) ·
    [Radar-ASTERIX-Adapter (CAT048)](milestones/Radar-ASTERIX_Adapter_CAT048.md) ·
    [OpenSky OAuth2](milestones/OpenSky-OAuth2_Client_Credentials.md) ·
    [Per-Track-Provenienz](milestones/Per-Track-Provenienz_Source-Ages.md) ·
    [Vertikallage (I062/136)](milestones/M-vertikallage-cat062.md) ·
    [Quell-Eingang (Parser/Wiring)](milestones/ORCH-Source-Input_2a_Parser.md) ·
    [Recording/Replay](milestones/SDPS-005_Legal_Recording_Replay.md) ·
    [Observability](milestones/SDPS-006_Erweiterte_Observability.md)
- **[decisions/](decisions/)** — Entscheidungs-Logbuch (ADRs): welche wichtige
  Weichenstellung *warum* getroffen wurde.
- **[requirements/](requirements/)** — Anforderungs-Register mit
  Rückverfolgbarkeit (Anforderung → Code → Test). Kern der
  Zertifizierungs-Fähigkeit.
- **[cross-project/](cross-project/)** — Cross-Project-Todos Firefly ↔ Wayfinder.
- **[../DOCKER.md](../DOCKER.md)** — Container-Setup (Docker/docker-compose),
  lokaler Start und Cloud-Deployment.

## Wie wir arbeiten

Die verbindlichen Spielregeln stehen in der [`CLAUDE.md`](../CLAUDE.md) im
Projekt-Wurzelverzeichnis. Kurzfassung: **Erst erklären, dann bauen** — und
Dokumentation gehört zur Leistung, nicht obendrauf.
