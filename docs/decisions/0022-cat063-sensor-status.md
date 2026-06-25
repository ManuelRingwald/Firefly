# ADR 0022 — CAT063 Sensor Status Messages (Per-Sensor-Liveness)

- **Status:** akzeptiert
- **Datum:** 2026-06-25
- **Schnittstellen-relevant:** ja (Multicast-Ausgabe-Vertrag, ICD → 2.5.0, additiv)
- **Auslöser:** Firefly-Issue #32 (`from-wayfinder`) — Wayfinder braucht ein
  Signal für „Sensor ausgefallen", um sein gelbes Degradierungs-Banner zu
  aktivieren.

## Kontext

Der Multicast-Strom trägt heute zwei Kategorien:

- **CAT062** — die Tracks („*was* fliegt").
- **CAT065** — der SDPS-Heartbeat („*lebt das Datenverarbeitungssystem*", ADR
  0018).

Beide zusammen lassen eine betrieblich wichtige Frage **unbeantwortet**: *Welche
Sensoren speisen das SDPS gerade — und welche sind ausgefallen?* Fällt ein Radar
aus (Antenne steht, Netzstrecke tot, Wartung), läuft der Tracker ungestört
weiter: CAT062 liefert weiter Tracks (aus den verbleibenden Sensoren und aus
Coasting), und der CAT065-Heartbeat schlägt unverändert „operationell". Das
Lagebild wird nur in der **Abdeckung** des ausgefallenen Sensors ärmer — leiser,
schleichender Qualitätsverlust **ohne jedes Signal**.

Für ein sicherheitsrelevantes ASD ist das ein echtes Problem: Der Lotse darf
einen Bereich, der nur noch von einem Sensor (statt dreien) abgedeckt wird, nicht
für unverändert zuverlässig halten. Echte SDPS/ARTAS-Systeme lösen das mit
**ASTERIX CAT063** („Sensor Status Messages"): ein periodischer Per-Sensor-
Statusbericht, der je Sensor sagt „liefert / liefert nicht / degradiert".

Wayfinder hat in seiner Sitzung das gelbe **Sensor-Degradierungs-Banner**
vorbereitet (AP4-Farbsemantik: gelb = Sensor-Degradierung, *nicht* leerer
Himmel), kann es aber nicht aktivieren, solange kein CAT063 auf dem Draht liegt.
Dieses ADR räumt den Blocker.

## Entscheidung

1. **Neue Kategorie CAT063 (Sensor Status).** Firefly sendet periodisch einen
   CAT063-Datenblock mit **einem Record je registriertem Sensor**. Jeder Record
   trägt das Subset der echten EUROCONTROL-CAT063-UAP, das einen periodischen
   Per-Sensor-Status ausmacht: I063/010 (SAC/SIC des Sensors), I063/030 (Time of
   Day, 1/128 s wie I062/070) und I063/060 (Sensor Configuration & Status;
   NOGO-Feld operationell/degradiert). FSPEC `0xE0`.

2. **Gleiche Multicast-Gruppe/Port wie CAT062/CAT065** (`239.255.0.62:8600`).
   Der Strom bleibt selbstbeschreibend: Ein Konsument dispatcht am führenden
   **CAT-Oktett** (`0x3E` → Track, `0x41` → Heartbeat, `0x3F` → Sensor-Status).
   Das entspricht der Praxis realer SDPS-Ausgaben (ARTAS, Phoenix), hält die
   Multicast-Topologie auf **einer** Gruppe (eine Firewall-Regel, eine
   Vertrauensgrenze) und lässt **ADR 0017 unverändert** gelten. Konsistent mit
   ADR 0018.

3. **Per-Sensor-Liveness aus dem Plot-Eingang.** Ein neuer
   `SensorHealthMonitor` (`firefly-multicast`) verfolgt je Sensor die
   Wall-clock-Zeit des letzten Plot-Batches. Ein Sensor gilt als **aktiv**,
   solange sein letzter Plot innerhalb von `2.5 × scan_period` Sekunden liegt;
   sonst **degradiert** (NOGO `0x40`). Der Faktor 2,5 gibt einem Sensor mehr als
   zwei volle Antennenumläufe Zeit, bevor er als ausgefallen gilt — robust gegen
   einzelne ausgelassene Scans (Pd < 1), ohne einen echten Ausfall lange zu
   verschleiern.

4. **Zwei Betriebsmodi.**
   - **Replay (deterministisch):** alle Sensoren der Szene werden als dauerhaft
     aktiv vorbelegt (`new_replay`, 1-h-Timeout). Eine deterministische
     Wiedergabe meldet **keine** Degradierung — sie würde sonst von der Wanduhr
     abhängen und die Reproduzierbarkeit (ADR 0003) verletzen.
   - **Live (echtzeit):** der OpenSky-Poller meldet jeden Plot-Batch per
     `record_activity` an den Monitor; die Liveness folgt dem echten Eingang.

5. **Wall-clock-getaktet, nicht datenzeit-getaktet.** Wie der CAT065-Heartbeat
   ist der Sensor-Status ein **Echtzeit-Lebenssignal** und wird im
   **wall-clock-Takt** gesendet (Default **5 s**, `FIREFLY_CAT063_PERIOD`),
   unabhängig vom datenzeit-getakteten Track-Strom, und stempelt I063/030 mit der
   tatsächlichen UTC-Tageszeit. Kein Bruch der Determinismus-Regel (ADR 0003):
   der Status trägt keine Track-Daten und berührt den Tracker-Kern nicht; die
   wall-clock-Lesung lebt allein am Zustell-Rand (`firefly-multicast`). Die
   5-s-Periode ist langsamer als der 1-s-Heartbeat, weil sich Sensor-Liveness auf
   der Zeitskala der Antennenumläufe (4–12 s) ändert, nicht im Sekundentakt.

6. **Eigener, entkoppelter Sende-Task.** `firefly-multicast::run_cat063_sender`
   läuft als separater Tokio-Task mit eigenem Sender-Socket, neben CAT062- und
   CAT065-Sender. Er ist **an, sobald Feed (`FIREFLY_CAT062_ENABLED`) und
   Heartbeat (`FIREFLY_CAT065_ENABLED`, Default an) laufen** — kein eigener
   Enable-Schalter, weil Per-Sensor-Status und Heartbeat denselben Zweck (Feed-/
   Sensor-Liveness) bedienen und gemeinsam wirken sollen.

7. **SAC-Konvention.** Alle lokalen Firefly-Sensoren tragen **SAC = 0**; die
   **SIC** identifiziert den einzelnen Sensor (Frankfurt-Szene: 1/2/3;
   Demo/Live: 1). Das hält die Sensor-Identität in I063/010 stabil und von der
   Feed-Identität (I062/010 SAC/SIC des SDPS) getrennt.

## Konsequenzen

**Positiv:**
- Wayfinder kann „Sensor ausgefallen" von „leerer Himmel" unterscheiden und sein
  gelbes Degradierungs-Banner aktivieren — der Blocker ist geräumt.
- Drei klar getrennte Liveness-Ebenen: CAT062 (Tracks), CAT065 (SDPS lebt),
  CAT063 (welche Sensoren speisen). Jede beantwortet eine eigene betriebliche
  Frage.
- Standardtreue (echte CAT063-UAP); ein konformer Drittkonsument liest den
  Sensor-Status ohne privates Profil.
- Keine Kopplung an den Tracker-Kern; reiner Zustell-Rand, deterministischer
  Replay unberührt.

**Zu beachten (Kompatibilität):**
- Der gemeinsame Strom enthält jetzt **drei** Kategorien. Ein Konsument **muss**
  am CAT-Oktett dispatchen und unbekannte Kategorien überspringen — die
  robuste-Decoder-Regel verlangte das ohnehin. Darum: **additiv (ICD 2.5.0)**.
- Die Degradierungs-Erkennung ist wall-clock-basiert und damit nur im **Live-
  Modus** aussagekräftig; im Replay sind alle Sensoren per Definition aktiv. Das
  ist gewollt (Determinismus), aber eine ehrliche Grenze: ein im Replay
  aufgezeichneter Sensor-Ausfall wird **nicht** als CAT063-Degradierung
  wiedergegeben.
- Das NOGO-Feld kennt in der UAP vier Zustände (operationell / degradiert /
  nicht verbunden / nicht initialisiert). Firefly nutzt heute nur die ersten
  beiden (`0x00` / `0x40`); die feinere Differenzierung (z. B. „nicht verbunden"
  bei nie gesehenem Sensor) ist ein möglicher Folgeschritt — die Kodierung steht.

## Rückverfolgbarkeit

- **Anforderungen:** FR-IO-007 (CAT063-Encoder/Decoder), FR-NET-010 (CAT063-
  Multicast-Sender + SensorHealthMonitor) im Register
  (`docs/requirements/README.md`).
- **Code:** `firefly-asterix::cat063` (Encoder/Decoder),
  `firefly-multicast::sensor_health` (`SensorHealthMonitor`),
  `firefly-multicast::cat063_sender` (`run_cat063_sender`),
  `firefly-server::main` (`spawn_cat063_sensor_sender`, Replay/Live-Verdrahtung).
- **ICD:** `docs/ICD-CAT062.md` Abschnitt 9, Changelog 2.5.0.
- **Cross-Project:** Firefly-Issue #32 (`from-wayfinder`);
  `docs/cross-project/todo-for-firefly.md`.
