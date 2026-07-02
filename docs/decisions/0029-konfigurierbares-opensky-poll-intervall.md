# ADR 0029 — Konfigurierbares OpenSky-Poll-Intervall pro Quelle (`poll_interval_secs`)

- **Status:** akzeptiert
- **Datum:** 2026-07-02
- **Schnittstellen-relevant:** Eingangs-Kontrakt — `adsb_opensky` bekommt ein
  optionales Feld `poll_interval_secs`; `docs/source-input-contract.md` → **v1.4.0**
  (additiv). Der **Ausgabe**-Vertrag (CAT062/UDP) bleibt unberührt. **Kein**
  CAT062-ICD-Eingriff.
- **Auslöser:** Wayfinder-E2E-Lauf — ein anonym pollender OpenSky-Feed lief in
  **HTTP 429** (Rate-Limit). Das Poll-Intervall war fest bei 10 s (Firefly-Default)
  und für den Betreiber **nicht** einstellbar. Wayfinder-Wunsch #3 (Poll-Schutz):
  Poll-Intervall pro Quelle konfigurierbar + Betreiber-Infobox.

## Kontext

Firefly trackt live (ADR 0020) aus einer env-getriebenen Quell-Liste (ADR 0023).
Der **OpenSky-Adapter** (ADR 0019) pollt die REST-API `states/all` in festen
Intervallen; das Intervall ist per `OpenSkyConfig::poll_interval_secs`
(Env `FIREFLY_OPENSKY_POLL_INTERVAL_SECS`, Default 10 s) konfigurierbar — **aber
nur** auf dem diskreten Standalone-/Dev-Pfad, nicht über den `FIREFLY_SOURCES`-
Kontrakt, den der Orchestrator (Wayfinder) benutzt. Setzt der Orchestrator
`FIREFLY_SOURCES`, hat dieses Vorrang und die diskreten `FIREFLY_OPENSKY_*`-Envs
werden **nicht** ausgewertet (Kontrakt §1). Damit hatte der Betreiber über das
Wayfinder-UI **keine** Möglichkeit, die Poll-Kadenz zu steuern.

**Fachliche Bedeutung.** OpenSky rate-limitet: anonym ~1 Anfrage/10 s,
authentifiziert (OAuth2 Client-Credentials, ADR 0024) ~1/5 s. Ohne Steuerung
läuft ein anonymer Feed unweigerlich in 429; umgekehrt kann ein authentifizierter
Feed schneller pollen, als der 10-s-Default erlaubt. Die Kadenz gehört daher in
die Hand des Betreibers — pro Quelle, weil ein Feed mehrere Quellen mit
unterschiedlicher Auth tragen kann.

**Nur OpenSky.** Ein „Poll-Intervall" ist ein Konzept **gepollter** Quellen.
`flarm_aprs` ist ein **Push**-Strom (APRS-IS, nominale 5-s-Scan-Periode,
`FLARM_NOMINAL_SCAN_SECS`); `radar_asterix` hat eine eigene, sensor-getriebene
Scan-Periode. Für beide wäre das Feld bedeutungslos — es gilt ausschließlich für
`adsb_opensky`.

## Entscheidung

### 1. Additives, optionales Feld `poll_interval_secs` im Quell-Spec

`SourceSpec` bekommt `poll_interval_secs: Option<u64>` (`#[serde(default)]`). Der
Feldname ist identisch zu Wayfinders `source_config`, sodass der Orchestrator
nahezu pass-through serialisiert. Weil `SourceSpec` **kein** `deny_unknown_fields`
trägt, ist die Änderung in **beide** Richtungen kompatibel: ein älterer Firefly
ignoriert ein gesetztes Feld, ein neuer Firefly nimmt bei fehlendem Feld den
Default — die Merge-Reihenfolge der beiden Repos ist damit entkoppelt.

### 2. `0`/fehlend → Default; nur `> 0` überschreibt

`opensky_config_from_spec` übernimmt `poll_interval_secs` nur, wenn es **> 0** ist;
`0` oder `None` behält den `OpenSkyConfig`-Default (10 s). Das spiegelt exakt die
bestehende Env-Logik (`OpenSkyConfig::from_env` verwirft `0`) und verhindert eine
Heiß-Lauf-Poll-Schleife durch einen versehentlichen `0`-Wert. Eine **Obergrenze**
erzwingt Firefly bewusst **nicht** — Firefly bleibt tolerant (unset/ungültig →
Default); die sinnvolle Bereichsgrenze (5–3600 s) setzt Wayfinder am Schreib-Rand
seines Admin-UI (dort, wo der Betreiber den Wert eingibt und die Infobox steht).

### 3. Ausgabe-Kadenz zieht automatisch nach

`representative_config` bestimmt den Ausgabetakt des Trackers als das **Minimum**
über die Poll-/Scan-Intervalle aller Quellen. Ein per `poll_interval_secs`
gesetztes OpenSky-Intervall fließt darüber ohne Extra-Code in die Ausgabe-Kadenz
ein.

## Konsequenzen

- **Positiv:** Der Betreiber kann die OpenSky-Poll-Kadenz pro Quelle steuern und
  das Rate-Limit respektieren (429-Vermeidung) bzw. authentifiziert schneller
  pollen — ohne Code-Änderung, rein über das Wayfinder-UI.
- **Additiv & kompatibel:** kein Wire-Bruch; ältere/neuere Leser koexistieren.
- **Ehrliche Grenze:** Das Feld steuert nur die **Kadenz**. Ein echter
  exponentieller **Backoff** bei 429 (statt fixem Intervall) ist ein separater
  Härtungsschritt (Wayfinder #2 / Firefly-Todo) und **nicht** Teil dieses ADR.
- **Rückverfolgbarkeit:** Anforderungs-Register (`docs/requirements/`),
  Kontrakt-Changelog (v1.4.0) und Cross-Project-Todo für Wayfinder aktualisiert.

## Alternativen

- **Nur global per Env (Status quo):** Vom Orchestrator-Pfad (`FIREFLY_SOURCES`)
  gar nicht erreichbar — verworfen, löst das Problem nicht.
- **Für alle Quelltypen zulassen:** bedeutungslos für Push-/Scan-Quellen und
  irreführend im UI — verworfen; Feld bleibt OpenSky-spezifisch.
- **Obergrenze in Firefly erzwingen:** widerspricht der tolerant-Default-Philosophie
  des Adapters; die Bereichsvalidierung gehört an den Wayfinder-Schreib-Rand.
