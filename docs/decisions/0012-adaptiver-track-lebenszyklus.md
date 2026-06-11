# ADR 0012 — Adaptiver Track-Lebenszyklus (Revisit-Intervall × Feed-Kadenz)

- **Status:** akzeptiert
- **Datum:** 2026-06-11

## Kontext

Mit der zentralen Mess-Fusion (ADR 0010) liefern mehrere Radare ihre Scans zu
**unterschiedlichen, versetzten Zeiten** (`scan_offset`): Radar 1 scannt bei
`t = 0, 4, 8, …`, Radar 2 bei `t = 1.3, 5.3, 9.3, …`, Radar 3 bei
`t = 2.6, 6.6, 10.6, …`. Der Tracker bekommt also nicht mehr alle 4 s eine
Sammel-Lieferung, sondern alle ~1.3 s einen Plot-Schwung von irgendeinem Radar.

Der bisherige Lebenszyklus (M2) zählte **Misses pro Scan-Aufruf**: ein
Aufruf ohne Treffer für einen Track = ein Miss, und nach `delete_misses_*`
solchen Aufrufen in Folge wird der Track gelöscht. Bei versetzten Radaren
bedeutet das: ein Track, den nur Radar 1 sieht, gilt bei *jedem* Aufruf von
Radar 2 oder Radar 3 (die ihn gar nicht sehen können) als „Miss" — er sammelt
in 4 s drei Misses statt null. Reproduzierbar führte das zu **massivem
Track-Churn**: in einer ersten Messung mit den obigen Versätzen entstanden
über den Lauf 28 statt der erwarteten 8 Track-IDs (die meisten Flugzeuge
wurden alle paar Sekunden gelöscht und unter neuer ID neu geboren).

Der naheliegende Fix — Löschung nach **verstrichener Zeit** statt nach
Miss-*Anzahl* (`update_age > Timeout`) — würde jedoch **NFR-CLOUD-004**
verletzen: dessen Test
`timing::deletion_is_governed_by_miss_count_not_elapsed_time` verlangt, dass
ein Track nach derselben Anzahl Misses gelöscht wird, egal ob diese 4 s oder
100 s auseinanderliegen (eine langsame/verzögerte Quelle darf Tracks nicht
vorschnell killen).

## Entscheidung

**Adaptiver Lebenszyklus**: Lebenszyklus-Fenster (Bestätigung, Löschung) werden
weiterhin in **Miss-Budgets** (`confirm_n`, `delete_misses_*`) gezählt, aber
das Zeitintervall, das ein „Miss" repräsentiert, ist nicht mehr fest 1 Scan,
sondern eine pro Track adaptiv geschätzte **Referenzdauer**
(`coast_reference`):

```
coast_reference = max(revisit_interval, cadence)
```

- **`revisit_interval`** — ein EWMA (`REVISIT_EWMA = 0.5`) der Zeitlücken
  zwischen den tatsächlichen *Treffern* (`mark_hit`) dieses Tracks. Für einen
  Track, der von mehreren Radaren oft getroffen wird, ist das kurz; für einen
  Track in nur einem Radar ist das ungefähr dessen Scan-Periode (hier 4 s).
- **`cadence`** — eine vom `Tracker` geführte, feed-weite Schätzung: das
  Maximum aus der Lücke seit dem letzten Scan-Aufruf und der größten je
  beobachteten Scan-Periode eines einzelnen Sensors (`sensor_period`, je
  Sensor `t − sensor_last_scan[sensor]`). Sie verhindert, dass ein frisch
  geborener Track (dessen `revisit_interval` noch `0` ist) zwischen zwei
  Scans *verschiedener* Sensoren (kurze Lücke) sofort als „lange überfällig"
  gilt.

Damit wird:

- **Bestätigung** (Schritt 4): `hits_within(confirm_n · coast_reference, t) ≥
  confirm_m` — „mindestens `confirm_m` Treffer innerhalb der letzten
  `confirm_n` Revisit-Intervalle", statt eines festen Scan-Fensters.
- **Löschung** (Schritt 5): `update_age ≥ delete_misses_* · coast_reference` —
  „so viele Revisit-Intervalle ohne Treffer wie das Budget erlaubt".

### Bootstrap-Sonderfall

Bevor **irgendein** Sensor seinen zweiten Scan abgeliefert hat, ist weder
`revisit_interval` (frisch geborener Track) noch `sensor_period` (noch keine
zwei Scans desselben Sensors) bekannt. In diesem kurzen Fenster wäre die
einzig verfügbare Zahl die Lücke seit dem *letzten* Scan-Aufruf — bei
versetzten Radaren z. B. `1.3 s`, obwohl die wahre Periode `4 s` beträgt. Ein
Track, der in diesem Fenster geboren wird und vom *nächsten* (anderen)
Sensor naturgemäß nicht gesehen wird, würde sofort gelöscht, bevor sein
eigener Sensor überhaupt wieder scannt.

Deshalb gilt: Bringt ein Scan Plots, aber **kein** Sensor hat bisher eine
Periode geliefert (`sensor_period` leer), ist `cadence = ∞` — in diesem Fall
wird **nichts** gelöscht (`age ≥ budget · ∞` ist für endliches `age` immer
falsch). Ein Scan **ohne** Plots (reines Coasting, z. B. ein einzelner Sensor
ohne Daten) fällt nicht unter diese Sonderregel und nutzt weiterhin die
Lücke seit dem letzten Aufruf — das hält den bestehenden
Single-Sensor-Lebenszyklus (M2) unverändert.

## Konsequenzen

**Positiv**

- Async-Multi-Radar-Churn behoben: dieselbe Frankfurt-Szene mit
  `scan_offset = 0 / 1.3 / 2.6` läuft jetzt mit **8** statt 28 Track-IDs, exakt
  wie die synchrone Variante (`scan_offset = 0` für alle).
- NFR-CLOUD-004 bleibt erfüllt: Bei einem einzelnen, regelmäßig scannenden
  Sensor ist `coast_reference` konstant (≈ Scan-Periode), also entspricht das
  Miss-Budget weiterhin einer festen *Anzahl* Scans, unabhängig vom
  Zeitabstand (`timing::deletion_is_governed_by_miss_count_not_elapsed_time`
  bleibt grün).
- Verzögerte Einzel-Scans (NFR-CLOUD-004,
  `timing::long_gap_with_data_keeps_track_identity`) bleiben unverändert
  robust — `revisit_interval` adaptiert sich an die tatsächliche
  Wiederkehr-Rate.

**Negativ / Grenzen**

- Ein Track, der **nie** wieder gesehen wird und im Bootstrap-Fenster (vor dem
  zweiten Scan irgendeines Sensors) geboren wurde, wird erst gelöscht, sobald
  `sensor_period` für irgendeinen Sensor bekannt ist — eine Verzögerung von
  höchstens einer Sensor-Periode, einmalig zu Beginn des Laufs.
- `coast_reference` ist pro Track ein EWMA, kein hartes Limit; ein Track, der
  zufällig einmal sehr spät (aber noch innerhalb seines Budgets) wieder
  getroffen wird, „erbt" dadurch ein größeres `revisit_interval` und damit
  großzügigere künftige Fenster. Das ist im Sinne von NFR-CLOUD-004
  (Robustheit gegen Verzug) gewollt.
