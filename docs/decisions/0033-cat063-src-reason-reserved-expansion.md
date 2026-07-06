# ADR 0033 — CAT063 per-Quelle-Fehlergrund (`SRC-REASON` im I063/RE)

- **Status:** akzeptiert
- **Datum:** 2026-07-06
- **Schnittstellen-relevant:** ja (CAT063-Ausgabe-Vertrag, ICD → 3.1.0, **additiv**)
- **Auslöser:** Wayfinder-Issue #197 (`from-wayfinder`) — die Feed-Health-UI zeigt
  bei degradiertem Feed nur **dass**, nicht **warum**. Aufbauend auf ADR 0032
  (CAT063-UAP-Standardisierung), das das RE-Feld erst standardkonform ermöglicht.

## Kontext

Seit ADR 0022 meldet CAT063 pro Sensor **operationell/degradiert** (I063/060 CON).
Das reicht dem Betreiber nicht: Eine degradierte ADS-B-Quelle kann drei sehr
unterschiedliche Ursachen haben, die **unterschiedliche Reaktionen** verlangen:

- **unreachable** — Netz/Firewall/DNS/Timeout. Die Credentials sind in Ordnung;
  das Problem ist die Erreichbarkeit (genau die Codespaces-Diagnose vom
  2026-07-05: OpenSky verwirft Datacenter-IPs).
- **auth** — HTTP 401/403, falsche oder fehlende Credentials. Hier — und **nur**
  hier — hilft ein Nachtippen der Zugangsdaten.
- **rate_limited** — HTTP 429, Drosselung. Warten/Intervall erhöhen hilft.

Ohne diese Unterscheidung tippt der Betreiber im Zweifel sinnlos Credentials nach,
obwohl das Problem ein Firewall-Egress ist. CAT063 soll den Grund **im Strom**
mitliefern, damit die ASD-UI ihn anzeigen kann.

ADR 0032 hat die CAT063-UAP zuvor auf die echten EUROCONTROL-FRN-Positionen
gebracht — Voraussetzung dafür, den Grund im **standardkonformen** Reserved
Expansion Field (RE, FRN 13) statt in einer verbogenen UAP zu tragen.

## Entscheidung

1. **Träger: I063/RE (Reserved Expansion Field, FRN 13).** RE ist der vom
   EUROCONTROL-Standard vorgesehene, **selbst-begrenzende** Erweiterungs-Slot für
   vendor-spezifische Subfelder. Firefly definiert genau ein Subfeld:

   ```
   [LEN = 0x03] [SUBFIELD_SPEC = 0x80] [SRC-REASON]
   ```

   `LEN` zählt das ganze Feld inkl. sich selbst; `SUBFIELD_SPEC` markiert per
   Bit 8 (`0x80`) das `SRC-REASON`-Subfeld (Bit 1 = FX = 0); `SRC-REASON` ist ein
   u8: `1 = unreachable`, `2 = auth`, `3 = rate_limited` (`0 = ok` wird nie
   gesendet).

2. **Nur bei degradiertem Sensor mit bekanntem Grund.** Ein operationeller Sensor
   trägt **kein** RE-Feld (Record bleibt 9 Oktette, FSPEC `0xB8`). Ein degradierter
   Sensor **ohne** klassifizierten Grund (still gewordene FLARM-/Radar-Quelle)
   trägt ebenfalls keins. Nur wenn beides zutrifft, wächst die FSPEC auf `0xB9 0x04`
   und das RE-Feld wird angehängt. So bleibt der häufige Fall schlank und additiv.

3. **Grund-Ableitung aus den Poller-Fehlerpfaden.** Die beiden HTTP-ADS-B-Poller
   (`firefly-opensky`, `firefly-adsbagg`) klassifizieren ihre `PollError` über
   `is_rate_limited()` (HTTP 429) und das neue `is_auth()` (HTTP 401/403);
   alles andere (DNS/Connect/Timeout/5xx) ist `unreachable`. Der Server ruft bei
   jedem Poll-Fehler `SensorHealthMonitor::record_failure(sensor_id, reason)`;
   ein erfolgreicher Plot setzt den Grund via `record_activity` auf `Ok` zurück.
   Der `SensorHealthMonitor` führt den Grund pro Sensor mit; die
   `SensorHealthSnapshot` liefert ihn an den CAT063-Sender.

