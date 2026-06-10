# ADR 0009 — Frontend-Architektur M3 (Async-Server, WebSocket, JSON-Adapter, MapLibre)

- **Status:** akzeptiert
- **Datum:** 2026-06-10

## Kontext

Bis einschließlich M2 endet die Verarbeitungskette im Prozess: Plots rein,
`SystemTrack`s raus — sichtbar nur in Tests. M3 baut die **Brücke nach draußen**:
Die berechneten Tracks sollen *live* (Scan für Scan, während die Simulation
läuft) in einem Browser ankommen und dort auf einer 2D-Karte erscheinen. Das ist
zugleich die Grundlage für die **Ein-Befehl-Demo** (NFR-OPS-001) und ein erster,
konkreter Ausgabe-Pfad neben dem späteren CAT062-Encoder zur ASD (ADR 0006).

Vier Bausteine sind zu entscheiden:

1. **Async-Runtime + Web-Framework** (Rust-Server, der Verbindungen, Datenstrom,
   Health-Probes und sauberes Herunterfahren *gleichzeitig* bedient).
2. **Transport** zum Browser (laufendes „Push" neuer Track-Positionen).
3. **Ausgabe-Format** auf der Leitung (neutral gehalten, Ports & Adapters).
4. **Karten-Bibliothek** im Browser.

## Entscheidung

1. **Async-Runtime: Tokio. Web-Framework: axum.** Tokio ist der De-facto-Standard
   für asynchrones Rust; axum stammt aus demselben Ökosystem (Tower-Middleware),
   ist verbreitet, gut dokumentiert und trägt unsere Cloud-Prinzipien sauber:
   Health-/Readiness-Probes als Routen, 12-Factor-Konfiguration, geordnetes
   Shutdown.
2. **Transport: WebSocket.** Eine dauerhafte, bidirektionale Verbindung passt zum
   *laufenden* Pushen von Track-Frames — anders als klassische, jeweils wieder
   geschlossene HTTP-Anfragen.
3. **Ausgabe-Format: JSON über `serde_json`, als erster Output-Adapter.** Ein
   `Frame` = `{ time, sensor, tracks: [SystemTrack, …] }`. JSON ist
   menschenlesbar und im Browser leicht zu debuggen — ideal für die Lern- und
   Demo-Phase. Der Tracker-Kern bleibt neutral; JSON ist *ein* Adapter, der
   spätere **CAT062-Encoder** ein *zweiter* auf demselben `SystemTrack`.
4. **Karten-Bibliothek: MapLibre GL.** GPU-gestützte Vektorkarte (WebGL), offen
   und anbieter-neutral.

## Begründung

- **MapLibre statt Leaflet — bewusste Wahl Richtung Zukunft.** Beide könnten die
  heutige Last (zehn bis wenige hundert Tracks) tragen. MapLibre rendert per GPU
  (WebGL) und skaliert dadurch besser zu vielen, häufig aktualisierten Objekten —
  genau die Richtung von **M4** (mehrere Radare, dichterer Verkehr, Fusion).
  Außerdem ist MapLibre quelloffen und **anbieter-neutral** (Fork von Maplibre
  aus dem offenen Mapbox-GL-Stand), was zu unserem Souveränitäts-/On-Prem-Anspruch
  passt (ADR 0003): Vektor-Kacheln und Stil lassen sich selbst hosten, ohne an
  einen Karten-Konzern oder dessen API-Schlüssel gebunden zu sein. Der Preis ist
  eine etwas steilere Lernkurve; den nehmen wir bewusst in Kauf, statt in M4
  umbauen zu müssen.
- **Kern bleibt neutral (ADR 0006).** Server, JSON und Karte sind allesamt
  *Adapter* um den unveränderten Tracker-Kern. Ein zweiter Ausgabe-Pfad (CAT062)
  ändert den Kern nicht.
- **Cloud-Prinzipien von Anfang an (ADR 0003).** axum/Tower erlauben
  Health-/Readiness-Routen, strukturierte Logs/Metriken (NFR-OBS-001) und
  geordnetes Herunterfahren, ohne Architektur-Bruch.

## Abgrenzung (was hier *nicht* entschieden wird)

- **Karten-Hintergrund / Kachel-Quelle** (welcher Stil, welche selbst-gehostete
  oder offline Vektor-Kachel-Quelle) — Detail des Frontend-Häppchens (3.4); die
  Wahl von MapLibre macht Selbst-Hosting *möglich*, schreibt eine Quelle aber
  nicht fest.
- **Message-Bus** (NATS/Kafka) bleibt offen (eigener ADR, sobald relevant); M3
  nutzt zunächst den direkten WebSocket-Pfad.
- **Genauer Frame-/JSON-Schema-Schnitt** wird im Adapter-Häppchen (3.1) festgelegt.

## Konsequenzen

- Neue (Entwicklungs-)Abhängigkeiten ab M3: `tokio`, `axum` (Server),
  `serde_json` (Frame-Serialisierung) — jeweils beim Einführen im Häppchen
  begründet/getestet.
- Das Frontend (statisches HTML/JS mit MapLibre GL) wird ein eigenes, kleines
  Artefakt neben den Rust-Crates; es *rendert* nur und trifft keine
  safety-relevante Entscheidung (ADR 0008).
- M3 wird in Häppchen geschnitten (3.1 JSON-Adapter → 3.2 Player → 3.3
  WebSocket-Server → 3.4 MapLibre-Frontend → 3.5 Ein-Befehl-Demo); der
  CAT062-Encoder (3.X) folgt separat nach der Demo.
