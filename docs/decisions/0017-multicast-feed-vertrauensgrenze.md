# ADR 0017 — Vertrauensgrenze des CAT062-Multicast-Feeds

- **Status:** akzeptiert
- **Datum:** 2026-06-15
- **Schnittstellen-relevant:** nein (keine Wire-Format-Änderung; betrifft Netz-
  und Betriebs-Annahmen rund um den bestehenden CAT062/UDP-Vertrag)

## Kontext

Firefly sendet den CAT062-Systemtrack-Strom als **UDP-Multicast** aus
(`firefly-multicast`, ADR 0006). UDP-Multicast hat **keine eingebaute
Authentifizierung, Integritätssicherung oder Verschlüsselung**: Jeder Host, der
der Multicast-Gruppe im selben L2/L3-Bereich beitreten kann, empfängt den
Strom; jeder Host, der Pakete an die Gruppe/den Port senden kann, kann
Datagramme **einspeisen**, die für jeden Empfänger (jedes ASD) wie reguläre
Firefly-Tracks aussehen.

Mit Wayfinder als produktivem ASD-Konsumenten und insbesondere seit **ADR 0016
(TSE)** ist die Tragweite konkret geworden:

1. **Vertraulichkeit.** Die Luftlage (Positionen, Identitäten, Callsigns seit
   AP7) ist ein operatives Bild, kein öffentliches Datum. Passives Mitlesen im
   selben Netzsegment ist mit Standard-Multicast nicht verhindert.
2. **Integrität / Injektion.** Ein Angreifer im selben Segment könnte
   gefälschte CAT062-Datenblöcke an die Gruppe senden — Phantom-Tracks
   einschleusen, Positionen verfälschen, oder seit ADR 0016 ein **gefälschtes
   TSE-Bit** senden und damit dem Lotsen einen **echten** Track aus dem Bild
   löschen lassen (ein ASD, das TSE korrekt umsetzt — wie gefordert —, entfernt
   den Track sofort und ungefragt).
3. **Verfügbarkeit.** Ein Flood gefälschter Blöcke könnte Empfänger mit
   Dekodier-Last überziehen — der robuste Decoder (CLAUDE.md §7) schützt vor
   Abstürzen, aber nicht vor Last.

Diese drei Eigenschaften — Vertraulichkeit, Integrität, Verfügbarkeit — sind
mit **reinem UDP-Multicast nicht herstellbar**. Die Frage ist also nicht "wie
verschlüsseln/signieren wir CAT062", sondern: **welche Schicht trägt die
Vertrauensgrenze**, und was dokumentieren wir als bewusst akzeptierte Grenze
versus als zu schließende Lücke.

## Entscheidung

**Die Vertrauensgrenze liegt auf der Netzwerk-Schicht, nicht im
CAT062-Anwendungsprotokoll.** Konkret:

1. **Netz-Isolation als primäre Maßnahme.** Der CAT062-Multicast-Verkehr
   (Default-Gruppe `239.255.0.62:8600`) läuft auf einem **dedizierten,
   abgeschotteten Netzsegment** (eigenes VLAN oder gleichwertige Isolation),
   das ausschließlich Firefly-Sender und autorisierte ASD-Empfänger (Wayfinder
   u. a.) enthält. Hosts außerhalb dieses Segments haben **keinen** L2/L3-Zugang
   zur Multicast-Gruppe — weder zum Empfangen noch zum Einspeisen. Dies ist
   **Betriebs-/Deployment-Verantwortung** (Netzwerk-Konfiguration), nicht Code.
2. **Multicast-TTL bleibt 1** (`MulticastConfig` Default, bereits umgesetzt) —
   Pakete verlassen das lokale Subnetz nicht, auch wenn Routing fehlerhaft
   konfiguriert wäre. Das ist eine **zusätzliche**, aber **keine
   hinreichende** Maßnahme (TTL schützt nicht vor Angreifern *im selben*
   Subnetz).
3. **Kein anwendungsseitiges Signieren/Verschlüsseln von CAT062 im
   Geltungsbereich dieser ADR.** ASTERIX CAT062 kennt kein natives
   Auth-/Krypto-Feld; ein Eigenbau-Wrapper würde den selbstbeschreibenden,
   standardkonformen Draht-Vertrag (ADR 0006) brechen und stünde im Widerspruch
   zu "keinen Firefly-Code/-Format-Eigenbau in den Vertrag mischen". Falls
   Ende-zu-Ende-Integrität/-Vertraulichkeit über die Netz-Isolation hinaus
   gefordert wird, ist das eine **Netz-Layer-Maßnahme** (siehe Ausblick), kein
   CAT062-Thema.