4. **Reason-Typ in `firefly-asterix`.** `SensorReason` (Ok/Unreachable/Auth/
   RateLimited, mit `code()`/`from_code()`) lebt neben dem CAT063-Encoder — es ist
   ein Wire-Konzept. `Cat063Encoder::encode` nimmt `&[SensorReport { sic,
   operational, reason }]`; `DecodedSensorStatus` trägt den dekodierten `reason`.

## Begründung

- **Standard-Weg statt privates Feld.** RE ist der sanktionierte Vendor-Escape
  der ASTERIX-UAP; wir dehnen weder ein bestehendes Item noch erfinden wir eine
  FRN. Ein konformer Fremd-Decoder überspringt RE über sein Längen-Oktett.
- **Additiv, kein Bruch.** Dank der RE/SP-Längen-Toleranz aus ADR 0032 (Wayfinder
  ADR 0019) bricht der bestehende Wayfinder-Decoder nicht — er ignoriert das
  RE-Feld, bis H4 es auswertet. Deshalb ist H3 (dieser ADR) **kein** Breaking
  Change, obwohl er unmittelbar auf einem Breaking (3.0.0) aufsetzt.
- **Grund an der Quelle klassifiziert.** Die Fehler-Semantik (429 → rate_limit,
  401/403 → auth, sonst unreachable) lebt in den Poller-Crates, die den HTTP-
  Status kennen; der Server macht nur das triviale Mapping. Rückverfolgbar und
  unit-testbar (`is_auth`/`is_rate_limited`).

## Konsequenzen

- **ICD 3.1.0 (additiv).** Neuer Abschnitt-9-Unterpunkt „I063/RE — SRC-REASON" +
  byte-genauer Referenz-Dump. **Wayfinder H4** wertet das RE-Feld aus und zeigt
  den Grund im Feed-Health-Chip → schließt Wayfinder #197. Deploy-Kopplung: H4
  kann jederzeit nach H3 folgen (rein additiv, kein Lockstep-Zwang).
- `firefly-asterix`: neuer `SensorReason` + `SensorReport`; `encode`-Signatur
  und `DecodedSensorStatus` erweitert.
- `firefly-multicast`: `SensorHealthMonitor` führt pro Sensor einen Grund
  (`record_failure`); `SensorHealthSnapshot.per_sensor` trägt `SensorHealth
  { active, reason }`.
- `firefly-opensky`/`firefly-adsbagg`: neues `PollError::is_auth()`.
- `firefly-server`: klassifiziert Poll-Fehler und ruft `record_failure`; kein
  neues Env, keine neuen Metriken (die bestehenden `*_poll_errors_total`/
  `*_rate_limited_total` bleiben).
- Anforderung **FR-IO-007** erweitert (RE/SRC-REASON, Grund-Ableitung).

## Ehrliche Grenze

Nur die **HTTP-ADS-B-Poller** (OpenSky, adsb_aggregator) klassifizieren heute
einen Grund. **FLARM** (APRS-IS) und **Radar** (CAT048/UDP) reconnecten intern und
melden dem Server keinen Pro-Versuch-Fehler; ein von ihnen degradierter Sensor
trägt daher **keinen** Grund (kein RE-Feld) — die UI zeigt „degradiert, Grund
unbekannt". Das ist ehrlich (wir raten keinen Grund), aber eine mögliche
Folgearbeit, sobald diese Adapter einen Fehler-Callback exponieren. Ebenso ist
`auth` für die auth-freien Aggregatoren in der Praxis nicht erreichbar — die
Klassifikation ist dennoch symmetrisch implementiert.
