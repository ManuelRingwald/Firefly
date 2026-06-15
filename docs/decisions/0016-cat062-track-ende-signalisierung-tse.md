# ADR 0016 — CAT062 Track-Ende-Signalisierung (I062/080 TSE)

- **Status:** akzeptiert
- **Datum:** 2026-06-15
- **Schnittstellen-relevant:** ja (CAT062-Ausgabe-Vertrag, ICD → 2.2.0, additiv)

## Kontext

Der CAT062-Ausgabestrom kennt heute **keinen expliziten Track-Tod**. Firefly
löscht einen Track intern (`Tracker`-Lebenszyklus, `should_delete` /
`should_delete_continuous`), und damit verschwindet er einfach aus dem nächsten
periodischen Datenblock. Ein Konsument (Wayfinder, jedes ASD) muss den Tod also
**implizit per Timeout raten**: „Track X war länger nicht mehr im Strom, also
ist er wohl weg."

Das ist aus zwei Gründen unzureichend für den Produktivbetrieb:

1. **Verzögertes/unsicheres Entfernen.** Bis ein Stale-Timeout greift, bleibt
   ein toter Track auf dem Lotsenschirm stehen — oder er verschwindet zu früh,
   wenn nur ein Datenblock verloren ging. Der Lotse kann „Track beendet" nicht
   von „Datenblock verpasst" unterscheiden.
2. **Kein Lebenszyklus-Signal.** Echte SDPS/ARTAS-Feeds senden einen letzten
   Record mit gesetztem **TSE** (*Track Service End*) — dem expliziten „dies ist
   die letzte Meldung für diesen Track, jetzt entfernen". Das Bit liegt im
   CAT062-Item **I062/080 (Track Status)**, zweites Oktett.

Architektonische Spannung: Die periodische Ausgabe (`Tracker::snapshot_at`,
ADR 0013) ist bewusst **read-only** und zeigt nur **lebende** Tracks. Das
Lösch-*Ereignis* entsteht aber im Eingangs-Pfad (`process_scan` /
`process_plots`). Ein toter Track ist im Snapshot schon weg — es gibt nichts
mehr zu kodieren. Die Lösch-Information muss also **eingefangen und einmalig
nachgeliefert** werden.

## Entscheidung

1. **Lösch-Ereignis einfangen.** An beiden Löschstellen (Batch-`process_scan`
   und zeit-kontinuierlicher Pfad) wird, bevor der Track verworfen wird, sein
   **vollständiger letzter Zustand** als `SystemTrack` mit gesetztem
   Ende-Marker in einen Tracker-Puffer (`ended_tracks`) geschrieben.
2. **Neutraler Typ: `SystemTrack.ended: bool`** (Default `false`), konsistent
   mit den vorhandenen Status-Flags `confirmed` / `coasting`. Der finale Record
   trägt `ended = true` und ansonsten den **vollständigen letzten bekannten
   Track-Zustand** (Position, Track-Nummer, Identität, …) — nicht nur ein
   Minimal-Record. So bleibt der Encoder gleichförmig und das ASD erhält die
   maximale Information zum entfernten Track.
3. **Ausgabe-Anbindung.** Beim nächsten Herzschlag hängen die Ausgabe-Ports
   (`Player::periodic_snapshots` / `periodic_frames`, Multicast-Sender) die
   gepufferten Ende-Records **einmalig** an die Live-Tracks an und leeren den
   Puffer. Ein Track erscheint damit **genau einmal** mit `ended = true` und
   danach **nie wieder**.
4. **Encoder: TSE-Bit in I062/080.** `encode_track_status` setzt bei
   `ended = true` das **TSE-Bit im zweiten Oktett** von I062/080. Die genaue
   Bit-Position wird **byte-genau gegen SUR.ET1.ST05.2000-STD-09-01 Ed. 1.10
   verifiziert**, bevor der Wert eingefroren wird (Qualitäts-Gate).
