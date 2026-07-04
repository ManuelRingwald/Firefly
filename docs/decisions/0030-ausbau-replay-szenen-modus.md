# ADR 0030: Ausbau des Replay-/Szenen-Modus — der quellen-getriebene Live-Tracker ist der einzige Betriebsmodus

**Status:** akzeptiert (2026-07-04)

## Kontext

Der Server kannte zwei Betriebsmodi (ADR 0020): **Replay** (Default) spielte
eine vorberechnete Demo-Szene ab (`FIREFLY_SCENE=demo|frankfurt`, Tempo via
`FIREFLY_SPEED`), **Live** (`FIREFLY_MODE=live`) betrieb den echten Tracker
über die konfigurierten Quellen (ADR 0023). Die Szenen stammten aus der
Lernprojekt-Phase (M1–M6) — als Showcase, bevor es echte Quellen gab.

Seit dem Produktions-Pivot (ADR 0014) und dem Quell-Eingangs-Kontrakt
(`FIREFLY_SOURCES`, ADR 0023) ist der Szenen-Pfad **Altlast**:

- Der Betreiber testet die echte Kette (Wayfinder-Orchestrator → Auto-Spawn →
  Live-Quellen). Die Frankfurt-Szene hat dort keinen Mehrwert (E2E-Rückmeldung
  2026-07-04) und stiftete Verwirrung — zuletzt dadurch, dass ein Feed **ohne**
  Quellen vom Orchestrator still mit der Szene bespielt wurde und
  Phantom-Tracks zeigte.
- Zwei Modi bedeuten doppelte Pfade in Server, Doku und Betriebsanleitungen —
  Pflegeaufwand ohne operativen Nutzen.

## Entscheidung

1. **Der quellen-getriebene Live-Tracker ist der einzige Betriebsmodus.**
   `ServerMode`, `Scene`, `scene.rs`, der Replay-CAT062-Sender im Server, die
   Log-only-OpenSky-Variante und der „Verzug simulieren"-Demo-Knopf der
   eingebauten Karte entfallen. `FIREFLY_MODE`, `FIREFLY_SCENE` und
   `FIREFLY_SPEED` werden **toleriert und ignoriert** (Warn-Log), damit alte
   Deployments nicht brechen.
2. **Ohne aktive Quelle: leerer Himmel, ehrlich.** Eine Instanz ohne Quellen
   läuft mit leerem Tracker, sendet weiter den CAT065-Heartbeat (ADR 0018) und
   meldet `/ready` **sofort bereit** — ihr leerer Himmel *ist* das vollständige
   Lagebild. (Zuvor wäre `/ready` ewig 503 geblieben, weil nur der erste
   Quell-Plot das Flag setzte.)
3. **Standalone-Quellen sind durchgängig Opt-in.** Da Live jetzt der Default
   ist, darf ein nackter Start keinen Überraschungs-Egress erzeugen: OpenSky
   wird im Fallback-Pfad (ohne `FIREFLY_SOURCES`) nur noch mit
   `FIREFLY_OPENSKY_ENABLED=true` aktiviert — symmetrisch zu FLARM/Radar.
4. **Die Test-Grundwahrheit bleibt.** `firefly-sim` und `firefly-player` sind
   Verifikations-Infrastruktur, kein Demo-Ballast. Die Frankfurt-Regressionstests
   (JPDA-Kreuzungspaar, Ein-Track-pro-Flugzeug, Multi-Radar-Handover) ziehen als
   Fixture nach `firefly-player/tests/frankfurt_regression.rs` um — die
   Nachweise für FR-TRK-018…023 bleiben lückenlos. Ebenso unberührt: die
   `.ffplots`-Replay-Engine (NFR-REPRO-001, Fehler-Reproduktion) und der
   getaktete Multicast-Sender `firefly_multicast::run` (generische, durch
   Wire-Level-Tests abgesicherte Transport-Primitive).

## Konsequenzen

- **Wire-Vertrag unverändert** (CAT062/065/063 byte-identisch). Die ICD
  verliert nur die Replay-Bezüge (Szenen-Ursprung als I062/100-Referenz,
  Replay-Verhalten von CAT063) — dokumentarische Klarstellung, ICD 2.6.1.
- **Wayfinder zieht nach** (eigener PR): der Orchestrator-Platzhalter
  `WAYFINDER_FIREFLY_SCENE` entfällt; ein Feed ohne Quellen spawnt einen
  Firefly mit leerem Himmel + Heartbeat statt einer Fake-Szene;
  `docker-compose.bridge.yml` (komplett szenen-basiert) entfällt.
- `NFR-OPS-001` (Ein-Befehl-Demo mit eingebauter Szene) **entfällt** als
  Anforderung; die Vorführbarkeit läuft heute über die echte Kette
  (Wayfinder-Stacks, OpenSky-Quellen).
- `SensorHealthMonitor::new_replay` heißt jetzt `new_preseeded` (nur noch
  Test-/Statik-Nutzung).

## Bezüge

ADR 0014 (Produktions-Pivot), ADR 0018 (CAT065-Heartbeat = „leerer Himmel ≠
toter Feed"), ADR 0020 (Live-Modus — durch diesen ADR vom *Modus* zum
*einzigen Betrieb* befördert), ADR 0023 (Quell-Kontrakt), Wayfinder-E2E-Feedback
2026-07-04.
