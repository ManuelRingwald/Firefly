# Todo für Wayfinder (aus Firefly)

Schnittstellen-Themen, die in Firefly entstehen und Wayfinder-Arbeit auslösen.

> **Quell-Eingangs-Kontrakt ratifiziert (ADR 0023, Antwort auf Wayfinder-Issue
> #35).** Firefly liest die Live-Quellen einer orchestrierten Instanz aus einer
> env-getriebenen **JSON-Liste `FIREFLY_SOURCES`** (Credentials isoliert in
> benannten Cred-Envs, `user:pass`-Format/UX-2, Live via `FIREFLY_MODE=live`);
> `adsb_opensky` ist unterstützt, FLARM/Radar reserviert. Maßgeblich:
> `docs/source-input-contract.md` v1.1.0. **Wayfinder-Folge (ORCH-5, eigene
> Roadmap — kein separates Issue):** Docker-Backend übersetzt `source_config` →
> `FIREFLY_SOURCES` + injiziert die aufgelösten Creds in die Cred-Envs; UI gibt
> je `adsb_opensky`-Quelle zwei Felder, intern ein verschlüsseltes Secret.
> Firefly-Folge (Schritt 2): `FIREFLY_SOURCES`-Parser + Multi-Adapter-Speisung.

> **OpenSky-Auth: OAuth2 statt Basic Auth (ADR 0024, kein separates Issue).**
> OpenSky hat Basic Auth abgeschaltet — Firefly nutzt jetzt OAuth2
> Client-Credentials. Der Cred-Wert ist `client_id:client_secret` statt
> `benutzer:passwort`; der **Wire-Vertrag bleibt** (ein String, ein `:`, Split am
> ersten `:`), Wayfinders Backend (ORCH-5b) ist **nicht** betroffen. **Wayfinder-
> Folge (nur UI):** die zwei Secret-Felder im Admin sollten „Client-ID" /
> „Client-Secret" heißen (statt „Benutzername"/„Passwort"); reiner Label-/Hinweis-
> Wechsel, keine Logik. Teil der ORCH-5-E2E-Vorbereitung.

> **`flarm_aprs`-Adapter unterstützt (ADR 0026, Kontrakt v1.2.0, kein separates
> Issue).** Firefly hat den **zweiten** Live-Quell-Adapter implementiert: FLARM-
> Positionen über OGN/APRS-IS (Schritt A ADR · B Crate `firefly-flarm` · C
> Verdrahtung). Im Kontrakt wechselt `flarm_aprs` von „reserviert" → „unterstützt";
> Cred-Wert `callsign:passcode` (read-only anonym ohne `cred_env`), gleiche
> Ein-String-Form wie `adsb_opensky`. **Additiv** — kein Wire-Format-Bruch.
> **Wayfinder-Folge: keine** — das Docker-Backend serialisiert `flarm_aprs` bereits
> aus `source_config` nach `FIREFLY_SOURCES` (ORCH-5, Vokabular war reserviert).

> **`radar_asterix`-Adapter unterstützt (ADR 0028, Kontrakt v1.3.0, Issue #91).**
> Firefly hat den **dritten und letzten** reservierten Live-Quell-Adapter
> implementiert: ein realer Monoradar über **ASTERIX CAT048 über UDP** (Decoder
> `firefly-asterix::cat048` · Crate `firefly-radar` · Verdrahtung). Damit ist
> **Issue #35 auf Firefly-Seite vollständig** (alle drei Vokabular-Typen haben
> Adapter). Im Kontrakt wechselt `radar_asterix` „reserviert" → „unterstützt" mit
> **neuen Pflicht-Feldern `lat`/`lon`** (Radar-Standort — CAT048 ist polar und
> trägt ihn nicht) und optional `height_m`/`listen` (`group:port`). **Additiv**,
> aber **Wayfinder-Folge nötig (Issue #91):** das Docker-Backend muss für eine
> `radar_asterix`-Quelle künftig `lat`/`lon`/`listen` aus dem `source_config`
> nach `FIREFLY_SOURCES` serialisieren (heute nur `sac`/`sic`).

> **Per-Track-Provenienz: I062/290 Per-Technologie-Alter (ADR 0027, ICD 2.6.0,
> additiv).** Firefly liefert die Track-Herkunft jetzt **autoritativ im Strom**
> statt sie Wayfinders Frontend-Heuristik (`provenance.js`) zu überlassen.
> I062/290 trägt zusätzlich zu PSR (`0x40`) und ES/ADS-B (`0x08`) optional
> **SSR-Age** (`0x20`), **Mode-S-Age** (`0x10`) und **FLARM-Age** (`0x04`,
> Firefly-Vendor-Subfeld); Age-Oktette in Bit-Priorität MSB→LSB. **Damit wird
> FLARM erstmals unterscheidbar.** Strikt additiv — `0x40`/`0x08` unverändert,
> kein Wire-Bruch. **Wayfinder-Folge (Issue #90):** Decoder liest die neuen
> Subfelder, leitet die Provenienz daraus ab (≥ 2 frische → kombiniert; sonst
> dominante einzelne), ersetzt `provenance.js`. Antwort auf Wayfinder-Issue #30.

| Issue | Thema | Status |
|-------|-------|--------|
| [Wayfinder#90](https://github.com/ManuelRingwald/Wayfinder/issues/90) (`from-firefly`) | **CAT062 ICD 2.6.0 (additiv):** I062/290 Per-Technologie-Alter — **SSR** (`0x20`), **Mode S** (`0x10`), **FLARM** (`0x04`) zusätzlich zu PSR/ES; Age-Oktette in Bit-Priorität MSB→LSB. Autoritative Track-Provenienz ersetzt Wayfinders `provenance.js`-Heuristik; FLARM erstmals unterscheidbar. ADR 0027, Antwort auf Wayfinder #30. Firefly-Seite (Encoder+Decoder+byte-genaue Vektoren+ICD) erledigt. Wayfinder: Decoder + Provenienz-Ableitung + UI-Symbolik. | ⏳ Firefly fertig; Wayfinder offen |
| [Wayfinder#5](https://github.com/ManuelRingwald/Wayfinder/issues/5) (`from-firefly`) | **CAT062 ICD 2.0.0 (Breaking):** neues optionales **I062/136** (Measured Flight Level, FRN 17, i16, LSB 1/4 FL = 25 ft) + **I062/500 von FRN 16 → FRN 27** (UAP-Standardtreue, FSPEC 3→4 Oktette). ADR 0015. Wayfinder-Decoder muss in lockstep nachziehen (AP2). | ✅ erledigt (Wayfinder PR #6, AP2) |
| [Wayfinder#9](https://github.com/ManuelRingwald/Wayfinder/issues/9) (`from-firefly`) | **CAT065 SDPS-Heartbeat, ICD 2.3.0 (additiv):** neuer Kategorie-Strom (`0x41`) auf derselben Multicast-Gruppe; Konsument dispatcht am CAT-Oktett. SDPS-Status (I065/010/000/015/030/040). ADR 0018. Wayfinder: CAT065-Decoder, Receiver-Dispatch, Staleness-Erkennung, Feed-Banner. | ✅ erledigt (beide Repos, Branch `claude/cat065-heartbeat`) |
| [Wayfinder#21](https://github.com/ManuelRingwald/Wayfinder/issues/21) (`from-firefly`) | **ICD 2.4.0 ES-Age-Subfeld (additiv, AP9.5/AP9.9):** I062/290 ist variabel lang; Bit `0x08` im primären Subfeld-Oktett zeigt ES-Age-Byte an. Wayfinder: Decoder variabel-lang, `DecodedTrack.AdsbAgeS *float64`, ADS-B-Badge im Track-Label (< 30 s frisch). ADR 0019. Abhängig von AP9.4 für echte ADS-B-Tracks. | ✅ erledigt (Wayfinder AP9.9, Commit `05d22b8`, Branch `claude/beautiful-dijkstra-e7ityj`) |
| [Wayfinder#72](https://github.com/ManuelRingwald/Wayfinder/issues/72) (`from-firefly`) | **CAT063 Sensor Status, ICD 2.5.0 (additiv):** neue Kategorie (`0x3F`) auf derselben Multicast-Gruppe; ein Record je Sensor (I063/010 SAC/SIC, I063/030 ToD, I063/060 NOGO operationell/degradiert). ADR 0022. Firefly-Seite (FF-1 SensorHealthMonitor, FF-2 Encoder/Decoder, FF-3 Sender) erledigt. Wayfinder: WF-1 CAT063-Decoder + Dispatch `0x3F`, WF-2 Health-Registry Sensor-Soll/-Ist + gelb = `0 < aktiv < gesamt`, WF-3 UI „SENSOR AUSFALL". Gegenstück zu Firefly #32. | ⏳ Firefly fertig; Wayfinder WF-1…3 offen |
