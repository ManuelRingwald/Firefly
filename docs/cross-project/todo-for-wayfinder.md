# Todo für Wayfinder (aus Firefly)

Schnittstellen-Themen, die in Firefly entstehen und Wayfinder-Arbeit auslösen.

> **`adsb_opensky` trägt optionales `poll_interval_secs` (ADR 0029, Kontrakt
> v1.4.0, additiv).** Antwort auf Wayfinder-Wunsch #3 (Poll-Schutz): das
> OpenSky-Poll-Intervall ist jetzt **pro Quelle** über `FIREFLY_SOURCES` setzbar
> (ganze Sekunden, `> 0`; fehlt/`0` → Firefly-Default 10 s). Nur für
> `adsb_opensky` (FLARM ist Push, Radar hat eigene Scan-Periode). **Additiv** —
> `SourceSpec` trägt kein `deny_unknown_fields`, ein älterer Firefly ignoriert das
> Feld, ein neuer nimmt bei fehlendem Feld den Default (Merge-Reihenfolge
> entkoppelt). **Wayfinder-Folge (bereits umgesetzt, gleicher 4-Punkte-Batch):**
> `store.Source`/Docker-Backend serialisieren `poll_interval_secs` nach
> `FIREFLY_SOURCES`; Admin-UI-Feld (nur ADS-B, Default 10 s, Wertebereich
> 5–3600 s) + Infobox zum OpenSky-Rate-Limit. **Nicht** enthalten: echter
> 429-Backoff (separater Härtungsschritt, Wayfinder #2).

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
| [Wayfinder#242](https://github.com/ManuelRingwald/Wayfinder/issues/242) (`from-firefly`) | **CAT062 ICD 3.6.0 (additiv, FR-TRK-043, VERT.3):** Kinematik-Trends — **I062/200** (FRN 15, 1 Oktett: TRANS Kurs / LONG Speed / VERT je 2 Bit, 3 = undetermined; ADF immer 0; **nur gesendet, wenn mindestens eine Achse bestimmt** — Absenz = kein Trend-Anspruch) und **I062/210** (FRN 8, Ax/Ay i8 × 0,25 m/s², Sättigung ±31,75). Nur bei frischem Schätzwert (≤ 30 s); Track ohne beides byte-identisch alt. Byte-genaue Referenz-Vektoren in ICD §4.9 (`04 FE`, `7F 80`, `0x54`, `0xB0`). Wayfinder: Decoder + WS-JSON + Label (Kurven-Indikator aus TRANS; VERT konsistent mit RoCD-Pfeil aus #241). Kein Lockstep. S3. | ⏳ offen |
| [Wayfinder#241](https://github.com/ManuelRingwald/Wayfinder/issues/241) (`from-firefly`) | **CAT062 ICD 3.5.0 (additiv, FR-TRK-042, VERT.2):** die Vertikal-Kette — **I062/130** (FRN 18, geometrische Höhe, i16 × 6,25 ft), **I062/135** (FRN 19, gefilterte barometrische Höhe; **QNH-Bit** nur bei Korrektur auf beobachtetes regionales QNH, sonst Druckhöhe mit Bit 0; 15-Bit-ZK × 25 ft), **I062/220** (FRN 20, RoCD, i16 × 6,25 ft/min, positiv = steigen). Nur bei frischem Schätzwert (≤ 30 s); Track ohne Vertikal-Daten byte-identisch alt; I062/136 bleibt daneben. Byte-genaue Referenz-Vektoren in ICD §4.8. Wayfinder: Decoder + WS-JSON + Label (geglättete Höhe statt springender I062/136-Rohwerte, QNH-Kennzeichnung, Climb-/Descend-Pfeil mit Hysterese). Kein Lockstep. S3. | ⏳ offen |
| [Wayfinder#240](https://github.com/ManuelRingwald/Wayfinder/issues/240) (`from-firefly`) | **Quell-Kontrakt v1.7.0 (additiv, FEP.5):** neuer Typ `mlat_asterix` — **WAM/MLAT** als **CAT020/019 über UDP** (unabhängige Überwachung; σ je Meldung aus I020/500). Felder `listen`? (`group:port`, Default `0.0.0.0:8020`), `sac`/`sic`?, `sensor_id`? (Default 240); **keine** bbox, **kein** Standort, **kein** `cred_env`. Firefly-Seite (Decoder `cat020`/`cat019`, Crate `firefly-mlat`, Verdrahtung, Kontrakt) erledigt. Wayfinder: Store-Vokabular + Validierung, Docker-Backend-Serialisierung, Admin-UI-Typ „WAM/MLAT"; Hinweis `FIREFLY_SYSTEM_REF_*` bei Einzelquelle. Bietet sich zusammen mit #239 an (identisches Muster). Kein Ausgabe-ICD-Bezug (MLAT-Provenienz erscheint als Mode S; eigenes MLT-Subfeld = künftiger additiver Bump). S2–S3. | ⏳ Firefly fertig; Wayfinder offen |
| [Wayfinder#239](https://github.com/ManuelRingwald/Wayfinder/issues/239) (`from-firefly`) | **Quell-Kontrakt v1.6.0 (additiv, FEP.3):** neuer Typ `adsb_asterix` — ADS-B von der eigenen **Bodenstation** als **CAT021 über UDP** (Produktions-Bezugsweg; σ je Meldung aus NACp). Felder `listen`? (`group:port`, Default `0.0.0.0:8021`), `sac`/`sic`?, `sensor_id`? (Default 230); **keine** bbox, **kein** `lat`/`lon`, **kein** `cred_env`. Firefly-Seite (Decoder `firefly-asterix::cat021`, Crate `firefly-adsb021`, Verdrahtung, Kontrakt) erledigt. Wayfinder: Store-Vokabular + Validierung, Docker-Backend-Serialisierung, Admin-UI-Typ „ADS-B (Bodenstation)" ohne bbox/Credentials; Hinweis `FIREFLY_SYSTEM_REF_*` bei Einzelquelle. Analog #91/#201. Kein Ausgabe-ICD-Bezug. S2–S3. | ⏳ Firefly fertig; Wayfinder offen |
| [Wayfinder#238](https://github.com/ManuelRingwald/Wayfinder/issues/238) (`from-firefly`) | **CAT062 ICD 3.4.0 (additiv, FR-TRK-040, FEP.2):** I062/380 trägt die Mode-S-EHS-**DAPs** — **MHG** (#3, LSB 360/2¹⁶ °), **SAL** (#6, Selected Altitude: eingedrehte Autopilot-Höhe, 13-Bit-Zweierkomplement × 25 ft, **Level-Bust-Basis**), **IAR** (#26, LSB 1 kt), **MAC** (#27, LSB 0,008); nur bei frischem Wert (≤ 30 s), DAP-loser Track byte-identisch alt, kein Lockstep. Wayfinder: I062/380 subfeld-getrieben dekodieren, SEL-Höhe im Label neben Ist-Höhe (Abweichung hervorheben), MHG/IAS/Mach im Detail-Fenster, WS-JSON erweitern. Byte-genauer Referenz-Dump in ICD §4.7. S3. | ⏳ offen |
| [Wayfinder#237](https://github.com/ManuelRingwald/Wayfinder/issues/237) (`from-firefly`) | **CAT063 ICD 3.3.0 (additiv, FR-IO-008, REG.3/ADR 0034):** Bei aktiver Registrierungs-Korrektur (REG.2b) trägt der CAT063-Record je Radar die **angewandte Bias-Korrektur** in **I063/080** (FRN 7: SRG=0 + SRB, i16 BE, LSB 1/128 NM ≈ 14,47 m) und **I063/081** (FRN 8: SAB, i16 BE, LSB 360/2¹⁶ ° ≈ 0,0055°). Nur bei in Kraft befindlicher Korrektur gesendet — **Absenz = „keine Korrektur"**; FSPEC dann `0xBB 0x80`, Record 16 Oktette; byte-genauer Referenz-Dump in ICD §9. Kein Wire-Bruch, kein Lockstep. Wayfinder: CAT063-Decoder um FRN 7/8 erweitern (feste Längen), Bias je Sensor im Sensor-/Feed-Panel anzeigen (z. B. „Δr +145 m · Δθ +0,30°"). Baut auf WF-1…3 (#72) auf. S2–S3. | ⏳ offen |
| [Wayfinder#236](https://github.com/ManuelRingwald/Wayfinder/issues/236) (`from-firefly`) | **CAT062 ICD 3.2.0 (additiv, FR-TRK-036):** I062/080 trägt die ARTAS-Vertrauens-Flags **MON** (Oktett 1 `0x80`, monosensor — ≤ 1 Sensor im 30-s-Frische-Fenster, keine Kreuz-Prüfung) und **SPI** (Oktett 1 `0x40`, „Ident"-Puls der letzten Meldung, Quelle CAT048); **SIM**-Slot (Oktett 2 `0x80`) dokumentiert, immer 0. Kein Wire-Bruch, kein Lockstep. Wayfinder: Decoder liest MON/SPI → WS-JSON → ASD (Mono-Sensor-Kennzeichnung, SPI-/Ident-Highlight). S2–S3. | ⏳ offen |
| [Wayfinder#235](https://github.com/ManuelRingwald/Wayfinder/issues/235) (`from-firefly`) | **FSPEC-Härtung + Fuzzing für den Go-Decoder (Fireflys Fuzzer-Fund, QW.2):** Fireflys neues Fuzzing (NFR-SAFE-002) fand in der gemeinsamen FSPEC-FRN-Arithmetik einen u8-Überlauf bei feindlich verlängerten FX-Ketten (> 36 Oktette) — gefixt via `MAX_FSPEC_OCTETS` = 36 + `FspecTooLong`. Wayfinders CAT062/063/065-Decoder parst dieselben Ketten vom unauthentifizierten Multicast: (1) FX-Ketten-Obergrenze prüfen/einziehen, (2) `go test -fuzz`-Targets + CI-Schritt (erfüllt Charter §7 „Fuzzing vorsehen"), (3) Überlange-Kette-Regressionstest. Kein Wire-/ICD-Bezug, kein Lockstep. S2–S3. | ⏳ offen |
| [Firefly#55](https://github.com/ManuelRingwald/Firefly/issues/55) (`from-wayfinder`) | **CAT063-UAP-Standardisierung + per-Quelle-Fehlergrund (ADR 0032/0033).** **H1 (ICD 3.0.0, BREAKING, ADR 0032):** Sensor-Status-Records folgen den echten EUROCONTROL-FRN-Slots — I063/010 = **SDPS** (25/2), Sensor-Identität ins neue **I063/050** (FRN 4), I063/030 → FRN 3, I063/060 → FRN 5, FSPEC `0xE0`→**`0xB8`**, CON-Werte standardkonform. **H3 (ICD 3.1.0, additiv, ADR 0033):** ein **degradierter** Sensor mit bekanntem Grund trägt zusätzlich das **I063/RE** (FRN 13, FSPEC dann `0xB9 0x04`) mit Vendor-Subfeld **`SRC-REASON`** (`1=unreachable`/`2=auth`/`3=rate_limited`), Layout `[LEN=0x03][0x80][code]`; nur bei Degradierung mit Grund gesendet, RE selbst-begrenzend (kein Wire-Bruch). Grund aus den HTTP-ADS-B-Pollern (OpenSky/adsbagg); FLARM/Radar ohne Grund. **Wayfinder-Folgen:** **H2 (lockstep zu H1):** Decoder liest Sensor aus I063/050, FSPEC `0xB8`, RE/SP längen-tolerant (ADR 0019). **H4 (additiv):** RE-`SRC-REASON` dekodiert + im Feed-Health-Chip angezeigt (`· NICHT ERREICHBAR`/`· AUTH-FEHLER`/`· RATENLIMIT`, ADR 0020) → **schließt #197**. | ✅ **erledigt** — H1 (PR #56), H3 (PR #57) gemergt; Wayfinder H2 (#206) + H4 (#207) gemergt; #55 + #197 geschlossen |
| [Wayfinder#201](https://github.com/ManuelRingwald/Wayfinder/issues/201) | **Quell-Kontrakt v1.5.0 (additiv, ADR 0031):** neuer Typ `adsb_aggregator` (auth-freier ADS-B via adsb.lol/adsb.fi) mit optionalem `provider`-Feld (`adsb_lol` Default \| `adsb_fi`; unbekannt → Startfehler), `poll_interval_secs` gilt auch hier, **kein** `cred_env` (gesetzt → ignoriert). Firefly-Seite (Crate `firefly-adsbagg` + Verdrahtung + Kontrakt) erledigt (#53). Wayfinder: Store-Vokabular + Validierung (Provider-Whitelist), Docker-Backend reicht `provider` durch, UI-Typ „ADS-B (Community-Aggregator)" mit Provider-Auswahl, **ohne** Credential-Block. | ⏳ Firefly fertig; Wayfinder offen |
| [Wayfinder#90](https://github.com/ManuelRingwald/Wayfinder/issues/90) (`from-firefly`) | **CAT062 ICD 2.6.0 (additiv):** I062/290 Per-Technologie-Alter — **SSR** (`0x20`), **Mode S** (`0x10`), **FLARM** (`0x04`) zusätzlich zu PSR/ES; Age-Oktette in Bit-Priorität MSB→LSB. Autoritative Track-Provenienz ersetzt Wayfinders `provenance.js`-Heuristik; FLARM erstmals unterscheidbar. ADR 0027, Antwort auf Wayfinder #30. Firefly-Seite (Encoder+Decoder+byte-genaue Vektoren+ICD) erledigt. Wayfinder: Decoder + Provenienz-Ableitung + UI-Symbolik. | ⏳ Firefly fertig; Wayfinder offen |
| [Wayfinder#5](https://github.com/ManuelRingwald/Wayfinder/issues/5) (`from-firefly`) | **CAT062 ICD 2.0.0 (Breaking):** neues optionales **I062/136** (Measured Flight Level, FRN 17, i16, LSB 1/4 FL = 25 ft) + **I062/500 von FRN 16 → FRN 27** (UAP-Standardtreue, FSPEC 3→4 Oktette). ADR 0015. Wayfinder-Decoder muss in lockstep nachziehen (AP2). | ✅ erledigt (Wayfinder PR #6, AP2) |
| [Wayfinder#9](https://github.com/ManuelRingwald/Wayfinder/issues/9) (`from-firefly`) | **CAT065 SDPS-Heartbeat, ICD 2.3.0 (additiv):** neuer Kategorie-Strom (`0x41`) auf derselben Multicast-Gruppe; Konsument dispatcht am CAT-Oktett. SDPS-Status (I065/010/000/015/030/040). ADR 0018. Wayfinder: CAT065-Decoder, Receiver-Dispatch, Staleness-Erkennung, Feed-Banner. | ✅ erledigt (beide Repos, Branch `claude/cat065-heartbeat`) |
| [Wayfinder#21](https://github.com/ManuelRingwald/Wayfinder/issues/21) (`from-firefly`) | **ICD 2.4.0 ES-Age-Subfeld (additiv, AP9.5/AP9.9):** I062/290 ist variabel lang; Bit `0x08` im primären Subfeld-Oktett zeigt ES-Age-Byte an. Wayfinder: Decoder variabel-lang, `DecodedTrack.AdsbAgeS *float64`, ADS-B-Badge im Track-Label (< 30 s frisch). ADR 0019. Abhängig von AP9.4 für echte ADS-B-Tracks. | ✅ erledigt (Wayfinder AP9.9, Commit `05d22b8`, Branch `claude/beautiful-dijkstra-e7ityj`) |
| [Wayfinder#72](https://github.com/ManuelRingwald/Wayfinder/issues/72) (`from-firefly`) | **CAT063 Sensor Status, ICD 2.5.0 (additiv):** neue Kategorie (`0x3F`) auf derselben Multicast-Gruppe; ein Record je Sensor (I063/010 SAC/SIC, I063/030 ToD, I063/060 NOGO operationell/degradiert). ADR 0022. Firefly-Seite (FF-1 SensorHealthMonitor, FF-2 Encoder/Decoder, FF-3 Sender) erledigt. Wayfinder: WF-1 CAT063-Decoder + Dispatch `0x3F`, WF-2 Health-Registry Sensor-Soll/-Ist + gelb = `0 < aktiv < gesamt`, WF-3 UI „SENSOR AUSFALL". Gegenstück zu Firefly #32. | ⏳ Firefly fertig; Wayfinder WF-1…3 offen |
