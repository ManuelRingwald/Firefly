# HA.1 — Zustands-Snapshot + Wiederanlauf

> **Anforderung:** FR-TRK-049 · **ADR:** 0040 · **ICD:** unberührt ·
> **Einstufung:** S3–S4 · umgesetzt auf Fable 5

## Fachlich

Nach einem Absturz oder Neustart war Firefly bisher minutenlang blind:
Jeder Track musste sich über mehrere Antennen-Umläufe neu bestätigen, und
die manuellen Korrelations-Pins des Lotsen (FPL.2) waren weg. Jetzt
sichert Firefly seinen Arbeitszustand regelmäßig und liest ihn beim Start
wieder ein — **das Luftlagebild ist nach einem Neustart binnen eines
Output-Ticks zurück**, samt Track-Nummern, Identitäten und Pins. Das ist
der erste Baustein von SDPS-002 (Verfügbarkeit) und der Grundstein für
HA.2 (Main/Standby: Wer seinen Zustand serialisieren kann, kann ihn auch
synchronisieren).

## Technik

- **Snapshot** (`firefly-server::snapshot`): versioniertes JSON-Envelope —
  Tracker-Kern (Tracks, IMM-Filterzustände, Nummern-Pool, Clutter-Karten),
  letzte Datenzeit, manuelle Pins, Konfigurations-Fingerprint. Kadenz
  `FIREFLY_SNAPSHOT_PERIOD` (Default 10 s, wall-clock) auf
  `FIREFLY_SNAPSHOT_PATH` (unset = aus; kaputte Werte = Start-Fehler).
- **Atomar:** `.tmp`-Geschwister + fsync + rename — ein Absturz mitten im
  Schreiben hinterlässt den letzten guten Snapshot, nie eine zerrissene
  Datei. Schreibfehler: WARN + Zähler + Wiederversuch (Verfügbarkeit vor
  Sicherung; stilles Aufgeben wäre stiller Verlust der
  Wiederanlauf-Fähigkeit).
- **Drei Restore-Torwächter** (jede Ablehnung laut, dann leerer Start):
  Format-Version (Layout-Brüche wie VERT.4bs 6-D-Umbau ⇒ Bump statt
  stillem Fehl-Parse), **Konfigurations-Fingerprint** (Referenzpunkt +
  Sensor-Liste — ein Zustand für eine andere Quell-Konfiguration wird nie
  wiederbelebt; fängt den klassischen Betriebsfehler „Restart mit
  geänderter `FIREFLY_SOURCES`"), Alter ≤ `FIREFLY_SNAPSHOT_MAX_AGE`
  (Default 300 s — veralteter Verkehr ist gefährlicher als ein leerer
  Schirm).
- **Restore:** Tracker ersetzt, Datenzeit gesetzt (nächster Output-Tick
  publiziert das Bild — vor dem ersten Plot), Pins in die geteilte
  Override-Karte. `/ready` bleibt an den ersten Quell-Plot gekoppelt
  (Feed-Liveness ≠ Bild-Verfügbarkeit).
- **Metriken:** `firefly_snapshot_writes_total`/`_errors_total`,
  `firefly_snapshot_age_seconds`, `firefly_restore`.

## Ehrliche Grenzen

- Plots zwischen letztem Snapshot und Absturz sind **verloren** (Fenster
  ≤ Periode). Forensisches Replay bleibt Sache des Input-Recordings
  (`.ffplots`, ADR 0020).
- In Kubernetes braucht der Pfad ein **persistentes Volume** — sonst ist
  der Snapshot mit dem Pod weg (Deployment-Sache, HA.3).
- Kein Restore von Metrik-Zählerständen (Prometheus-Standard bei
  Prozess-Neustart).
- Main/Standby (unterbrechungsfreier Übergang, State-Sync) ist **HA.2** —
  dieser Baustein deckt den Einzel-Instanz-Neustart.
