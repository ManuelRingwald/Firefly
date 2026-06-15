# Todo für Wayfinder (aus Firefly)

Schnittstellen-Themen, die in Firefly entstehen und Wayfinder-Arbeit auslösen.

| Issue | Thema | Status |
|-------|-------|--------|
| [Wayfinder#5](https://github.com/ManuelRingwald/Wayfinder/issues/5) (`from-firefly`) | **CAT062 ICD 2.0.0 (Breaking):** neues optionales **I062/136** (Measured Flight Level, FRN 17, i16, LSB 1/4 FL = 25 ft) + **I062/500 von FRN 16 → FRN 27** (UAP-Standardtreue, FSPEC 3→4 Oktette). ADR 0015. Wayfinder-Decoder muss in lockstep nachziehen (AP2). | ✅ erledigt (Wayfinder PR #6, AP2) |
| [Wayfinder#9](https://github.com/ManuelRingwald/Wayfinder/issues/9) (`from-firefly`) | **CAT065 SDPS-Heartbeat, ICD 2.3.0 (additiv):** neuer Kategorie-Strom (`0x41`) auf derselben Multicast-Gruppe; Konsument dispatcht am CAT-Oktett. SDPS-Status (I065/010/000/015/030/040). ADR 0018. Wayfinder: CAT065-Decoder, Receiver-Dispatch, Staleness-Erkennung, Feed-Banner. | ✅ erledigt (beide Repos, Branch `claude/cat065-heartbeat`) |
