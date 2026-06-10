# M3 — Vom Tracker zum Live-Lagebild im Browser

> Verständliche Erklärung des dritten Meilensteins. Begriffe stehen
> ausführlicher im [Glossar](../glossary.md).

M2 hat den Single-Radar-Tracker fertiggestellt: Aus Plots werden saubere,
durchgehende `SystemTrack`s in WGS84. M3 bringt diese Tracks **live in den
Browser** — als 2D-Karte, mit einem Befehl startbar, und mit einer Demo, die
zeigt, dass das System auch bei holpriger Zustellung robust bleibt.

---

## Häppchen 3.0 — Architektur-Entscheidung (ADR 0009)

**Status:** ✅ umgesetzt

### Die Entscheidung (technisch)

Vier Bausteine mussten zusammenpassen:

- **Async-Server:** **Tokio + axum** — die Standard-Kombination im
  Rust-Ökosystem für netzwerkgebundene Dienste (siehe Glossar: *async/await &
  Runtime*, *axum*).
- **Transport:** **WebSocket** — eine dauerhafte Verbindung, über die der
  Server fortlaufend Daten an den Browser *schiebt* (statt dass der Browser
  ständig nachfragt).
- **Ausgabeformat:** **JSON** — menschenlesbar, in jeder Sprache verarbeitbar,
  guter erster Adapter (ein binärer ASTERIX-CAT062-Adapter folgt später, ADR
  0006).
- **Karten-Frontend:** **MapLibre GL** — eine GPU-gestützte Vektorkarte, die
  auch bei dichterem Verkehr (M4) flüssig bleibt, und anbieter-neutral
  selbst-hostbar ist.

Alle vier Entscheidungen sind in
[ADR 0009](../decisions/0009-frontend-architektur-m3.md) festgehalten.

---

## Häppchen 3.1 — Der neutrale `Frame` (Crate `firefly-io`)

**Status:** ✅ umgesetzt · Anforderung `FR-IO-001`

### Das Problem (fachlich)

Der Tracker liefert `SystemTrack`s — neutral, aber auf das *interne* Modell
zugeschnitten (z. B. Geschwindigkeit als Ost/Nord-Vektor). Der Browser will
einfache, fertige Werte: Position in Grad, Geschwindigkeit als Betrag + Kurs.
Außerdem gehört zu jedem „Bild" der Luftlage eine **Datenzeit** und der
**Sensor**, der es erzeugt hat.

### Die Lösung (technisch)

Eine neue, kleine Crate `firefly-io` definiert `Frame { time, sensor,
tracks: Vec<FrameTrack> }`. `FrameTrack` ist die **Drahtform**: Position in
Grad (`lat_deg`, `lon_deg`), abgeleitete `ground_speed_mps` und
`track_angle_deg`, plus der Safety-Status aus ADR 0008 (`confirmed`,
`coasting`, `update_age_s`, `position_uncertainty_m`). `Frame::to_json` /
`from_json` machen den Roundtrip verlustfrei (`serde_json`).

Das ist der erste **Ausgabe-Adapter** im Sinne von Ports & Adapters (ADR 0006):
der Tracker-Kern weiß nichts von JSON oder Browsern.

---

## Häppchen 3.2 — Der „Player": Szenario → Frame-Strom

**Status:** ✅ umgesetzt · Anforderung `FR-IO-002`

### Das Problem (fachlich)

Damit der Server etwas zu senden hat, braucht er einen fertigen, zeitlich
geordneten Strom von `Frame`s — einen pro Scan-Zeitpunkt. Diese Erzeugung darf
aber **nichts** mit Netzwerk oder Wanduhr zu tun haben (ADR 0003): gleicher
Input muss immer denselben Strom ergeben.

### Die Lösung (technisch)

