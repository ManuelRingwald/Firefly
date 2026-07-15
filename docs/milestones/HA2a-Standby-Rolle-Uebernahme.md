# HA.2a — Standby-Rolle + automatische Übernahme

> **Anforderung:** FR-TRK-050 · **ADR:** 0041 · **ICD:** unberührt
> (CAT065 wird nur konsumiert) · **Einstufung:** S4 (Teil von HA.2, S5) ·
> umgesetzt auf Fable 5

## Fachlich

Eine einzelne Firefly-Instanz ist ein Single Point of Failure: Stirbt der
Prozess oder sein Knoten, ist das ASD blind, bis jemand neu startet.
Jetzt kann eine **zweite Instanz in Bereitschaft** mitlaufen: Sie sendet
nichts und verbraucht keine Quellen-Budgets, beobachtet aber den
**CAT065-Heartbeat** der aktiven Instanz. Verstummt er, **übernimmt sie
automatisch** — mit dem letzten HA.1-Snapshot als Startbild: gleiche
Track-Nummern, gleiche Identitäten, gleiche Lotsen-Pins. Kern von
SDPS-002, ohne neue Infrastruktur (kein etcd, kein Kubernetes-Lease):
Der eigene Wire-Vertrag trägt das Liveness-Signal.

## Technik

- **Rolle** `FIREFLY_ROLE ∈ {main, standby}` (Default main; Tippfehler =
  Start-Fehler). Standby verlangt aktivierten Multicast-Feed + Heartbeat.
- **Standby-Phase** (`main::run_standby_phase`): Probes-only-HTTP —
  `/ready` antwortet **503 „standby: watching the main's heartbeat"**,
  Kubernetes routet keinen Traffic; kein CAT062/065/063-Senden, keine
  Quellen-Adapter.
- **Heartbeat-Wache** (`firefly-server::standby`): lauscht per
  Multicast-Join auf CAT065 der **eigenen** SDPS-Identität. Fremde SDPS,
  CAT062/063-Verkehr und Garbage re-armieren den Detektor nie; ein
  NOGO-Heartbeat zählt als lebendig (ein degradierter Main ist immer
  noch der Sender — ihn zu doppeln wäre schlimmer). Die Uhr läuft ab
  Standby-Start: Ist der Main schon tot, kommt die Übernahme einen
  Timeout nach dem Start.
- **Promotion:** Heartbeat-Stille > `FIREFLY_FAILOVER_TIMEOUT`
  (Default 3 s = drei verpasste 1-s-Heartbeats) ⇒ voller Live-Stack
  inkl. HA.1-Restore (gemeinsames Snapshot-Volume, dieselben drei
  Torwächter); der eigene Heartbeat startet erst **nach** der Promotion.
  HTTP-Rebind mit `SO_REUSEADDR` (TIME_WAIT-Reste der Probe-Verbindungen
  blockieren den Neustart des Servers nicht).
- Deterministisch getestet (explizite Zeitpunkte statt Sleeps) plus ein
  End-to-End-Test über **echtes UDP-Multicast** (Fake-Main sendet
  Heartbeats, verstummt, Standby promotet erst danach).

## Ehrliche Grenzen

- **Timeout-Detektion, kein Konsens:** Eine Netz-Partition zwischen den
  Instanzen kann vorübergehend **zwei Sender** erzeugen. Die Demotion
  (ein Main, der einen fremden aktiven Heartbeat derselben Identität
  sieht, tritt zurück) und die Failover-Metriken (`firefly_role`,
  `firefly_failovers_total`) sind **HA.2b**.
- Übernahme-Bild = letzter Snapshot (Verlustfenster ≤ Snapshot-Periode +
  Failover-Timeout); Quellen verbinden sich erst mit der Promotion neu.
- Gemeinsames Snapshot-Volume ist Deployment-Sache (HA.3); ohne Volume
  übernimmt der Standby mit leerem Bild (die HA.1-Torwächter melden das
  laut).
- Ein WS-Client am Standby sieht bis zur Promotion einen leeren Strom.