4. **Sender-Härtung (klein, additiv).** `firefly-multicast` bindet den
   Sende-Socket bereits an eine Schnittstelle/Adresse über `MulticastConfig`;
   dies bleibt die einzige code-seitige Maßnahme dieser ADR — keine neue Logik
   nötig, lediglich Dokumentation der bestehenden 12-Factor-Konfiguration
   (`FIREFLY_CAT062_GROUP`/`_PORT`) als Teil der Vertrauensgrenze.

## Begründung

- **Konsistent mit dem bestehenden Vertrag (ADR 0006).** CAT062/UDP-Multicast
  wurde *bewusst* gewählt, weil es der reale, standardisierte SDPS-Ausgabe-
  Kontrakt ist (kein Eigenbau-Transport). Echte SDPS/ARTAS-Umgebungen lösen
  genau dieselbe Vertrauensfrage über **Netz-Segmentierung**, nicht über
  Anwendungs-Krypto im ASTERIX-Strom — Firefly folgt damit etablierter Praxis.
- **Trennung der Zuständigkeiten.** Firefly liefert einen korrekten,
  standardkonformen Strom; *wer* ihn empfangen/einspeisen kann, ist eine
  Netz-/Deployment-Entscheidung, die bei jedem Einsatz neu zu treffen ist
  (Kubernetes-NetworkPolicy, VLAN, Firewall — je nach Zielumgebung). Eine
  Code-seitige "Lösung" könnte diese Entscheidung nicht für alle Umgebungen
  vorwegnehmen.
- **TSE-Risiko wird dokumentiert, nicht "weggecoded".** Das gefälschte-TSE-
  Szenario ist real (Punkt 2 oben), aber sein Schutz ist identisch mit dem
  allgemeinen Injektions-Schutz — keine TSE-spezifische Zusatzmaßnahme nötig,
  sobald die Netzgrenze steht.

## Konsequenzen

- **Betriebs-Dokumentation statt Code-Änderung.** Diese ADR ist primär eine
  **explizite Aussage der Vertrauensgrenze** für Betreiber/Auditoren
  (ED-109A-Rückverfolgbarkeit: "wo ist die Grenze, und warum reicht sie aus").
  `README.md`/Deployment-Doku sollte bei der nächsten Gelegenheit einen Hinweis
  auf das benötigte isolierte Segment erhalten.
- **Neue Anforderung NFR-SEC-001** im Register: Netz-Isolation des
  CAT062-Multicast-Pfads als Vertrauensgrenze, mit Verweis auf diese ADR
  (Status: "dokumentiert" — Umsetzung ist Deployment-Sache, nicht Test-Sache).
- **Wayfinder-seitiges Pendant folgt** (Wayfinder-ADR, Häppchen 1.2 der
  Roadmap): spiegelt diese Vertrauensgrenze für den Empfangspfad und
  entscheidet zusätzlich den **Browser-Rand** (TLS/Auth), der hier *nicht*
  behandelt wird — das ist eine andere Grenze (Wayfinder ↔ Lotsen-Browser, kein
  Multicast).
- **Kein Code-Diff in diesem Schritt.** `MulticastConfig` (Bind-Interface,
  TTL=1, Gruppe/Port per Env) ist bereits ausreichend für die hier getroffene
  Entscheidung; nichts wird geändert.

## Ehrliche Grenze

- Diese ADR **garantiert keine** Vertraulichkeit/Integrität, falls die
  Netz-Isolation in einer konkreten Umgebung **nicht** korrekt umgesetzt ist —
  sie macht die **Annahme explizit**, prüft sie aber nicht automatisiert (das
  wäre Aufgabe von Netzwerk-Audits/Penetrationstests der Zielumgebung, außerhalb
  des Code-Projekts).
- **Multicast-Layer-Verschlüsselung/-Authentifizierung** (z. B. IPsec, MACsec,
  authentifiziertes Multicast-Routing) ist als **möglicher zukünftiger
  Härtungsschritt** identifiziert, aber **nicht** Teil dieser Entscheidung —
  sie würde auf Netz-/Infrastruktur-Ebene angesiedelt, nicht in
  `firefly-multicast`.
- Ein **Last-/DoS-Schutz** gegen einen Flood gefälschter Datagramme (Punkt 3,
  Verfügbarkeit) ist mit reiner Netz-Isolation **nicht** vollständig
  adressiert (ein Angreifer *innerhalb* des Segments könnte fluten); dies bleibt
  als offener Punkt für die "Betriebs-Härtung"-Roadmap-Position vermerkt.
