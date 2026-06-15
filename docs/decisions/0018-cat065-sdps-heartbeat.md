# ADR 0018 — CAT065 SDPS-Service-Status-Heartbeat

- **Status:** akzeptiert
- **Datum:** 2026-06-15
- **Schnittstellen-relevant:** ja (Multicast-Ausgabe-Vertrag, ICD → 2.3.0, additiv)

## Kontext

Der CAT062/UDP-Multicast-Strom überträgt heute **nur Tracks**. Ein Konsument
(Wayfinder, jedes ASD) kann damit zwei betrieblich völlig verschiedene
Situationen **nicht unterscheiden**:

1. **Leerer Himmel** — der Tracker läuft, es ist nur gerade kein Luftfahrzeug
   in Reichweite. Ein gültiger, aber track-loser CAT062-Datenblock (oder, im
   asynchronen Pfad, einfach längere Stille).
2. **Toter Feed** — Sender abgestürzt, Netz unterbrochen, Multicast-Pfad
   blockiert. Es kommt **nichts** mehr an.

Für ein sicherheitsrelevantes ASD ist das ein ernstes Problem: Der Lotse darf
einen eingefrorenen Schirm nicht für „ruhige Lage" halten. Echte
SDPS/ARTAS-Feeds lösen das mit einem periodischen **Service-Status-Report** in
**ASTERIX CAT065** („SDPS Service Status Messages"): ein kleiner Herzschlag, der
sagt „das Datenverarbeitungssystem lebt und ist operationell". Bleibt der
Herzschlag aus, weiß der Konsument: **der Feed ist tot (stale)**, unabhängig
davon, ob gerade Tracks zu sehen wären.

CAT065 ergänzt damit das Observability-Paket (#2): Logs/Metriken machen die
*Sender*-Seite beobachtbar; der Heartbeat macht die *Empfänger*-Seite
staleness-fähig und liefert ein belastbares Readiness-Signal.

## Entscheidung

1. **Neue Kategorie CAT065 (SDPS Service Status).** Firefly sendet periodisch
   eine CAT065-**SDPS-Status-Meldung** (I065/000 = 1). Der Record trägt das
   Subset der echten EUROCONTROL-CAT065-UAP, das einen periodischen Status
   ausmacht: I065/010 (SAC/SIC), I065/000 (Message Type), I065/015 (Service
   Identification), I065/030 (Time of Day, 1/128 s wie I062/070) und I065/040
   (SDPS Configuration & Status; NOGO-Feld operationell/degradiert).

2. **Gleiche Multicast-Gruppe/Port wie CAT062** (`239.255.0.62:8600`). Der
   Strom ist selbstbeschreibend: Ein Konsument dispatcht am führenden
   **CAT-Oktett** (`0x3E` → Track, `0x41` → Status). Das entspricht der Praxis
   realer SDPS-Ausgaben (ARTAS, Phoenix), hält die Multicast-Topologie auf
   **einer** Gruppe (eine Firewall-Regel, eine Vertrauensgrenze) und lässt
   **ADR 0017 unverändert** gelten. Ein Datagramm bleibt **eine** vollständige
   Meldung; das frühere „ein Datagramm = ein Scan" gilt jetzt **pro Kategorie**.

3. **Wall-clock-getaktet, nicht datenzeit-getaktet.** Der Heartbeat ist ein
   **Echtzeit-Lebenssignal**. Er wird daher im **wall-clock-Takt** gesendet
   (Default 1 s, `FIREFLY_CAT065_PERIOD`), unabhängig vom datenzeit-getakteten
   Track-Strom, und stempelt I065/030 mit der **tatsächlichen UTC-Tageszeit**
   bei Aussendung. Das ist kein Bruch der Determinismus-Regel (ADR 0003): Der
   Heartbeat trägt **keine Track-Daten** und berührt den Tracker-Kern nicht; die
   wall-clock-Lesung lebt allein am Zustell-Rand (`firefly-multicast`).

4. **Eigener, entkoppelter Sende-Task.** `firefly-multicast::run_heartbeat`
   läuft als separater Tokio-Task mit eigenem Sender-Socket, neben dem
   CAT062-Sender. 12-Factor-Konfiguration (`FIREFLY_CAT065_*`); per Default
   **an**, sobald der Feed überhaupt läuft (`FIREFLY_CAT062_ENABLED`), denn ein
   Feed ohne Lebenssignal verfehlt den Zweck.

## Konsequenzen

**Positiv:**
- Der Konsument kann „leer" von „tot" unterscheiden — Grundlage für
  Staleness-Erkennung, einen Lotsen-Hinweis und ein echtes Readiness-Signal.
- Standardtreue (echte CAT065-UAP); ein konformer Drittkonsument liest den
  Herzschlag ohne privates Profil.
- Keine Kopplung an den Tracker-Kern; reiner Zustell-Rand.

**Zu beachten (Kompatibilität):**
- Der gemeinsame Strom enthält jetzt **zwei** Kategorien. Ein Konsument **muss**
  am CAT-Oktett dispatchen und unbekannte Kategorien überspringen. Ein reiner
  CAT062-Decoder, der jedes Nicht-`0x3E`-Datagramm als Fehler wertet, sieht
  (harmlose, aber laute) Decode-Fehler. Die robuste-Decoder-Regel verlangte das
  Dispatchen ohnehin. Darum: **additiv (ICD 2.3.0)**, mit dieser Auflage.
- Der Heartbeat meldet aktuell stets **operationell**. Eine echte
  GO/NOGO-Ableitung aus dem internen Systemzustand (z. B. degradiert bei
  Sensor-Ausfall) ist ein Folgeschritt — die Verdrahtung (NOGO-Bit) steht.

## Alternativen (verworfen)

- **Eigene Multicast-Gruppe für CAT065.** Sauberere Trennung, aber eine zweite
  Gruppe/Firewall-Regel und eine erweiterte Vertrauensgrenze (zweiter Pfad in
  ADR 0017). Der Nutzen (Status-only-Konsumenten) wiegt die Betriebskosten hier
  nicht auf; die Ein-Gruppen-Lösung ist die SDPS-übliche.
- **Anwendungs-Heartbeat außerhalb ASTERIX** (eigenes JSON/Marker-Datagramm).
  Verworfen: bricht die „nur ASTERIX auf dem Draht"-Linie (ADR 0006/0014) und
  wäre für Drittkonsumenten nicht standardlesbar.
- **Track-Strom-Timeout statt Heartbeat** (Konsument rät Tod aus Stille).
  Genau das unzureichende Verhalten, das CAT065 behebt: bei unregelmäßiger
  Scan-Kadenz (ADR 0013) ist Stille mehrdeutig.
