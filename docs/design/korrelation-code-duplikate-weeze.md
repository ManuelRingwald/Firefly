# Design-Notiz: Flugplan-Korrelation und Squawk-Duplikate (Lektion Weeze)

> **Status:** Vormerkung für das künftige Arbeitspaket **Flugplan-Korrelation
> (I062/390)** auf der ARTAS-Gap-Roadmap. Noch kein Code — diese Notiz hält
> Betriebs-Erfahrung fest, damit sie in den Entwurf einfließt.

## Beobachtung (Betreiber, 2026-07-11)

Am Flughafen Weeze (Phoenix-Tracker mit Phoenix WebInnovation als EFS/ASD)
treten **doppelte Flugplan-Korrelationen** auf: Derselbe Flugplan korreliert
auf zwei Tracks, weil im angrenzenden niederländischen Luftraum derselbe
Mode-3/A-Code (Squawk) legitim von einem anderen Flug genutzt wird.

## Ursache

Squawks sind **nicht global eindeutig**. Die ORCAM-Vergabe (Originating
Region Code Assignment Method) verteilt Code-Blöcke auf Regionen unter der
Annahme geographischer Trennung — an Staatsgrenzen (Weeze liegt direkt an
der NL-Grenze) bricht die Annahme: Das Lagebild sieht beide Verkehre, eine
rein **Code-basierte** Korrelation matcht den Flugplan auf beide Tracks.
Verschärfend: In Mode-S-Gebieten ist der Conspicuity-Code **1000** per
Design mehrdeutig.

## Entwurfs-Anforderungen für Fireflys Korrelation (wenn das AP ansteht)

1. **Callsign-first:** Primärer Korrelations-Schlüssel ist die Mode-S
   Aircraft Identification (bei uns bereits im Strom: I062/245) gegen das
   Flugplan-Callsign — praktisch eindeutig, kein ORCAM-Problem. Der Squawk
   ist nur Rückfall-Schlüssel.
2. **Code nur bei Eindeutigkeit:** Code-basierte Korrelation ausschließlich,
   wenn der Code im Interessensgebiet **eindeutig** ist. Duplikat ⇒ keine
   Auto-Korrelation + „Duplicate Code"-Signal (manuelle Korrelation) — ein
   doppeltes Label ist gefährlicher als ein unkorreliertes. Code 1000 ist
   **nie** ein Korrelations-Schlüssel.
3. **Räumlich/zeitlich plausibilisiert:** Korrelation nur innerhalb
   definierter Volumina (AoI/AoR, Höhenband) und im Erwartungsfenster des
   Flugplans (ETA, Anflugrichtung, Profil).
4. **Beobachtbarkeit:** Duplikat-Ereignisse als Metrik/Log (Meldegrundlage
   Richtung CCAMS/Nachbar-ANSP).

Dieselbe Denkfigur nutzt bereits REG.1 (ADR 0034): Korrespondenz-Pairing über
die **ICAO-Adresse** statt über den wiederverwendbaren Squawk.
