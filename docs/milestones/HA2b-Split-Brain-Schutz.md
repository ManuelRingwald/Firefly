# HA.2b — Split-Brain-Schutz + Failover-Observability

> **Anforderung:** FR-TRK-050 (erweitert) · **ADR:** 0041 (Nachtrag) ·
> **ICD:** unberührt · **Einstufung:** S4 · umgesetzt auf Fable 5

## Fachlich

HA.2a erkennt einen toten Main per Timeout — aber Timeout-Erkennung kann
sich irren: Nach einer Netz-Partition (der Standby hört den Main nicht,
obwohl er lebt) senden **zwei Instanzen dieselbe SDPS-Identität** — das
ASD sähe doppelte, gegeneinander springende Tracks. HA.2b macht diesen
Zustand zu einem kurzen Übergang statt eines Dauerzustands: Die beiden
Seiten erkennen einander am Heartbeat, und eine **deterministische
Regel** entscheidet, wer weicht — ohne Koordinator, ohne Konfiguration.
Dazu wird der Failover-Zustand messbar (`firefly_role` & Co.).

## Technik

- **Startup-Arbitrierung:** Ein `main` lauscht **vor** dem ersten Senden
  einen Failover-Timeout (3 s) auf der Gruppe. Fremder Heartbeat der
  eigenen Identität ⇒ Standby-Wache statt Doppel-Feed. Fängt den
  demotierten Main nach Neustart und die versehentlich doppelt als
  `main` konfigurierte Instanz. Fail-open bei Socket-Fehler (laut
  gewarnt aktiv): Split-Brain-*Risiko* < sicherer Ausfall.
- **Laufzeit-Demotion (crash-only):** Die aktive Instanz beobachtet die
  Gruppe weiter (`standby::run_demotion_watch`). Ein fremder Heartbeat
  der eigenen SAC/SIC (andere Absender-Adresse) ist Split-Brain-Evidenz.
  **Tie-Breaker:** die höhere Absender-Adresse (IP, Port) weicht — beide
  Seiten sehen beide Adressen, also geht **genau eine** (nie beide =
  Doppel-Ausfall, nie keine = Dauer-Split-Brain). Die weichende Seite
  beendet sich mit **Exit-Code 3**; der Supervisor-Neustart landet über
  die Arbitrierung im Standby. Kein In-Prozess-Umbau (Sender
  stummschalten, Quellen stoppen) — crash-only ist der einfachere,
  ehrlichere Pfad.
- **Eigen-Erkennung:** Eigene Loopback-Heartbeats werden über die
  Absender-Adresse erkannt (Egress-IP Richtung Gruppe + Port des
  Heartbeat-Sockets, der dafür vor dem Task-Spawn erzeugt wird). Ist die
  Selbst-Adresse unbestimmbar, wird die Wache **nicht** scharf — eine
  falsche Selbst-Sicht dürfte sonst die einzige Instanz töten.
- **Metriken:** `firefly_role` (1 = aktiv, 0 = standby),
  `firefly_failovers_total` (Promotions dieses Prozesses),
  `firefly_main_heartbeat_age_seconds` (im Standby je Schleifendurchlauf
  gepflegt).

## Ehrliche Grenzen

- **Kein Konsens:** Während einer echten Netz-Partition zwischen den
  Instanzen senden **beide**, bis die Partition heilt und die Demotion
  greift — ohne Quorum ist das nicht eliminierbar (CAP; ein
  Konsens-Protokoll wäre ein eigener, schwerer ADR).
- **Supervisor vorausgesetzt:** Die demotierte Instanz beendet sich;
  ohne Restart-Policy bleibt sie unten (dann läuft der Betrieb einfach
  einspurig weiter — sicher, aber ohne Reserve).
- **Kaltstart-Latenz:** +1 Failover-Timeout für jeden Main mit
  aktiviertem Feed (der Preis der Arbitrierung).
- **Multi-homed-Hosts:** asymmetrisches Routing kann die
  Eigen-Erkennung täuschen; die Wache deaktiviert sich dann bzw. der
  Tie-Breaker entscheidet — bekannte, dokumentierte Restlücke.
