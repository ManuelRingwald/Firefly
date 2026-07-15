# ADR 0041: Main/Standby über Heartbeat-Wache (HA.2)

**Status:** akzeptiert (2026-07-15, Betreiber-Go HA.2) · **Bezug:**
ARTAS-Gap-Roadmap AP-HA (SDPS-002), ADR 0018 (CAT065-Heartbeat),
ADR 0040 (Zustands-Snapshot, HA.1), ADR 0017 (Netz-Isolation),
FR-TRK-050 · Umsetzung in zwei Häppchen: **HA.2a** (Standby-Rolle +
Übernahme), **HA.2b** (Split-Brain-Schutz + Failover-Observability)

## Kontext — in normaler Sprache

HA.1 macht den Neustart *derselben* Instanz schnell — aber eine einzelne
Instanz bleibt ein Single Point of Failure: Stirbt der Prozess oder sein
Knoten, ist das ASD blind, bis jemand (oder ein Orchestrator) neu startet
und der Neustart durchläuft. SDPS-002 verlangt mehr: eine **zweite
Instanz in Bereitschaft**, die automatisch übernimmt.

Die Kernfrage: **Woher weiß die Bereitschafts-Instanz, dass die aktive
tot ist?** Klassische Antworten brauchen einen externen Koordinator
(etcd, ZooKeeper, Kubernetes-Lease) — neue Infrastruktur, neue
Fehlerquellen, Anbieter-Kopplung.

## Entscheidung

1. **Die Liveness-Quelle ist der eigene Wire-Vertrag.** Der Standby
   lauscht auf der CAT062-Multicast-Gruppe auf den **CAT065-Heartbeat**
   der eigenen SDPS-Identität (SAC/SIC) — das Signal existiert genau
   dafür (ADR 0018: „leerer Himmel" vs. „toter Feed"), jeder Konsument
   sieht dieselbe Wahrheit, kein Koordinator nötig, anbieter-neutral.
2. **Rollen explizit:** `FIREFLY_ROLE ∈ {main (Default), standby}`; ein
   Tippfehler ist ein Start-Fehler (nie versehentlich ein zweiter
   aktiver Sender). Ein Standby verlangt den aktivierten Multicast-Feed
   samt Heartbeat — sonst kann er weder wachen noch nach Übernahme
   senden (Start-Fehler).
3. **Standby = Probes only.** Der Standby bedient `/health`, `/metrics`
   und antwortet auf `/ready` mit **503 „standby"** (Kubernetes schickt
   ihm keinen Traffic); er sendet **nichts** (kein CAT062/065/063 — eine
   SDPS-Identität, ein Sender) und pollt **keine Quellen** (kein doppeltes
   Rate-Limit-Budget). Er ist ein *warm spare über den geteilten
   HA.1-Snapshot*, kein heißer Zweit-Tracker.
4. **Promotion bei Stille:** Bleibt der Heartbeat länger als
   `FIREFLY_FAILOVER_TIMEOUT` (Default 3 s = drei verpasste Heartbeats
   bei 1-s-Periode) aus, startet der Standby den vollen Live-Stack —
   inklusive **HA.1-Restore** des letzten Snapshots vom gemeinsamen
   Volume: gleiche Track-Nummern, Identitäten und manuelle Pins. Die
   Uhr läuft ab Standby-Start: Ist der Main beim Start schon tot, kommt
   die Übernahme einen Timeout später — es braucht nie einen ersten
   Heartbeat, um den Detektor zu „armieren".
5. **Split-Brain-Schutz (HA.2b):** Ein Main, der (wieder) startet oder
   läuft und einen **fremden aktiven Heartbeat derselben Identität**
   sieht, geht selbst in Standby (Demotion) statt doppelt zu senden.
   Dazu Failover-Observability (`firefly_role`,
   `firefly_failovers_total`, Heartbeat-Alter).

## Begründung

- **Keine neue Infrastruktur:** Die Multicast-Gruppe ist ohnehin das
  Rückgrat (ADR 0006/0017); ein Koordinator wäre eine zweite
  Verfügbarkeits-Abhängigkeit mit eigener Ausfall-Semantik.
- **Beobachtbare Wahrheit:** Der Failover-Auslöser ist dasselbe Signal,
  mit dem auch Wayfinder Staleness erkennt — was der Standby sieht,
  sieht der Konsument.