Die neue Crate `firefly-player` bietet `Player::new(&scenario,
tracker_config).frames() -> Vec<Frame>`. Sie spielt das Szenario (M1) durch
den Tracker (M2) und sammelt nach jedem Scan den `SystemTrack`-Stand als
`Frame` ein. Reine Funktion, keine I/O — Tempo und Netz kommen erst in 3.3
dazu.

---

## Häppchen 3.3 — Der WebSocket-Server (Crate `firefly-server`)

**Status:** ✅ umgesetzt · Anforderung `FR-NET-001`

### Das Problem (fachlich)

Der fertige Frame-Strom muss **live** beim Browser ankommen — nicht alles auf
einmal (das wäre kein „Live"-Bild), sondern getaktet wie echte Radar-Updates.
Gleichzeitig soll der Dienst sich wie ein guter Cloud-Bürger verhalten:
sauber starten/stoppen, konfigurierbar ohne Code-Änderung, beobachtbar.

### Die Lösung (technisch)

`firefly-server` (axum/Tokio) bietet:

- **`/ws`** — pumpt den Frame-Strom als JSON-Textnachrichten an den Client.
- **`/health`**, **`/ready`** — Kubernetes-Probes (ADR 0003).
- **12-Factor-Konfiguration** (`config.rs`): `FIREFLY_PORT`, `FIREFLY_SPEED`
  aus Umgebungsvariablen, mit sicheren Defaults.
- **Geordnetes Herunterfahren**: reagiert auf Ctrl-C/SIGTERM.
- **Strukturierte Logs** über `tracing` (NFR-OBS-001).

Der entscheidende Baustein ist `pacing.rs` — die **einzige** Stelle im ganzen
System, an der Datenzeit auf Wanduhr trifft (siehe Häppchen 3.5 unten).
Start mit einem Befehl: `cargo run -p firefly-server` → `http://localhost:8080`.

---

## Häppchen 3.4 — Das MapLibre-Frontend

**Status:** ✅ umgesetzt · Anforderung `FR-UI-001`

### Das Problem (fachlich)

Zahlen in einer JSON-Nachricht sagen einem Fluglotsen nichts. Er braucht ein
**Bild**: wo ist welches Flugzeug, wohin fliegt es, und — sicherheitsrelevant
— wie *sicher* ist diese Aussage gerade (ADR 0008)?

### Die Lösung (technisch)

`crates/firefly-server/static/index.html`, zur Compile-Zeit ins Server-Binary
eingebettet (`include_str!`) — ein Befehl reicht für die ganze Demo. Eine
2D-Karte (MapLibre, Stil `demotiles.maplibre.org`) verbindet sich zu `/ws` und
zeichnet pro `Frame`:

- einen **Punkt** je Track, gefärbt nach Status (blau = bestätigt, grau =
  vorläufig, orange = coasting),
- einen **Unsicherheits-Ring** um jeden Track (Radius =
  `position_uncertainty_m`), gestrichelt während des Coastings,
- einen **Geschwindigkeitsvektor** (Linie in Kursrichtung, Länge ∝
  Geschwindigkeit).

Damit ist der safety-relevante Status nicht nur *vorhanden*, sondern
*sichtbar*.

---

## Häppchen 3.5 — Demo-Erlebnis: Ein-Befehl-Start + „Verzug"-Knopf

**Status:** ✅ umgesetzt · Anforderung `NFR-OPS-001`

### Das Problem (fachlich)

Zwei Dinge fehlten noch zur **vorzeigbaren** Demo:

1. Der Server lief zwar schon mit einem Befehl — aber das war bisher nicht
   *bewiesen* sichtbar gemacht.
2. NFR-CLOUD-004 verspricht: schwankende oder verzögerte **Zustellung** der
   Daten darf den Tracks nichts anhaben — Tracks werden nach **Datenzeit**
   geführt, nicht nach Wanduhr. Das war bisher nur durch Tests in
   `firefly-track` bewiesen (`timing::*`), aber niemand außer dem Code konnte
   es *sehen*.

Für eine Vorführung vor Kollegen ohne Programmierkenntnisse ist „ich
verspreche, das ist robust" wenig überzeugend. Besser: ein Knopf, der den
Stream absichtlich kurz pausiert — und man sieht, dass die Tracks danach
einfach nahtlos weiterlaufen, mit denselben IDs, an der richtigen Position für
die verstrichene Datenzeit.

### Die Lösung (technisch)

**Wichtige Abgrenzung zuerst:** Der „Verzug" passiert **ausschließlich** an
der **Auslieferungs-Kante** — in `pump_frames` (`firefly-server/src/app.rs`).
Der Frame-Strom selbst (`Player::frames()`, also alle Track-Entscheidungen) ist
zu diesem Zeitpunkt längst fertig berechnet und unveränderlich. „Verzug
simulieren" verändert kein einziges Bit davon; es verzögert nur, *wann* ein
bereits fertiges `Frame` zum Browser geschickt wird.

Konkret:

1. **Frontend:** ein Knopf „Verzug simulieren (5 s)“ schickt die Textnachricht
   `"delay"` über den bestehenden `/ws`-Socket.
2. **Server:** die Sende-Schleife wartet ohnehin vor jedem Frame eine kurze,
   datenzeit-proportionale Pause (`pacing::delay_before`, schon seit 3.3).
   Mit `tokio::select!` hört sie *währenddessen* zusätzlich auf eingehende
   Nachrichten. Kommt `"delay"` an, schickt der Server eine Bestätigung
   (`{"event":"delay_triggered","duration_s":5.0}`) und verlängert die Wartezeit
   um 5 Sekunden (`DELAY_TRIGGER_PAUSE`).
3. **Frontend:** zeigt für die gemeldete Dauer ein Banner „Zustellung pausiert
   — Tracks bleiben stabil…“ und sperrt den Knopf währenddessen.

Ein WebSocket-Test
(`websocket::delay_trigger_pauses_delivery_without_corrupting_the_stream`)
beweist: nach dem Trigger kommt die Bestätigung sofort, und das nächste `Frame`
kommt — verzögert, aber **vollständig und in der richtigen Reihenfolge** — an.

### Warum das reicht (und was *nicht* nötig war)

Man könnte einwenden: „Aber zeigt das wirklich, dass der *Tracker* robust
ist?“ — Ja, indirekt und ehrlich: Der Frame-Strom, den der Server hier verzögert
ausliefert, *ist* das Ergebnis des Trackers für genau diese Datenzeiten. Die
Pause ändert nichts daran, dass Track #1 für `t = 42.0 s` exakt dieselbe
Position/ID hat, ob sie pünktlich oder 5 Sekunden später beim Browser
ankommt. Genau diese Eigenschaft — Entkopplung von *Berechnung* (Datenzeit) und
*Zustellung* (Wanduhr) — ist der Kern von ADR 0003/NFR-CLOUD-004, und die
`timing::*`-Tests in `firefly-track` beweisen sie bereits für den Tracker
selbst. Häppchen 3.5 macht sie für die Demo *erlebbar*.

Eine künstliche Verzögerung *im Tracker* (statt am Ausgabe-Rand) wäre der
falsche Weg gewesen — sie hätte gegen genau das Prinzip verstoßen, das gezeigt
werden soll.

---

## M3 — Fazit

Die komplette Kette steht: **ein Befehl** (`cargo run -p firefly-server`)
startet einen Server, der eine eingebaute Demo-Szene durch den Tracker spielt
und live als 2D-Karte im Browser zeigt — inklusive sichtbarem Safety-Status und
einem Knopf, der die Timing-Robustheit demonstriert. Damit ist M3
abgeschlossen.

**Als Nächstes (M3.X / M4):** ein CAT062-Encoder-Adapter (binäre
ASTERIX-Ausgabe neben JSON) sowie SSR/ADS-B-Identitätskorrelation und
Multi-Radar-Fusion.
