# Entscheidungs-Logbuch (ADRs)

Ein **ADR** (*Architecture Decision Record*) ist eine kurze Notiz, die **eine**
wichtige Entscheidung festhält: Worum ging es, welche Optionen gab es, wofür haben
wir uns *warum* entschieden, und welche Konsequenzen hat das?

Warum machen wir das? Weil in einem Lernprojekt das *Warum* genauso wertvoll ist
wie das *Was*. Wer später (auch wir selbst) den Code liest, soll nachvollziehen
können, weshalb er so aussieht — ohne raten zu müssen.

Format: jede Datei `NNNN-kurztitel.md`, fortlaufend nummeriert. Eine Entscheidung
wird nicht nachträglich umgeschrieben; ändert sie sich, schreiben wir einen
*neuen* ADR, der den alten ersetzt.

## Liste

- [0001 — Technologie-Stack und Vorgehen](0001-technologie-stack-und-vorgehen.md)
- [0002 — Sprache von Code und Dokumentation](0002-sprache-code-und-doku.md)
- [0003 — Cloud-native Architektur (anbieter-neutral)](0003-cloud-native-architektur.md)
- [0004 — Assurance & Zertifizierungs-Fähigkeit](0004-assurance-und-zertifizierungsfaehigkeit.md)
- [0005 — `nalgebra` als lineare Algebra](0005-nalgebra-als-lineare-algebra.md)
- [0006 — Integrationsziel Phoenix ASD & CAT062-Ausgabe](0006-integration-phoenix-asd-cat062.md)
- [0007 — `serde` für die Zustands-Serialisierung](0007-serde-serialisierung.md)
- [0008 — Safety-relevante Track-Zustandsentscheidung im Tracker](0008-safety-track-zustand-im-tracker.md)
- [0009 — Frontend-Architektur M3 (Async-Server, WebSocket, JSON, MapLibre)](0009-frontend-architektur-m3.md)
- [0010 — Multi-Radar-Fusions-Architektur M4 (zentrale Mess-Fusion)](0010-multi-radar-fusion-architektur-m4.md)
- [0011 — Gemeinsame Scan-Referenz & getrenntes Initiierungs-Tor gegen Geister-Tracks](0011-scan-fusion-referenz-und-initiierungs-tor.md)
- [0012 — Adaptiver Track-Lebenszyklus (Revisit-Intervall × Feed-Kadenz)](0012-adaptiver-track-lebenszyklus.md)
- [0013 — JPDA-Showcase: kreuzende Ziele statt parallelem Nahpaar](0013-jpda-showcase-kreuzende-ziele-statt-paralleles-nahpaar.md)
- [0014 — Produktionsbetrieb statt Lernprojekt; Wayfinder konsumiert CAT062/UDP](0014-produktionsbetrieb-statt-lernprojekt-wayfinder-cat062.md)
- [0015 — CAT062 Vertikallage (I062/136) & UAP-Standardtreue](0015-cat062-vertikallage-und-uap-standardtreue.md)
- [0016 — CAT062 Track-Ende-Signalisierung (I062/080 TSE)](0016-cat062-track-ende-signalisierung-tse.md)
- [0017 — Vertrauensgrenze des CAT062-Multicast-Feeds](0017-multicast-feed-vertrauensgrenze.md)
- [0018 — CAT065 SDPS-Service-Status-Heartbeat](0018-cat065-sdps-heartbeat.md)
- [0019 — ADS-B-Integration via OpenSky Network](0019-adsb-integration-opensky.md)
- [0020 — Live-Tracker-Modus und Plot-Aufzeichnung](0020-live-tracker-modus-fuer-echtzeit-adsb.md)
- [0021 — Konfigurierbarer System-Referenzpunkt (Single Source of Truth)](0021-konfigurierbarer-system-referenzpunkt.md)
