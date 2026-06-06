# ADR 0003 — Cloud-native Architektur (anbieter-neutral)

- **Status:** akzeptiert
- **Datum:** 2026-06-06

## Kontext

Heute wird beim Projektverantwortlichen ein **Legacy-Produkt in einen Container
gepackt** („Lift & Shift") und in der Cloud betrieben — mit nur mittelmäßigem
Ergebnis. Grund: Solche Systeme nehmen eine feste Maschine, einen langlebigen
Prozess und lokalen Zustand an und „kämpfen" deshalb gegen die Cloud.

Firefly soll **von Grund auf cloud-nativ** sein: gebaut für eine Welt, in der
Recheninstanzen kommen und gehen, in der horizontal skaliert wird und in der
Ausfälle normal sind. Die Besonderheit: Ein Tracker ist **zustandsbehaftet** —
Tracks leben über viele Scans hinweg. Genau das macht cloud-native Statefulness
zur eigentlichen Herausforderung.

## Entscheidung

Zielplattform ist **Kubernetes, anbieter-neutral** (läuft ebenso auf einer
souveränen/On-Prem-Private-Cloud). Wir verankern folgende Prinzipien:

1. **Deterministische Verarbeitung nach Datenzeit (Event-Time).**
   Der Tracker rechnet entlang der Zeitstempel *in den Radardaten*, nicht entlang
   der Server-Uhr. Gleicher Eingangsstrom ⇒ exakt gleicher Track-Ausgang. Folgen:
   - **Wiederherstellbarkeit:** Ein abgestürzter Knoten rekonstruiert seinen
     Zustand durch erneutes Abspielen (Replay) des Plot-Stroms ab dem letzten
     Snapshot.
   - **Testbarkeit & Audit-Reproduzierbarkeit:** Ein Vorfall lässt sich bit-genau
     nachstellen.
   (In M1 bereits angelegt: reproduzierbarer Seed, `Timestamp`.)

2. **Explizit verwalteter, wiederherstellbarer Zustand.**
   Track-Zustand wird nicht „im Prozess versteckt", sondern als klar
   definierter, serialisierbarer Zustand geführt, der periodisch gesichert
   (Snapshot) und repliziert werden kann.

3. **Entkopplung über einen Datenstrom (Message Bus).**
   Sensoren → Bus → Tracker → Bus → Anzeige. Erlaubt Skalierung, Replay,
   Lastpuffer (Back-Pressure) und sauberes Verhalten bei Ausfall. Die konkrete
   Technologie (z. B. NATS/Kafka) wird in einem späteren ADR gewählt, wenn M3 ihn
   braucht — die *Schnittstelle* halten wir vorher abstrakt.

4. **Partitionierbare Last.**
   Aufteilbar nach Sensor / geografischem Sektor; Track-Übergaben an
   Sektorgrenzen werden später explizit modelliert.

5. **Betriebs-Selbstverständlichkeiten:** Konfiguration über Umgebung
   (12-Factor), Health-/Readiness-Probes, sauberes Herunterfahren, kleine
   Container-Images.

6. **Observability als Pflicht:** strukturierte Logs, Metriken, Tracing — dient
   zugleich als Nachweismaterial für Audits (Brücke zu ADR 0004).

## Begründung

- Rust passt hervorragend: schneller Kaltstart, geringer Speicherbedarf, **keine
  Garbage-Collection-Pausen** → vorhersagbare Latenz in einer „lauten" Cloud,
  wichtig für ein soft-echtzeitfähiges Überwachungssystem.
- Determinismus dient gleich drei Zielen: Cloud-Resilienz (Replay/Recovery),
  Testbarkeit und Zertifizierungs-Nachweis.
- Anbieter-Neutralität schützt vor Lock-in und respektiert mögliche
  Datensouveränitäts-Vorgaben eines ANSP.

## Konsequenzen

- Der Tracker-Kern wird so entworfen, dass „ein Schritt" = „Zustand + neuer
  Plot-Batch → neuer Zustand + Track-Ausgabe" eine **reine, deterministische
  Funktion** ist. Seiteneffekte (Netz, Uhr, Logging) bleiben außen.
- Wir brauchen später ein Snapshot-/Replay-Konzept und eine
  Bus-Abstraktion (ab M3).
- Wanduhr-Zeit darf die Rechenlogik nie beeinflussen (nur Datenzeit).
