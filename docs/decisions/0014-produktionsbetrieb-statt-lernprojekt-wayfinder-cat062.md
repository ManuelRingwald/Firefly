# ADR 0014 — Produktionsbetrieb statt Lernprojekt; Wayfinder konsumiert CAT062/UDP

- **Status:** akzeptiert
- **Datum:** 2026-06-13

## Kontext

Firefly und Wayfinder wurden ursprünglich als **Lernprojekte** angelegt: Tempo
zweitrangig, Verständnis erstrangig, jeder Fachbegriff erklärt, Modellwahl an
der Lernkurve orientiert (CLAUDE.md, alte Fassung, Abschnitt 1–2).

Der Projektverantwortliche hat entschieden, diesen Rahmen **für beide Projekte
aufzugeben** und stattdessen für den **realen Betrieb** zu bauen. Im selben Zug
wurde die offene Frage aus der Wayfinder-Planung entschieden: Wayfinder ist
selbst das **ASD** (Air Situation Display) — entsprechend soll es den
**produktiven ASD-Kontrakt** von Firefly konsumieren, nicht den
Lern-/Demo-Pfad.

Firefly bietet zwei Ausgabe-Adapter auf demselben `SystemTrack`-Strom
(ADR 0006, ADR 0009):

- **JSON/WebSocket** (`firefly-io`, `firefly-server`) — ursprünglich als
  Lern-/Demo-Pfad mit MapLibre konzipiert (ADR 0009).
- **ASTERIX CAT062/UDP-Multicast** (`firefly-asterix`, `firefly-multicast`) —
  der in ADR 0006 festgelegte ASD-Produktions-Kontrakt, inkl. Empfänger-seitigem
  Decoder (Häppchen D.1–D.3, vollständig getestet inkl. Loopback-Empfänger).

Vor dieser Entscheidung hatte Wayfinder (Stand: Charta + 5 GitHub-Issues
`from-wayfinder` #6–#10) seine gesamte Schnittstellen-Erwartung gegen
JSON/WebSocket formuliert.

## Entscheidung

1. **Beide Projekte (Firefly, Wayfinder) werden auf Produktionsbetrieb
   umgestellt.** Die didaktische Rahmung (CLAUDE.md, alte Fassung) entfällt;
   Maßstab ist Produktionsreife (Korrektheit, Robustheit, Sicherheit,
   Betreibbarkeit, Zertifizierungs-Fähigkeit). Das Ankündigen-und-Freigabe-Tor
   bleibt erhalten — als **Design-/Review-Gate**, nicht als Lern-Ritual.
2. **Wayfinder konsumiert CAT062/UDP-Multicast** (Pfad A, ADR 0006) als
   primären und einzigen produktiven Kontrakt — nicht JSON/WebSocket (Pfad B).
3. **Pro Schritt wird das verwendete bzw. empfohlene Modell genannt** — für den
   Schritt selbst und für jede an einen Subagenten delegierte Arbeit
   (Ergänzung zur bestehenden S1–S5-Skala).

## Begründung

- **Wayfinder ist selbst das ASD** (eigene Charta, Abschnitt 1). Der in ADR 0006
  für das ASD definierte Kontrakt (CAT062/UDP) ist damit der konsequente
  Eingang, nicht ein zweites, demo-orientiertes Format.
- **I062/105 liefert WGS84 direkt** — Wayfinder kann CAT062 ohne
  stereografische Rückprojektion rendern (I062/100 bleibt als zusätzliche
  Systemebene optional verwertbar). Der Mehraufwand eines ASTERIX-Decoders in Go
  ist damit überschaubar und durch Fireflys byte-genaue Encoder-Tests
  (`firefly-asterix::cat062`) referenzierbar.
- **Multicast ist nativ Fan-out.** Mehrere ASD-Instanzen/Arbeitsplätze hören
  unabhängig dieselbe Multicast-Gruppe — das in Issue #6 beschriebene
  Replay-Problem des WebSocket-Pfads entsteht für CAT062 gar nicht erst.
- **ASTERIX ist selbstbeschreibend** (CAT/LEN/FSPEC) — der in Issue #8
  beschriebene Diskriminator-Bedarf entfällt.
- **Empfänger-Seite bereits bewiesen.** ADR 0006, Häppchen D.1–D.3: Decoder,
  Rückprojektion und echter Multicast-Empfänger sind in Firefly Ende-zu-Ende
  getestet (Sender → Draht → Empfänger → Decoder). Wayfinder repliziert dieses
  Verhalten eigenständig in Go gegen dieselben Referenzvektoren — kein
  Code-Import (Konsistenz mit "Kein Firefly-Code importieren").

## Konsequenzen

- **Wayfinder-Charta** wird komplett neu gefasst: CAT062/UDP-Vertrag als
  Abschnitt 2 (Herzstück), Stack-Vorschlag (Go) unverändert, Produktions-
  Querschnittsprinzipien statt Lern-Workflow.
- **Firefly-Charta** verliert die didaktische Rahmung, behält aber Abschnitt 9
  (Cross-Project-Todos) als funktionierenden Koordinationsmechanismus —
  inhaltlich aktualisiert auf den neuen Kontext.
- **Issues #6, #8, #10** (Pub/Sub-Fan-out, Typ-Diskriminator, Schema-
  Versionierung) werden **geschlossen** — durch die CAT062-Architektur
  gegenstandslos (siehe Begründung).
- **Issue #7** (Auth auf `/ws`) wird transformiert: Multicast hat keine
  Verbindungs-/Token-Authentifizierung; die Sicherheitsfrage verschiebt sich auf
  **Netz-Isolation des Multicast-Pfads** und den **Browser-Rand von Wayfinder**
  (eigener ADR in Wayfinder).
- **Issue #9** (UTC Time-of-Day) bleibt **unverändert relevant und wird
  zentraler**: CAT062 I062/070 *ist* das ASTERIX-Time-of-Day-Feld; Firefly
  kodiert heute "Sekunden seit Szenario-Start" statt echter UTC-Tageszeit. Wird
  zu einem eigenen Produktions-Häppchen in Firefly.
- Der **JSON/WebSocket-Pfad** (`firefly-io`, `firefly-server`) bleibt im Code
  bestehen (ADR 0006-Nachtrag: "Adapter bleiben unabhängig voneinander") und
  kann z. B. für Recorder/Tools weiterverwendet werden — ist aber **nicht**
  mehr der ASD-Kontrakt.

## Ehrliche Grenze

Dieser ADR ist eine **Rahmen-Entscheidung**; die konkrete Umsetzung (Wayfinder
ADR 0001 für den Stack, Decoder-Implementierung, Sicherheits-ADR für den
Multicast-Eingang) folgt in eigenen, einzeln abgestimmten Häppchen.
