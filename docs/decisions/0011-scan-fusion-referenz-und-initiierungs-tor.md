# ADR 0011 — Gemeinsame Scan-Referenz & getrenntes Initiierungs-Tor gegen Geister-Tracks

- **Status:** akzeptiert
- **Datum:** 2026-06-11

## Kontext

Mit der zentralen Mess-Fusion (ADR 0010) verarbeitet **ein** Tracker den
gemischten Plot-Strom **mehrerer** Radare. Innerhalb eines Scans werden die
Sensoren **sequenziell** abgearbeitet (deterministische Reihenfolge über eine
`BTreeMap`): jeder Sensor gated, assoziiert und aktualisiert die Tracks, dann
kommt der nächste.

Bei der Verfeinerung der Frankfurt-Showcase-Szene (M6.1) traten reproduzierbar
**Geister-Tracks** auf: dasselbe Flugzeug erschien als zwei bestätigte Tracks.
Sie wurden zunächst durch ein künstlich geweitetes Assoziations-Tor
(`P_G = 0.999` statt `0.99`) maskiert. Eine empirische Diagnose zeigte **zwei
verschiedene Ursachen** — beide in der Multi-Radar-Verarbeitung, *nicht* im
IMM-Manöver-Modell (entgegen der ersten Vermutung):

1. **Sequenzielle Tor-Verengung.** Sensor A faltet seinen Plot in den Track →
   dessen Kovarianz `P` **schrumpft** (das Tor wird enger). Wird der Plot von
   Sensor B (dasselbe Flugzeug) anschließend gegen den **bereits verengten**
   Track gegated, fällt er knapp heraus → er assoziiert nicht → die
   Initiierung gebärt einen **Duplikat-Track**.
   - *Beleg:* Der Geist am Holding-Pattern verschwindet vollständig, sobald nur
     **ein** Radar es sieht.

2. **Ausreißer-Plot gebärt sofort einen Track.** Bei `P_G = 0.99` fallen rund
   1 % *echter* Plots aus dem Tor (χ²-Schwanz). Ein einzelner solcher
   3-σ-Ausreißer eines geradeaus, gut verfolgten Ziels liegt außerhalb *jeder*
   Prädiktion — die gemeinsame Referenz (Punkt 1) hilft hier nicht. Er startet
   einen Tentative-Track, der sich bestätigt und fortan je einen Radar-Stream
   „stiehlt".
   - *Beleg:* Dieser Geist verschwindet nur durch mehr Prozessrausch (das das
     Tor weitet) — also ein reines Tor-/Initiierungs-Problem.

## Die abgewogenen Optionen

**Für Ursache 1 (sequenzielle Verengung):**
- *(A) Joint-Update gegen eine eingefrorene Scan-Start-Referenz* — alle Sensoren
  gaten/assoziieren gegen die Prädiktion zu Scan-Beginn; die Zustands-Fusion
  bleibt sequenziell (für unabhängige Messungen ergibt das dieselbe gemeinsame
  A-posteriori).
- *(B) Echtes simultanes Multi-Sensor-JPDA* — theoretisch am saubersten, aber ein
  großer Umbau (Assoziations-Ereignisraum als Produkt über Sensoren).
- *(C) Tor bei `0.999` belassen* — Workaround, behebt die Ursache nicht.

**Für Ursache 2 (Ausreißer-Spawn):**
- *(D) Getrenntes, weiteres Initiierungs-Sperr-Tor* — ein enges
  **Assoziations**-Tor (Zustandspräzision) und ein weiteres
  **Initiierungs**-Tor; ein Plot im weiteren Tor eines bestehenden Tracks
  startet **keinen** neuen Track (er ist ein Ausreißer eines bekannten Ziels).
  Klassisches „Two-Gate"-Schema der Surveillance-Literatur.
- *(E) Höheres Prozessrausch* — weitet das Tor global, verschlechtert aber die
  Präzision auf den Geraden; maskiert nur.

## Entscheidung

**Wir wählen (A) + (D).** Beide Mechanismen sind komplementär und zusammen
robust für synchrone *und* (mit ADR 0012) asynchrone Sensoren:

1. **Gemeinsame, zu Scan-Beginn eingefrorene Fusions-Referenz.** `process_scan`
   bildet **eine** Referenzliste `reference` aus den Track-Schätzungen zu
   Scan-Beginn. *Alle* Sensoren gaten und assoziieren gegen diese Referenz —
   nie gegen den schon aktualisierten Live-Track. Damit gibt es keine
   sequenzielle Verengung mehr. Die Zustands-Fusion bleibt sequenziell
   (`update_pda` je Sensor), was für unabhängige Messungen exakt das gemeinsame
   Update ist. Ein **während** des Scans neu geborener Track wird an
   `reference` angehängt — aber erst *nach* dem Sensor-Block, sodass er den
   **nächsten** Sensor vetoiert (ein Flugzeug, zwei Radare → ein Track), nicht
   die Geschwister-Plots desselben Sensors (verschiedene Ziele desselben
   Sensors müssen je einen Track starten).

2. **Getrenntes Initiierungs-Sperr-Tor.** `TrackerConfig` bekommt zusätzlich zum
   Assoziations-Tor `gate` (`P_G = 0.99`) ein weiteres `init_gate`
   (`P_G = 0.9999`). Die Initiierung wird über `init_gate` unterdrückt: ein
   Plot innerhalb des weiteren Tors eines bestehenden Tracks startet keinen
   neuen Track. Die **Assoziation** nutzt weiterhin das enge `gate`, sodass die
   Zustandsschätzung präzise bleibt.

Das künstlich geweitete Assoziations-Tor der Showcase-Szene (`0.999`) entfällt
damit; sie läuft wieder mit dem Standard `0.99` und exakt acht Tracks.

## Konsequenzen

**Positiv**
- Geister-Tracks sind an der **Ursache** behoben, nicht maskiert; das
  Assoziations-Tor bleibt eng (präzise Zustände).
- Single-Sensor-Verhalten ist **unverändert** (Referenz = Prädiktion, ein
  Sensor; das enge Tor entscheidet die Initiierung wie bisher, sobald
  `init_gate` ≥ `gate`).
- Das Two-Gate-Schema entspricht gängiger Surveillance-Praxis und ist
  rückverfolgbar getestet (FR-TRK-020).

**Negativ / Grenzen**
- `init_gate` ist eine weitere Stellgröße. Zu weit gewählt, könnte es ein
  *echtes* neues Ziel dicht neben einem bestehenden Track unterdrücken. Für die
  hier vorkommenden Abstände (das JPDA-Nahpaar von ~150 m existiert ab `t=0`
  als zwei Tracks und braucht keine Neu-Initiierung) ist `0.9999` sicher.
- Die Assoziations-Gewichte (`β`) werden gegen die eingefrorene Prädiktion statt
  den live aktualisierten Track berechnet — eine milde, in der PDA-Praxis übliche
  Näherung („gate against the prediction").
- Echtes simultanes Multi-Sensor-JPDA (Option B) bleibt eine mögliche spätere
  Verfeinerung für sehr dichten Verkehr.