- **Warm statt heiß:** Ein zweiter voll mitlaufender Tracker müsste alle
  Quellen doppelt konsumieren (HTTP-Poller: doppelte Rate-Limits) und
  bliebe trotzdem nie bit-identisch. Der Snapshot-Weg akzeptiert ein
  kleines Verlustfenster (≤ Snapshot-Periode) für ein deutlich
  einfacheres, ehrliches Modell.

## Nachtrag HA.2b — Split-Brain-Schutz (gleicher Tag)

Die in Punkt 5 angekündigte Demotion ist umgesetzt, mit zwei bewussten
Konkretisierungen:

1. **Startup-Arbitrierung:** Ein `main` lauscht **vor** dem ersten Senden
   einen Failover-Timeout lang auf der Gruppe. Hört er einen fremden
   Heartbeat seiner eigenen Identität, geht er in die Standby-Wache statt
   den Feed zu doppeln — das fängt den neu startenden (demotierten) Main
   und die versehentlich doppelt als `main` konfigurierte Instanz.
   Kosten: +1 Timeout (Default 3 s) beim Kaltstart jedes Mains mit
   aktiviertem Feed. Die Arbitrierung ist **fail-open**: Kann sie nicht
   lauschen (Socket-Fehler), startet die Instanz laut gewarnt aktiv —
   ein Split-Brain-*Risiko* wiegt leichter als ein sicherer Ausfall.
2. **Laufzeit-Demotion als Crash-only:** Die aktive Instanz beobachtet
   die Gruppe weiter. Ein fremder Heartbeat der eigenen Identität
   (andere Absender-Adresse) ist Split-Brain-Evidenz; der
   **deterministische Tie-Breaker** — der Sender mit der **höheren**
   Absender-Adresse (IP, Port) weicht — sorgt dafür, dass genau eine
   der beiden Seiten geht. Die weichende Seite **beendet sich mit
   Exit-Code 3** statt sich im Prozess umzubauen (Sender stummschalten,
   Quellen stoppen — fehleranfällige Sonderpfade): Der Supervisor
   (Kubernetes, systemd, Docker-Restart-Policy) startet sie neu, und die
   Startup-Arbitrierung landet sie sauber im Standby. **Ohne Supervisor
   bleibt die demotierte Instanz unten** — dokumentierte
   Betriebs-Voraussetzung.
3. **Eigen-Erkennung:** Die eigenen, per Multicast-Loopback zurückkommenden
   Heartbeats werden über die Absender-Adresse (Egress-IP zur Gruppe +
   Port des Heartbeat-Sockets) erkannt. Lässt sich die eigene Adresse
   nicht bestimmen, wird die Demotion-Wache **nicht** scharfgeschaltet
   (eine falsche Selbst-Sicht könnte den eigenen Loopback als fremd lesen
   und die einzige Instanz töten) — der Schutz reduziert sich dann auf
   die Startup-Arbitrierung, laut geloggt. Multi-homed-Hosts mit
   asymmetrischem Routing sind die bekannte Restlücke.
4. **Observability:** `firefly_role` (1 = aktiv, 0 = standby),
   `firefly_failovers_total` (Promotions dieses Prozesses),
   `firefly_main_heartbeat_age_seconds` (Alter des beobachteten
   Heartbeats, im Standby gepflegt).

## Ehrliche Grenzen (bewusst dokumentiert)

- **Failure Detection per Timeout, kein Konsens.** Eine Netz-Partition
  zwischen den Instanzen (Standby hört den Main nicht, Konsumenten
  schon) kann zu **zwei Sendern** führen, bis die Partition heilt —
  die HA.2b-Demotion begrenzt das auf die Partitions-Dauer, beseitigt
  es aber nicht (CAP: ohne Konsens-Quorum ist das nicht lösbar; ein
  Quorum wäre ein eigener, schwerer ADR).
- **Übernahme-Bild = letzter Snapshot** (Verlustfenster ≤ Periode + 
  Failover-Timeout); Quellen-Neuverbindung (HTTP-Poller, UDP-Join)
  beginnt erst mit der Promotion.
- Der Standby braucht Zugriff auf **dasselbe Snapshot-Volume** wie der
  Main (Deployment-Sache, HA.3) — ohne Volume übernimmt er mit leerem
  Bild (die HA.1-Torwächter melden das laut).
- Ein WS-Client am Standby sieht bis zur Promotion einen leeren Strom
  (`/ready` sagt warum).