5. **Nur TSE.** Das symmetrische TSB (*Track Service Begin*, „Track geboren",
   gleiches Oktett) wird in diesem Schritt **nicht** umgesetzt — für die
   Lösch-Korrektheit nicht nötig, bewusst zurückgestellt.

## Begründung

- **Fachlich:** Explizites, deterministisches Entfernen statt Timeout-Raten ist
  der ED-109A-typische Mechanismus und schließt eine echte Lücke im Lagebild.
- **Vollständiger Zustand statt Minimal-Record:** gleichförmiger Encoder-Pfad
  (kein Sonderfall), und das ASD kann den letzten bekannten Ort/Identität des
  entfernten Tracks noch anzeigen/protokollieren.
- **Flag am `SystemTrack`** spiegelt das bestehende Muster (`confirmed`,
  `coasting`) und hält die Adapter uniform — der Encoder liest nur ein Flag.
- **Determinismus (NFR-CLOUD-001):** Da der Eingangs-Pfad in Datenzeit-Ordnung
  arbeitet, ist die Reihenfolge der gepufferten Ende-Records reproduzierbar; der
  Puffer wird je Tick deterministisch geleert.

## Konsequenzen

- **Additiv, kein Wire-Bruch.** FRN 13 (I062/080) ist bereits in jedem Record;
  das Item ist variabel mit FX-Kette. Ein zusätzliches zweites Oktett mit
  gesetztem TSE bricht keinen toleranten Decoder. **ICD → 2.2.0 (minor,
  additiv)**, analog zu I062/245 (AP7/ICD 2.1.0).
- **Wayfinder muss in Lockstep nachziehen.** Ein Ende-Record ist ein **nicht
  mehr lebender** Track; ein naiver Konsument würde ihn **anzeigen** (Ein-Frame-
  Geist). Wayfinder **muss** TSE = 1 als „entfernen" interpretieren und den
  Record nicht als Live-Track rendern. Cross-Project-Issue `from-firefly`.
- `SystemTrack` bekommt `ended: bool`. Der JSON-Frame-Adapter (`firefly-io`)
  bleibt **unberührt** (eigener `FrameTrack`-Wire-Typ); nur der CAT062-Adapter
  und die periodische Ausgabe nutzen das Feld.
- Anforderungen: neues **FR-TRK-029** (Track-Ende-Signalisierung) und
  Erweiterung von FR-IO-003/004 um das TSE-Bit.
- Der byte-genaue Referenz-Dump (`single_track_matches_reference_dump`) bleibt
  **unverändert** (er kodiert einen lebenden Track ohne `ended`).

## Ehrliche Grenze

TSE signalisiert das **Ende der Track-Bedienung** durch Firefly, nicht
zwangsläufig, dass das Luftfahrzeug gelandet/verschwunden ist — ein Track kann
auch durch Coasting-Budget-Überschreitung sterben, während das Ziel noch fliegt
(z. B. dauerhafter Radar-Schatten). Das ASD entfernt die *Darstellung*; eine
fachliche „Landung"-Semantik ist damit nicht behauptet. TSB (Geburt) und ein
Unterscheiden der Lösch-*Ursache* bleiben bewusst offen.

Im **asynchronen Pfad** (ADR 0013, `process_plots`) wird die Löschung durch
**eintreffende Plots** getrieben, nicht durch den Ausgabe-Herzschlag. Verstummt
der **gesamte** Feed (kein einziger Plot von irgendeinem Sensor), läuft kein
Lösch-Sweep — ein verwaister Track coastet dann fort und es wird **kein** TSE
gesendet, bis wieder irgendein Plot eintrifft. Im Normalbetrieb mit mehreren
Zielen/Sensoren ist das unkritisch; ein heartbeat-getriebenes Löschen bei
komplett stillem Feed bleibt mögliche Folgearbeit.
