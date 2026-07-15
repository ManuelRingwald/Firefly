# ADR 0040: Zustands-Snapshot + Wiederanlauf (HA.1)

**Status:** akzeptiert (2026-07-15, Betreiber-Go HA.1) · **Bezug:**
ARTAS-Gap-Roadmap AP-HA (SDPS-002), ADR 0003 (Cloud-nativ: Zustand
explizit und wiederherstellbar), ADR 0020 (Input-Recording `.ffplots`),
ADR 0039 (manuelle Korrelations-Pins, bisher flüchtig), FR-TRK-049

## Kontext — in normaler Sprache

Stürzt Firefly ab oder wird neu gestartet, beginnt das Luftlagebild bei
Null: Jeder Track muss sich über mehrere Antennen-Umläufe neu bestätigen —
je nach Sensorik eine Minute und mehr, in der der Lotse ein leeres oder
unreifes Bild sieht. Ein SDPS darf das nicht (SDPS-002). Außerdem gingen
bisher die manuellen Korrelations-Pins (FPL.2) bei jedem Neustart verloren
— eine Lotsen-Entscheidung, die stillschweigend verschwindet.

## Entscheidung

1. **Periodischer Zustands-Snapshot.** Der Live-Tracker schreibt seinen
   Arbeitszustand — Tracker-Kern (Tracks, IMM-Filterzustände,
   Track-Nummern-Pool, Clutter-Karten), letzte Datenzeit und die manuellen
   Korrelations-Pins — als **versioniertes JSON** auf einen konfigurierten
   Pfad (`FIREFLY_SNAPSHOT_PATH`; unset = aus). Kadenz wall-clock
   (`FIREFLY_SNAPSHOT_PERIOD`, Default 10 s), geprüft je Output-Tick.
2. **Atomares Schreiben.** Temp-Datei (`.tmp`-Geschwister) + `fsync` +
   `rename`: Ein Absturz mitten im Schreiben hinterlässt den letzten
   guten Snapshot, nie eine zerrissene Datei. Ein Schreibfehler ist
   **nicht fatal** (Verfügbarkeit vor Sicherung, wie beim Plot-Recorder) —
   WARN + Fehlerzähler, und es wird **weiter versucht** (eine volle Platte
   kann wieder frei werden; anders als beim Recorder wäre stilles Aufgeben
   hier ein stiller Verlust der Wiederanlauf-Fähigkeit).
3. **Strenge Restore-Torwächter.** Beim Start wird ein vorhandener
   Snapshot nur übernommen, wenn **alle drei** Prüfungen bestehen; jede
   Ablehnung ist laut (WARN mit Grund), der Prozess startet dann leer:
   - **Format-Version** (`SNAPSHOT_FORMAT_VERSION`): Layout-Änderungen
     (wie der 6-D-IMM-Bruch aus VERT.4b) erhöhen die Version — nie
     stilles Fehl-Deserialisieren.
   - **Konfigurations-Fingerprint**: Referenzpunkt + vollständige
     Sensor-Liste (IDs, Scan-Perioden, Radar-Standorte/-Fehlermodelle)
     werden in den Snapshot gestempelt und beim Laden verglichen — ein
     Tracker-Zustand, der für eine **andere** Quell-Konfiguration gebaut
     wurde, darf nicht gegen diese wiederbelebt werden.
   - **Alter** (`FIREFLY_SNAPSHOT_MAX_AGE`, Default 300 s, wall-clock):
     ein zu alter Snapshot zeigt dem Lotsen veralteten Verkehr — das ist
     gefährlicher als ein leerer Start.
4. **Restore-Umfang.** Tracker ersetzt, letzte Datenzeit gesetzt (der
   **nächste Output-Tick** publiziert das wiederhergestellte Bild — noch
   vor dem ersten Plot), Pins in die geteilte Override-Karte gefüllt.
   `/ready` bleibt unverändert an den ersten Quell-Plot gekoppelt
   (Feed-Liveness ist eine andere Aussage als Bild-Verfügbarkeit).
5. **Beobachtbarkeit:** `firefly_snapshot_writes_total`/`_errors_total`,
   `firefly_snapshot_age_seconds` (das Verlustfenster eines Neustarts)
   und `firefly_restore` (1 = dieser Prozess startete aus einem Snapshot).

## Begründung

- **JSON statt Binärformat:** Der Zustand ist klein (hunderte Tracks,
  ms-Serialisierung bei 10-s-Kadenz); Lesbarkeit und Diff-Barkeit sind
  für Audit/Diagnose mehr wert als Kompaktheit. Die Serde-Basis des
  Tracker-Kerns existiert seit den Meilensteinen und wird per
  Roundtrip-Test (`PartialEq` auf dem ganzen Tracker) byte-genau belegt.
- **Wall-clock fürs Alter, Datenzeit für die Semantik:** Gemessen wird
  die reale Ausfallzeit; die Tracker-Semantik (Coasting/Löschung nach
  Wiederanlauf) läuft weiter über die Datenzeit (ADR 0003/0013) — alte
  Tracks altern nach dem Restore ehrlich weiter.
- **Fingerprint statt Vertrauen:** Der wahrscheinlichste Betriebsfehler
  ist ein Restart mit geänderter `FIREFLY_SOURCES` — genau der wird
  deterministisch abgefangen.

## Konsequenzen

- Das Snapshot-Format ist **kein** Schnittstellen-Vertrag (kein
  ICD-Bezug); es ist ein internes, versioniertes Betriebsartefakt.
  Layout-Änderungen kosten nur einen Versions-Bump (alter Snapshot wird
  verworfen — akzeptiert, solange es kein HA.2-State-Sync ist).
- Grundstein für **HA.2** (Main/Standby): Wer seinen Zustand
  serialisieren kann, kann ihn auch an eine Standby-Instanz übertragen.
- **Ehrliche Grenzen:** Ein Snapshot ersetzt keine Input-Aufzeichnung
  (Plots zwischen letztem Snapshot und Absturz sind verloren — Fenster
  ≤ Periode); Kubernetes braucht dafür ein persistentes Volume; der
  Restore übernimmt keine Metrik-Zählerstände (Prometheus-Counter starten
  bei 0 — Standard-Verhalten bei Prozess-Neustart).
