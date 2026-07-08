# Glossar — Fachbegriffe in einfacher Sprache

Dieses Lexikon wächst mit dem Projekt. Jeder Begriff wird einmal so erklärt, dass
man ihn ohne Vorwissen versteht — oft mit einer Analogie. Reihenfolge:
thematisch, nicht alphabetisch, damit Zusammenhänge sichtbar bleiben.

---

## Luftlage & Sensorik (die Fachwelt)

**ANSP** (*Air Navigation Service Provider*, Flugsicherungsorganisation)
Eine Organisation, die den Luftverkehr überwacht und lenkt (z. B. DFS in
Deutschland, Austro Control in Österreich). Sie betreibt die Radare und die
Systeme, die daraus ein Luftlagebild erzeugen.

**Luftlage / Luftlagedarstellung**
Das Gesamtbild „Wer fliegt gerade wo, wie schnell, in welche Richtung?". Der
Tracker ist das Rechenherz, das dieses Bild aus rohen Radarmeldungen erzeugt.

**ASD (*Air Situation Display*)**
Die Lagedarstellung am Lotsenarbeitsplatz — der Bildschirm, der die berechneten
Tracks zeigt. Ein *Konsument* des Trackers: Firefly soll später genau dieses ASD
beliefern (siehe ADR 0006).

**EFS (*Electronic Flight Strips*)**
Elektronische Flugstreifen, die die früheren Papierstreifen ersetzen. Brauchen
Tracks, die mit Flugplänen/Callsign korreliert sind (→ Identitätsarbeit in M4).

**Phoenix WebInnovation**
Die cloud-native Plattform des Projektverantwortlichen mit ASD + EFS, heute vom
Legacy-Phoenix-Tracker gespeist. Zielumgebung, an die Firefly andocken soll.

**Primärradar (PSR, *Primary Surveillance Radar*)**
Sendet einen Funkimpuls aus und misst das Echo, das vom Flugzeugrumpf
zurückgeworfen wird — wie eine Taschenlampe, deren Lichtreflex man sieht. Es
liefert nur **Entfernung und Richtung**, *keine* Identität und *keine* Höhe. Es
sieht aber auch Objekte, die nicht „antworten" wollen.

**Sekundärradar (SSR, *Secondary Surveillance Radar*)**
Stellt eine Frage („Interrogation") an das Flugzeug; ein Gerät an Bord (der
**Transponder**) antwortet aktiv. Dadurch bekommt man zusätzlich Identität und
Höhe — aber nur von kooperierenden Flugzeugen. Wie ein Zuruf „Wer da?", auf den
das Flugzeug seinen Namen und seine Höhe zurückruft.

**Transponder**
Das Antwortgerät an Bord, das auf SSR-Abfragen reagiert.

**Mode A / Mode C / Mode S** (SSR-Antwortarten)
- **Mode A:** liefert den **Squawk** — einen 4-stelligen Code (oktal, 0000–7777),
  den der Lotse dem Flug zuweist, zur Unterscheidung.
- **Mode C:** liefert die **barometrische Höhe** (aus dem Luftdruck), meist als
  *Flugfläche*.
- **Mode S:** moderner, selektiv adressierbar; liefert u. a. die weltweit
  eindeutige **24-Bit-ICAO-Adresse** des Flugzeugs und Zusatzdaten.

**Squawk (Mode-3/A-Code)**
Der 4-stellige Identifikationscode, z. B. „7000". `7700` = Notfall, `7600` =
Funkausfall, `7500` = Entführung.

**Flugfläche (FL, *Flight Level*)**
Höhe in Hunderten Fuß, bezogen auf einen genormten Standard-Luftdruck
(1013,25 hPa). „FL350" ≈ 35 000 Fuß ≈ 10 668 m. Wichtig: bezogen auf Druck, nicht
auf den echten geometrischen Abstand zum Boden.

**ADS-B** (*Automatic Dependent Surveillance – Broadcast*)
Das Flugzeug bestimmt seine Position selbst (per Satellitennavigation) und
funkt sie laufend aus. Kein Radar nötig — der Empfänger hört einfach zu.

**Community-Aggregator (ADS-B)**
Ein von Freiwilligen betriebener Dienst, der die ADS-B-Empfänge tausender
privater Boden-Empfänger einsammelt und als **offenes, auth-freies** API wieder
ausgibt — z. B. **adsb.lol** oder **adsb.fi**. Beide sprechen dasselbe
ADSBExchange-v2-kompatible JSON-Format (Punkt+Radius-Abfrage, max. 250 NM),
sodass Fireflys `adsb_aggregator`-Adapter (ADR 0031) mehrere Anbieter mit einem
Code bedient. Zweiter ADS-B-Bezugsweg neben **OpenSky** (dem
forschungsorientierten Aggregator mit OAuth2-Zugang, ADR 0019/0024) — Community-
wie Forschungs-Aggregatoren liefern Hobby-/Forschungsqualität, keine
zertifizierte Surveillance.

**ASTERIX** (*All-purpose Structured Eurocontrol Surveillance Information
Exchange*)
Das europäische Standard-Datenformat, in dem Radare und Systeme ihre Meldungen
austauschen. In „Kategorien" gegliedert:
- **CAT048:** Einzelradar-Zielmeldungen (Plots/Tracks eines Radars).
- **CAT021:** ADS-B-Meldungen.
- **CAT062:** fertige System-Tracks (die fusionierte Luftlage).
- **CAT063:** Sensor-Status-Meldungen — Per-Sensor-Liveness des SDPS (ADR 0022,
  UAP-Standardisierung ADR 0032, per-Quelle-Fehlergrund ADR 0033).
- **CAT065:** SDPS-Service-Status — periodischer „Heartbeat", der „leeren Himmel"
  von „totem Feed" unterscheidbar macht (ADR 0018).

Firefly sendet CAT062/CAT063/CAT065 auf **derselben** Multicast-Gruppe/Port; der
Empfänger dispatcht am führenden CAT-Oktett (`0x3E`/`0x3F`/`0x41`).

ASTERIX ist **bit-genau und binär**: Ein Datenblock ist `[CAT][LEN][Record…]`
(CAT = Kategorie-Nummer, LEN = Gesamtlänge), jeder Record beginnt mit einem
*FSPEC* (s. u.), gefolgt von den vorhandenen *Data Items* in fester Reihenfolge.

**Data Item (Datenfeld) / I062/NNN**
Ein einzelnes, genormtes Feld innerhalb einer Kategorie — z. B. `I062/070`
(Zeitstempel) oder `I062/105` (Position). `NNN` ist die feste Feldnummer im
Standard. Jedes Item hat eine definierte Byte-Breite und, bei Zahlen, einen
festen *LSB* (s. u.).

**FSPEC** (*Field Specification*, Feld-Spezifikation)
Die einleitende **Bitmaske** eines Records: Jedes der sieben oberen Bits eines
Octets sagt „dieses Datenfeld ist vorhanden ja/nein"; das unterste Bit (**FX**,
*Field Extension*) bedeutet „es folgt noch ein FSPEC-Octet". So weiß der
Empfänger, welche Felder in welcher Reihenfolge kommen — ohne dass leere Felder
übertragen werden müssen.

**UAP** (*User Application Profile*)
Die verbindliche **Zuordnung** „welches Bit der FSPEC steht für welches
Datenfeld". Das Bit wird über die **FRN** (*Field Reference Number*, laufende
Nummer im UAP) adressiert: FRN 1 = oberstes Bit des ersten Octets, FRN 8 =
oberstes Bit des zweiten Octets usw.

**SAC/SIC** (*System Area Code / System Identification Code*)
Die zweiteilige **Quell-Kennung** in ASTERIX (Datenfeld I062/010): *wer* hat die
Meldung erzeugt — welche geografische Stelle (SAC) und welches System dort (SIC).

**SDPS** (*Surveillance Data Processing System*, Radardatenverarbeitungssystem)
Das System, das die Meldungen der Einzelsensoren (Radare, ADS-B) zu einem
fusionierten Luftlagebild verarbeitet — bei uns **Firefly selbst**. In ASTERIX ist
das SDPS der *Absender* von CAT062 (Tracks), CAT063 (Sensor-Status) und CAT065
(SDPS-Service-Status/Heartbeat). Seit ADR 0032 trennt CAT063 sauber: **I063/010**
trägt die **SDPS-Identität** (SAC/SIC, Default 25/2 — *wer* meldet), das separate
**I063/050** die **Sensor-Identität** (SAC 0, SIC = `sensor_id` — *worüber*). So
ist ein einzelner ausgefallener Sensor erkennbar, obwohl das SDPS selbst
ungestört weiterläuft.

**RE / SP** (*Reserved Expansion Field / Special Purpose Field*)
Zwei im ASTERIX-UAP vorgesehene **Erweiterungs-Slots** am Ende eines Records, über
die ein Hersteller **eigene** Zusatzfelder mitschicken darf, ohne den Standard zu
brechen. Beide sind **selbst-begrenzend**: ihr erstes Octet ist eine Länge, die
das ganze Feld (inkl. sich selbst) zählt — so kann ein Decoder, der das Feld nicht
kennt, es einfach **überspringen**. Firefly nutzt das RE-Feld von CAT063 (FRN 13)
für den **`SRC-REASON`** (s. u.).

**SRC-REASON** (Firefly-Vendor-Subfeld im CAT063-I063/RE, ADR 0033)
Der **Ausfallgrund einer Quelle**, den Firefly bei einem *degradierten* Sensor
mitschickt, damit der Lotse/Betreiber weiß **warum** eine Quelle still ist:
`1 = unreachable` (Netz/Firewall — Zugangsdaten sind ok), `2 = auth` (falsche/
fehlende Zugangsdaten, HTTP 401/403), `3 = rate_limited` (Drosselung, HTTP 429).
Ein operationeller Sensor trägt keinen Grund. So spart sich der Betreiber
sinnloses Nachtippen von Credentials, wenn in Wahrheit eine Firewall blockiert.

**LSB / Skalierungsfaktor** (*Least Significant Bit*)
Der Wert, den das **kleinste Bit** eines Festkomma-Felds darstellt — z. B. ist
der LSB der CAT062-Position `180/2²⁵` Grad. Ein Bruchwert (etwa 12,0 Sekunden)
wird so zu einer ganzen Zahl von LSB-Schritten (12,0 s ÷ (1/128 s) = 1536).

**Festkomma (*fixed-point*)**
Eine Zahl als **Ganzzahl mal festem LSB** kodieren, statt als Fließkomma. In
ASTERIX die übliche Form — kompakt, eindeutig und ohne Rundungs-Überraschungen
des Fließkomma-Formats.

---

## Tracking-Grundbegriffe (was der Tracker tut)

**Plot (Zielmeldung)**
Eine **einzelne Detektion** eines Ziels bei **einem** Antennenumlauf eines
Radars: „Zum Zeitpunkt t war bei dieser Entfernung/Richtung etwas." Roh,
verrauscht, evtl. fehlend. Der Rohstoff.

**Track (Spur)**
Die **fortlaufend geschätzte Bahn** eines Objekts über die Zeit, zusammengesetzt
aus vielen Plots: geglättete Position, Geschwindigkeit, Richtung, Identität. Das
Endprodukt. Aus vielen Einzelpunkten („Plots") wird eine durchgehende Linie.

**Scan / Antennenumlauf**
Eine Radarantenne dreht sich. Ein voller Umlauf (z. B. alle 4 s) ist ein „Scan";
pro Scan entsteht höchstens ein Plot je Ziel.

**Erfassungswahrscheinlichkeit (Pd, *Probability of Detection*)**
Die Chance, ein vorhandenes Ziel bei einem Scan tatsächlich zu sehen (z. B. 0,9
= 90 %). Real verpasst ein Radar manchmal ein Ziel — der Tracker muss damit
umgehen.

**Falschalarm / Clutter**
Eine Detektion, hinter der gar kein echtes Flugzeug steckt (Vögel, Wetter,
Reflexionen). Der Tracker muss solche „Geister" aussortieren.

**Track-Initiierung**
Das Geburtsverfahren eines Tracks: Aus bisher nicht zugeordneten Plots wird ein
neuer („tentativer") Track gebildet, der sich erst über mehrere Scans bewähren
muss (oft „M aus N": z. B. in 3 von 4 Scans wiedergesehen).

**Coasting**
Wenn ein bestätigter Track in einem Scan keinen Plot bekommt (Ziel kurz nicht
gesehen), „segelt" er auf Basis der Vorhersage weiter, statt sofort zu sterben.
Das ist ein Vorgang **im Tracker**: dem Track *fehlt* eine Messung, seine
Unsicherheit wächst wirklich. Nicht zu verwechseln mit dem *Anzeige*-
Dead-Reckoning (siehe unten), bei dem die Daten nur *spät* ankommen.

**Dead-Reckoning (Koppelnavigation, Anzeige-Ebene)**
Überbrückt eine *Zustell*-Lücke (das Netz ist langsam, der „Verzug"-Knopf):
Die Anzeige rechnet jedes Track-Symbol aus seiner letzten bekannten
Geschwindigkeit **weiter**, damit der Flieger nicht in der Luft stehenbleibt.
Kommen die echten Daten wieder, **schnappt** das Symbol auf die Wahrheit. Reine
Anzeige-Überbrückung — die Track-Daten selbst bleiben unangetastet
(NFR-CLOUD-004). Anders als beim Coasting *fehlen* keine Messungen; sie sind nur
verspätet. Damit das Bild nicht dauerhaft nachhängt, holt der Server den
Rückstand danach auf (absolutes Pacing, „springt nach vorn").

**History-Trail / Kometenschweif**
Auf einem echten Radarschirm verschwinden Plots und vergangene Track-Positionen
nicht sofort, sondern bleiben als **verblassende Spur** stehen. Firefly zeichnet
dafür die Roh-Plots und die letzten Track-Positionen der jeweils letzten
Sekunden, deren Helligkeit mit dem Alter abnimmt.

**Update-Alter (*Track Update Age*)**
Wie viel **Datenzeit** seit dem letzten *realen Treffer* eines Tracks vergangen
ist. 0 s = gerade frisch gemessen; wächst, solange der Track coastet. Sagt dem
Verbraucher (Anzeige), wie „frisch" eine Spur ist — ohne die Wanduhr. In ASTERIX
CAT062 als I062/290 geführt.

**Track-Lebenszyklus (tentativ / bestätigt)**
Die „Lebensphasen" eines Tracks: Er wird **tentativ** (auf Probe) geboren,
wird nach Bewährung (M-aus-N) **bestätigt** (*confirmed*) und der Luftlage
gemeldet, und wird wieder **gelöscht**, wenn er zu lange nicht mehr gesehen
wird. Verhindert, dass Falschalarme sofort als „echte" Flugzeuge erscheinen.

**M-aus-N**
Die Bestätigungsregel: Ein tentativer Track wird bestätigt, sobald er in den
letzten **N** Scans mindestens **M** Treffer hatte (z. B. 3 aus 5).

**Revisit-Intervall**
Die Zeit zwischen zwei *Treffern* (echten Messungen) desselben Tracks. Bei
einem einzelnen Radar ≈ dessen Scan-Periode (z. B. 4 s); sehen mehrere Radare
dasselbe Ziel, ist es kürzer. Der adaptive Lebenszyklus (ADR 0012) schätzt das
Revisit-Intervall pro Track per EWMA und zählt Bestätigungs-/Löschfenster in
*Vielfachen* davon, statt in festen Scan-Aufrufen.

**EWMA (*Exponentially Weighted Moving Average*)**
Ein „gleitender Mittelwert mit Gedächtnis": der neue Wert geht mit einem festen
Gewicht (z. B. 0,5) ein, der bisherige Mittelwert mit dem Rest. Reagiert
schneller auf Änderungen als ein einfacher Durchschnitt über alle bisherigen
Werte, glättet aber einzelne Ausreißer (z. B. einen verpassten Treffer).

**Feed-Kadenz**
Die vom Tracker beobachtete Grund-Taktung des gesamten Plot-Stroms — das
Maximum aus der Lücke seit dem letzten Scan-Aufruf und der größten bekannten
Scan-Periode eines einzelnen Sensors. Verhindert, dass die kurze Lücke
*zwischen* zwei versetzt scannenden Radaren (ADR 0012) als „überfällige
Wiederkehr" eines Tracks missverstanden wird, der nur von einem der beiden
gesehen wird.

**Gating**
Bevor man fragt „Welcher Plot gehört zu welchem Track?", grenzt man den
Suchbereich ein: Nur Plots in einem plausiblen Fenster um die Vorhersage kommen
in Frage. Spart Rechenzeit und verhindert Unsinn. Das „Tor" (Gate) ist dieses
Plausibilitätsfenster.

**Datenassoziation**
Die Kernfrage: Welcher der vorhandenen Plots gehört zu welchem Track? Verfahren
(von einfach zu komplex):
- **NN / GNN** (*(Global) Nearest Neighbor*): nimm den/die nächstgelegenen Plots.
- **JPDA / MHT**: berücksichtigen mehrere Möglichkeiten gleichzeitig — wichtig
  bei dichtem Verkehr, wo Zuordnungen mehrdeutig sind.

**Kalman-Filter**
Das mathematische Herzstück der Glättung. Es kombiniert (a) eine **Vorhersage**
(„wo müsste das Ziel jetzt sein, wenn es so weiterfliegt?") mit (b) einer neuen,
verrauschten **Messung**, und gewichtet beide nach ihrer Unsicherheit. Ergebnis:
eine bessere Schätzung als jede der beiden allein. Analogie: ein erfahrener
Beobachter, der seine Erwartung und das, was er gerade sieht, klug
zusammenbringt.

**Zustand / Zustandsvektor**
Die Größen, die der Tracker über ein Ziel schätzt und mitführt — bei uns
`[Ost, Nord, Geschwindigkeit-Ost, Geschwindigkeit-Nord]`. Die „aktuelle
Annahme", wo das Ziel ist und wohin es fliegt.

**Prozessrauschen `Q` (Manöver-Budget)**
Der Regler dafür, wie sehr der Filter dem Bewegungsmodell misstraut. Echte
Flugzeuge fliegen nicht perfekt gleichförmig — `Q` lässt Abweichungen zu. Zu
klein: der Filter „klebt" stur an der Geraden und verliert Kurven. Zu groß: er
zappelt dem Rauschen hinterher. Eine zentrale Stellschraube der Track-Güte.
`Q` muss zur **erwarteten Manövrierfähigkeit** passen: Für eine sanfte 1°/s-Kurve
(≈3,7 m/s²) braucht es deutlich mehr als der für Geradeausflug gewählte Standard
(Faustregel `q ≳ a²·Δt`). Für *starke* Manöver ist die saubere Antwort kein
einzelnes `Q`, sondern **IMM** (mehrere Modelle parallel, Meilenstein M5).

**Innovation**
Die „Überraschung" eines neuen Plots: die Differenz zwischen dem, was gemessen
wurde, und dem, was der Filter vorhergesagt hatte (`y = Messung − Vorhersage`).

**Kalman-Gain (Vertrauens-Hebel)**
Bestimmt, wie stark eine neue Messung die Schätzung korrigiert. Präzise Messung
(kleines `R`) → großer Hebel, der Filter folgt der Messung; grobe Messung →
kleiner Hebel, der Filter bleibt eher bei seiner Vorhersage.

**Joseph-Form**
Eine besonders stabile Rechenform für das Aktualisieren der Unsicherheit `P` im
Kalman-Filter. Sie hält `P` auch bei Rundungsfehlern gültig (symmetrisch und
positiv definit) — wichtig für verlässliche, prüfbare Numerik.

**Bewegungsmodell**
Die Annahme darüber, *wie* ein Ziel sich bewegt:
- **CV** (*Constant Velocity*): gleichförmig geradeaus.
- **CA** (*Constant Acceleration*): gleichmäßig beschleunigend.
- **CT** (*Coordinated Turn*): saubere Kurve mit konstanter Drehrate `ω` (rad/s).
  Im Tracker (`MotionModel::CoordinatedTurn`) dreht die Übergangsmatrix den
  Geschwindigkeitsvektor pro Schritt um `ω·dt` und integriert den entstehenden
  Kreisbogen in die Position; der Betrag der Geschwindigkeit (die „Speed")
  bleibt dabei erhalten. Für `ω → 0` wird daraus exakt wieder CV — Geradeausflug
  ist also nur der Sonderfall „Drehrate null". Ein CV-Filter allein „hinkt" in
  Kurven hinterher (er sagt immer geradeaus voraus); ein CT-Modell, das die
  Drehrate kennt, folgt dem Bogen. Beide Modelle teilen denselben 4-D-Zustand
  `[Ost, Nord, v_Ost, v_Nord]`, damit derselbe Filter sie austauschbar nutzen —
  und der **IMM** mehrere davon parallel mischen — kann.

**IMM** (*Interacting Multiple Model*)
Lässt mehrere Bewegungsmodelle parallel laufen und gewichtet sie laufend — gut
für Flugzeuge, die mal geradeaus fliegen, mal Kurven fliegen. Jedes Modell hat
eine eigene Filter-Schätzung *und* eine **Modellwahrscheinlichkeit** `μ` (wie gut
es die Messungen gerade erklärt). Das ausgegebene Ergebnis ist die mit `μ`
gewichtete Mischung — auf der Geraden trägt das CV-Modell, in der Kurve das
CT-Modell, und der Übergang ist weich. Ein IMM-Zyklus hat vier Stufen:
**Mischung/Interaktion** (jedes Modell startet aus einem gewichteten Mix aller
Modelle), **modellbedingtes Filtern** (jedes Modell prädiziert + aktualisiert),
**Modellwahrscheinlichkeits-Update** (die Likelihoods justieren `μ` neu),
**Kombination** (gewichtetes Zusammenführen zur Ausgabe).

**Mischung / Interaktion (IMM-Mixing)**
Die erste IMM-Stufe und das, was die Filter überhaupt koppelt: Bevor ein Modell
für sich filtert, startet es nicht aus seiner *eigenen* letzten Schätzung,
sondern aus einer **Mischung** aller Modell-Schätzungen — gewichtet damit, wie
wahrscheinlich ein Ziel gerade *in dieses* Modell gewechselt ist. So erbt selbst
ein eben noch unwahrscheinliches Modell einen sinnvollen Startzustand, wenn das
Ziel gerade dorthin manövriert. Die gemischte Unsicherheit bekommt zusätzlich
einen **„Spread-of-the-Means"-Term**: Sind sich die Modelle über den Zustand
uneins, ist der gemischte Start ehrlich unsicherer.

**Markov-Übergangsmatrix (Modellwechsel)**
Beschreibt, wie ein Ziel zwischen den Bewegungsmodellen springt: `π_ij` ist die
Wahrscheinlichkeit, im nächsten Scan von Modell `i` nach Modell `j` zu wechseln.
Jede Zeile summiert sich zu 1 (*zeilenstochastisch*). Eine hohe Diagonale heißt
„Modelle sind träge" (selten Wechsel), kleine Nebendiagonalen erlauben
gelegentliches Umschalten — die zentrale Stellschraube, wie flink der IMM auf
ein Manöver reagiert.

**Likelihood (eines Modells)**
Wie gut ein Modell den gerade eingetroffenen Plot *vorhergesagt* hat — die
Gauß-Dichte `N(y; 0, S)` der Innovation `y` unter der Innovations-Kovarianz `S`.
Ein Plot, der dort landet, wo das Modell ihn erwartet hat (kleine Innovation),
bekommt eine hohe Likelihood; eine Überraschung eine niedrige. Im IMM ist die
Likelihood das **Beweisstück**: das Modell mit der höheren Likelihood gewinnt
Modellwahrscheinlichkeit `μ` hinzu (`μ_j ∝ c_j·Λ_j`). So „erkennt" der IMM, ob
das Ziel gerade geradeaus fliegt oder kurvt — ganz ohne separaten
Manöver-Detektor.

**PDA** (*Probabilistic Data Association*)
Die „weiche" Alternative zu GNN: Statt einem Track *einen* Plot fest zuzuweisen,
betrachtet PDA **alle** Plots im Tor gleichzeitig und gewichtet jeden mit einer
**Assoziationswahrscheinlichkeit** `β` — wie wahrscheinlich ist *dieser* Plot die
wahre Rückmeldung? Zusätzlich gibt es `β_0`: die Wahrscheinlichkeit, dass *gar
kein* Plot im Tor zum Ziel gehört (Fehldetektion oder reiner Clutter). Die `β`
summieren sich zu 1 und gehen gewichtet ins Filter-Update ein — eine
Fehlentscheidung bei mehrdeutiger Lage wirkt sich so weniger dramatisch aus als
bei GNNs hartem 0/1-Pick.

**Assoziationswahrscheinlichkeit `β` (Beta)**
Das Gewicht, mit dem PDA/JPDA einen Plot (oder „kein Treffer", `β_0`) in die
Schätzung einfließen lässt. Ergibt sich aus dem Verhältnis „wie gut passt dieser
Plot zur Vorhersage" (Likelihood `Λ`, s. o.) zu „wie plausibel ist Clutter/keine
Detektion" (Term `b`, aus dem Clutter-Modell). Ein Plot, der perfekt auf die
Vorhersage passt und in einer ruhigen (clutterarmen) Umgebung liegt, bekommt ein
`β` nahe 1; in dichtem Clutter sinkt sein `β` zugunsten von `β_0`.

**Clutter-Dichte `λ` (Falschalarm-Dichte) / Clutter-Modell**
Wie viele Falschalarme pro Flächeneinheit (z. B. pro km²) im Mittel zu erwarten
sind — ein Maß für „wie verrauscht/unruhig ist die Umgebung". Zusammen mit der
Detektionswahrscheinlichkeit `P_D` (s. *Erfassungswahrscheinlichkeit*) bildet sie
das **Clutter-Modell** (`ClutterModel`), aus dem PDA/JPDA den Term `b` ableiten —
je höher `λ` oder je niedriger `P_D`, desto eher erklärt PDA einen Plot im Tor
als „nur Rauschen" (`β_0` steigt).

**JPDA** (*Joint Probabilistic Data Association*)
Die Erweiterung von PDA auf **mehrere Tracks gleichzeitig**, wenn sich ihre Tore
überlappen — etwa zwei Flugzeuge im Formationsflug. Die Kernidee:
**Exklusivität** — ein einzelner Plot kann in einem „gemeinsamen Ereignis" nicht
gleichzeitig zu Track A *und* Track B gehören. JPDA zählt alle so zulässigen
gemeinsamen Zuordnungen auf, gewichtet jede nach Plausibilität und summiert
daraus die `β_ij` je Track-Plot-Paar — eine Verfeinerung gegenüber „jeder Track
rechnet PDA für sich", die der gegenseitigen Konkurrenz um dieselben Plots
gerecht wird.

**Cluster (JPDA)**
Eine Gruppe von Tracks und Plots, die — direkt oder über mehrere Schritte —
durch gemeinsame Tore miteinander verbunden sind (z. B. Track A teilt sich ein
Tor mit Plot X, und Plot X liegt auch im Tor von Track B → A, B und X bilden ein
Cluster). Innerhalb eines Clusters muss JPDA die Exklusivität gemeinsam
auflösen; Tracks/Plots *außerhalb* jedes Clusters sind unabhängig und werden wie
gewohnt (PDA bzw. „sicher kein Treffer") behandelt. In der realen Luftlage sind
Cluster meist klein (eine Handvoll Tracks), was die Aufzählung aller Ereignisse
praktikabel hält.

**Track-Koaleszenz (*Track Coalescence*)**
Eine bekannte Eigenheit von PDA/JPDA bei eng benachbarten Zielen: Weil jeder
Plot *weich* (mit `β<1`) auf mehrere Tracks verteilt wird statt ihn fest einem
zuzuschlagen, ziehen sich die Schätzungen mehrerer naher Tracks ein Stück
**aufeinander zu** (zu den geteilten Plots hin), statt exakt getrennt zu bleiben.
Solange die Ziele **auflösbar** sind (siehe *Auflösungsgrenze*), bleiben die
Tracks unterscheidbar — sie rücken nur etwas näher zusammen als ihre wahren
Positionen. **Unterhalb der Auflösungsgrenze** (Ziele näher als ~3–4σ des
Messrauschens) verschmelzen die Schätzungen dagegen vollständig zu einer — das
ist dann aber *keine* JPDA-Schwäche, sondern die korrekte Konsequenz daraus,
dass die Daten die beiden Ziele gar nicht mehr trennen (vgl. ADR 0013).

**Auflösungsgrenze (Radar)**
Der kleinste Abstand, bei dem zwei Ziele noch als *zwei* getrennte Rückmeldungen
erkennbar sind. Praktisch bestimmt vom Messrauschen: Liegen zwei Plots enger als
grob **3–4σ** beieinander (σ = Standardabweichung des Messfehlers, quer zur
Sichtlinie wächst sie mit der Entfernung), überlappen sich ihre
Wahrscheinlichkeits­wolken so stark, dass *kein* Verfahren sie zuverlässig
trennen kann — die Information dafür steckt nicht in den Daten. Wichtige
Konsequenz: Ein Tracker, der zwei sub-σ-nahe Ziele zu einer Spur verschmilzt,
macht keinen Fehler, sondern bildet die physikalische Grenze korrekt ab.

**Identitätstausch (*Track Swap*)**
Wenn ein Tracker zwei sich nahe kommende Ziele zwar als zwei Spuren behält, aber
beim Auseinandergehen die **Identitäten vertauscht** — Spur A folgt plötzlich
Ziel B und umgekehrt. Typische Gefahr bei einer *harten* 1:1-Zuordnung am
Kreuzungspunkt. JPDA beugt dem vor, indem es jede Spur über ihren eigenen
**Geschwindigkeitszustand** (Bewegungsmodell) durch die Mehrdeutigkeit trägt:
Wer von links-unten nach rechts-oben fliegt, taucht nach der Kreuzung auch
rechts-oben wieder auf. Der Frankfurt-Showcase prüft genau das (kreuzende
Ziele, Kurs bleibt im richtigen Quadranten).

**Zuordnungsproblem (*assignment problem*)**
Die Aufgabe, Zeilen (Tracks) und Spalten (Plots) einer Kostentabelle so paarweise
zuzuordnen, dass die Gesamtkosten minimal werden — jede Zeile/Spalte höchstens
einmal. Das mathematische Gerüst hinter GNN.

**Ungarische Methode (Kuhn–Munkres)**
Ein Standard-Algorithmus, der das Zuordnungsproblem **exakt und effizient**
(`O(n³)`) löst — global optimal statt gierig.

**Multi-Radar-Fusion**
Mehrere Radare sehen dasselbe Ziel. Fusion bedeutet, ihre Meldungen zeitlich
abzugleichen, systematische Messfehler (Bias) zu korrigieren und zu *einem*
gemeinsamen Track zusammenzuführen.

**Mess-Fusion (zentrales Tracking)** vs. **Track-Fusion (track-to-track)**
Zwei Wege, mehrere Radare zu fusionieren (siehe ADR 0010):
- **Mess-Fusion:** *alle* Plots aller Sensoren laufen in **einen** Tracker, der
  **ein** gemeinsames Lagebild pflegt. Genauer (verarbeitet Rohmessungen direkt),
  braucht aber einen gemeinsamen Koordinatenrahmen. *Unsere Wahl für M4.*
- **Track-Fusion:** *jeder* Sensor hat seinen **eigenen** Tracker (lokale Tracks);
  eine Schicht darüber verschmilzt die zusammengehörigen lokalen Tracks. Modular,
  aber das Fusionieren bereits gefilterter Tracks ist mathematisch heikler.

**Track-to-Track-Assoziation**
Bei der Track-Fusion: die Frage „Sind dieser lokale Track von Radar A und jener
von Radar B *dasselbe* Flugzeug?". Bei der Mess-Fusion entfällt sie, weil es von
vornherein nur *einen* Track je Flugzeug gibt.

**Lokaler Track vs. System-Track**
Ein **lokaler Track** ist die Schätzung *eines einzelnen* Sensors (sein eigener
Tracker). Der **System-Track** ist das *fusionierte*, nach außen gegebene
Ergebnis über alle Sensoren (in ASTERIX: CAT062). Bei zentraler Mess-Fusion
(unsere Wahl) gibt es gar keine separaten lokalen Tracks — der eine Tracker
liefert direkt System-Tracks.

**Sensor-Registrierung / Sensor-Bias**
**Bias** ist ein *systematischer* (nicht zufälliger) Messfehler eines Sensors —
z. B. ein Radar, dessen Azimut konstant um 0,2° verdreht oder dessen Entfernung
um 150 m versetzt ist. **Registrierung** ist das Vermessen und Herausrechnen
dieser Versätze, damit zwei Radare dasselbe Flugzeug an *denselben* Ort legen.
Bleibt der Bias unkorrigiert, zieht die Fusion ein Flugzeug auseinander (Geist/
Duplikat). Eigenes, späteres Thema (für beide Fusionswege gleich).

**Gemeinsamer Tracking-Frame / System-Referenzpunkt**
Der eine lokale ENU-Bezugsrahmen, in dem der zentrale Tracker rechnet —
unabhängig von jedem einzelnen Sensorstandort. Jeder Plot wird vor der
Verarbeitung in diesen Rahmen umgerechnet. Es ist zugleich der Bezugspunkt der
System-Stereografischen CAT062-Ausgabe (ADR 0006).

**Höhen-Projektionsfehler (height-projection bias)**
Ein Tücken-Detail der Multi-Radar-Fusion. Ein 2D-Tracker rechnet auf einer
*Bodenebene*. Projiziert man ein **hoch fliegendes** Ziel auf den Boden, hängt
das Ergebnis davon ab, *entlang welcher Vertikalen* man projiziert — und die
lokale „Senkrechte" zeigt an zwei zig Kilometer entfernten Radarstandorten in
leicht **verschiedene** Richtungen (die Erde ist gekrümmt). Projiziert jedes
Radar entlang *seiner eigenen* Vertikalen, landet dasselbe 10-km-Ziel um einige
zehn bis ~100 m **versetzt** — genug, dass die Messung des zweiten Radars aus
dem engen Tor eines schon eingerasteten Tracks fällt und ein **Geister-Track**
entsteht. Die Lösung: nicht im Sensorrahmen auf den Boden projizieren, sondern
den **vollständigen 3D-Punkt** (inkl. Höhe) in den gemeinsamen Tracking-Frame
heben und **erst dort** auf den Boden projizieren — dann ist das Ergebnis für
beide Radare identisch (`LocalFrame::horizontal_from` mit Höhe, FR-GEO-003).
Reale ATC-Systeme korrigieren genau das über höhenabhängige
„System-Error"-Korrekturen.

**Sensor-Provenienz (`contributing_sensors`)**
Welche(r) Sensor(en) im **letzten Scan** zu einem Track beigetragen haben — d. h.
ihn getroffen (geupdated oder neu gegründet) haben. Bei der Mess-Fusion (ADR 0010)
sieht ein fusionierter Track oft mehrere Sensoren gleichzeitig; die Provenienz
macht sichtbar, *wer gerade hinschaut*. Anders als die SSR-Identität (Mode 3/A,
ICAO-Adresse) ist sie **nicht sticky**: jeder Scan setzt die Liste neu — beim
Coasten (kein Sensor hat getroffen) ist sie leer.

**I062/060 (Mode 3/A Code)**
CAT062-Datenfeld für den **Mode-3/A-Code** ("Squawk", ein 4-stelliger
Oktalcode, den der Lotse dem Flugzeug zuweist). Zwei Oktette: die unteren 12
Bit tragen den Code, die oberen Bits sind Validierungs-Flags (V/G/CH), die wir
auf 0 lassen — der Tracker meldet nur einen *bereits bestätigten* Code.

**I062/380 (Aircraft Derived Data) / Target Address (ADR-Subfeld)**
CAT062-Compound-Item für Daten, die aus dem Mode-S-/ADS-B-Signal selbst
abgeleitet sind. Wir kodieren bisher nur das **ADR-Subfeld** ("Target
Address"): die weltweit eindeutige 24-Bit-**ICAO-Adresse** des Flugzeugs — der
Schlüssel, der bei der Multi-Radar-Fusion *dasselbe* Flugzeug über
verschiedene Sensoren hinweg identifiziert.

**I062/100 (Calculated Track Position, Cartesian)**
CAT062-Datenfeld für die Track-Position als **X/Y in Metern** auf der
System-Stereografischen Ebene (siehe oben), relativ zum System-
Referenzpunkt. Sechs Oktette: X (3 Oktette) und Y (3 Oktette), je ein
24-Bit-Zweierkomplement mit LSB 0,5 m. Wird **zusätzlich** zu I062/105
(WGS84) gesendet — das ASD nutzt I062/100 für seine interne Geometrie
(ADR 0006-Nachtrag). Berechnet aus der WGS84-Position des `SystemTrack` über
die System-Stereografische Projektion (`StereographicProjection`,
FR-GEO-004).

**Track-Kontinuität**
Maß dafür, ob *ein* Ziel *eine* durchgehende Track-Spur behält. Zwei Teilzahlen:
**Coverage** (Anteil der Scans, in denen das Ziel überhaupt einen bestätigten
Track hatte — ideal nahe 1) und **ID-Wechsel** (wie oft die Track-ID für dasselbe
Ziel springt — ideal 0; ein Sprung heißt: Spur abgerissen und neu geboren oder
Identität vertauscht).

---

## Geometrie & Koordinaten (wo ist „wo"?)

**Polar vs. kartesisch**
- **Polar:** Position als Entfernung + Winkel — so *misst* ein Radar (es kennt
  „wie weit" und „in welche Richtung").
- **Kartesisch:** Position als X/Y/Z-Werte auf einem Gitter — so *rechnet* man
  bequem (Geschwindigkeiten, gerade Linien).
Der Tracker übersetzt ständig zwischen beiden Welten.

**Range / Azimut / Elevation** (die polaren Messgrößen)
- **Range:** Schrägentfernung vom Radar zum Ziel (Meter).
- **Azimut:** Horizontalwinkel, von Nord im Uhrzeigersinn (0°=Nord, 90°=Ost).
- **Elevation:** Höhenwinkel über dem Horizont.

**WGS84**
Das weltweite Standard-Erdmodell (eine leicht abgeplattete Kugel, „Ellipsoid"),
auf das sich Längengrad/Breitengrad/Höhe beziehen — auch die Grundlage von GPS.

**ECEF** (*Earth-Centered, Earth-Fixed*)
Ein kartesisches Koordinatensystem mit Ursprung im Erdmittelpunkt, das sich mit
der Erde mitdreht. Praktischer Zwischenschritt beim Umrechnen.

**ENU** (*East-North-Up*, lokales Tangentialsystem)
Ein lokales X/Y/Z-System, das in einem Bezugspunkt (z. B. dem Radarstandort) auf
die Erdoberfläche „aufgesetzt" wird: X=Ost, Y=Nord, Z=Hoch. In diesem flachen
System ist Fliegen näherungsweise „geradeaus = gerade Linie" — ideal fürs
Tracking. Wie ein lokaler Stadtplan statt eines Globus.

**Geodäsie**
Die Lehre vom Vermessen der Erde — hier konkret das korrekte Umrechnen zwischen
WGS84, ECEF, ENU und polaren Radarkoordinaten.

**System-Stereografische Projektion**
Eine *flache Karte* der Erdoberfläche um einen festen System-Referenzpunkt
herum (stereografische Projektion — wie eine Schattenwurf-Abbildung der Kugel
auf eine Ebene durch diesen Punkt). ATC-Systeme rechnen intern oft auf so einer
Ebene (X/Y in Metern relativ zum Referenzpunkt), weil sich Geometrie
(Abstände, Winkel, Bildschirm-Pixel) darauf einfacher rechnen lässt als auf der
gekrümmten WGS84-Kugel. Ähnlich wie ENU, aber mit *einem* Referenzpunkt für ein
ganzes System (z. B. einen Flughafen/FIR), nicht pro Sensor. CAT062 kann
Positionen wahlweise in WGS84 (I062/105) oder in dieser System-Ebene (I062/100)
übertragen — siehe ADR 0006.

**Konforme Breite / Gaußsche Kugel** (Bausteine der exakten stereografischen
Projektion)
Das WGS84-Erdmodell ist ein *Ellipsoid* (an den Polen leicht abgeplattet), die
stereografische Projektion ist aber ursprünglich für eine *Kugel* definiert.
EUROCONTROL/ARTAS lösen das mit einem Zwischenschritt: Zuerst wird das
Ellipsoid *winkeltreu* (konform) auf eine Hilfskugel abgebildet, deren Radius
zum System-Referenzpunkt passt (die „Gaußsche Kugel"). Dabei wird aus der
geodätischen Breite (normales Lat/Lon) eine leicht verschobene „konforme
Breite". Erst auf dieser Hilfskugel wird die eigentliche stereografische
Projektion gerechnet. Ergebnis: Winkel und Formen bleiben auch über größere
Entfernungen vom Referenzpunkt erhalten („konform" = winkeltreu) — wichtig,
damit Abstände und Richtungen auf der Karte stimmen.

---

## Mathematik & Statistik (das Handwerkszeug)

**Messrauschen**
Die zufällige Ungenauigkeit jeder Messung. Ein Radar misst nie exakt — die
Entfernung „streut" um den wahren Wert.

**Standardabweichung (σ, „Sigma")**
Ein Maß dafür, *wie stark* eine Größe um ihren Mittelwert streut. Kleines σ =
präzise, großes σ = ungenau.

**Normalverteilung / Gauß**
Die typische „Glockenkurve" der Streuung: kleine Abweichungen häufig, große
selten. Mess- und Modellfehler werden meist so beschrieben.

**Kovarianz / Kovarianzmatrix**
Die „Unsicherheit in mehreren Dimensionen" — beschreibt nicht nur, wie ungenau
einzelne Werte sind, sondern auch, wie ihre Fehler zusammenhängen (z. B.
Position und Geschwindigkeit). Der Kalman-Filter rechnet ständig damit.

**Fehlerellipse / 1σ-Halbachse**
Die anschauliche Form der Positions-Unsicherheit: eine Ellipse (die „Zigarre"),
deren Achsen aus der Kovarianz folgen. Die **lange Halbachse** (Wurzel des
größten Eigenwerts der 2×2-Positions-Kovarianz) ist ein einzelnes, ehrliches Maß
für „wie unsicher ist die Position gerade" — das Maß, das der Tracker als
Positions-Unsicherheit ausgibt (CAT062 I062/500).

**Mahalanobis-Distanz**
Ein „fairer" Abstand, der die Unsicherheit berücksichtigt: Ein Plot, der in
Richtung großer Unsicherheit abweicht, zählt als „näher" als einer, der in
Richtung kleiner Unsicherheit gleich weit weg liegt. Grundlage des Gatings.

**Jacobi-Matrix**
Beschreibt, wie sich kleine Änderungen am *Eingang* einer Umrechnung auf den
*Ausgang* auswirken — die „lokale Umrechnungs-Steigung" in mehreren Dimensionen.
Damit lässt sich eine Unsicherheit von einem Koordinatensystem ins andere
„mitnehmen": `R_neu = J · R_alt · Jᵀ`.

**Converted Measurement (umgerechnete Messung)**
Das Standardverfahren, eine polare Radarmessung (Entfernung, Winkel) in
kartesische x/y zu übersetzen — *samt* ihrer Unsicherheit, die über die
Jacobi-Matrix in die richtige (zigarrenförmige, gekippte) Ellipse umgerechnet
wird. Wichtig: Die **Bodenentfernung** ist `ρ = Schrägentfernung · cos(Elevation)`,
also hängt die *radiale* Unsicherheit nicht nur vom Entfernungs-, sondern auch
vom **Höhenwinkel-Rauschen** ab (`σ_ρ² = (cos φ·σ_r)² + (r·sin φ·σ_φ)²`). Bei
hoch fliegenden Zielen dominiert der zweite Term — lässt man ihn weg, wird das
Gate viel zu eng und Tracks zerbrechen unnötig (gefunden & behoben in M3).

**χ²-Verteilung (Chi-Quadrat) & Freiheitsgrade**
Die Verteilung, der die quadrierte Mahalanobis-Distanz folgt, wenn die Modelle
stimmen. Die *Freiheitsgrade* entsprechen der Zahl der Messdimensionen (bei uns
2: Ost/Nord). Aus ihr leiten wir die Gate-Schwelle ab.

**Gate-Wahrscheinlichkeit `P_G`**
Die Chance, dass ein *echter* Plot innerhalb des Gates landet. Größer = weniger
verpasste echte Plots, aber mehr hereingelassener Clutter. Bestimmt die
χ²-Schwelle `γ` (für 2 Freiheitsgrade: `γ = −2·ln(1−P_G)`).

**RMSE** (*Root Mean Square Error*, Wurzel des mittleren quadratischen Fehlers)
Eine Kennzahl, wie weit die Schätzung im Schnitt von der Wahrheit abweicht:
Wurzel aus dem Mittel der **quadrierten** Einzelfehler. Das Quadrieren bestraft
große Ausreißer stärker als ein simpler Mittelwert. Die Einheit ist die der
Messgröße (bei uns Meter). Damit messen wir, *ob* der Tracker gut funktioniert.

---

## Software-Begriffe (das Werkzeug Rust)

**Rust**
Die Programmiersprache, in der wir die Rechen-Engine bauen. Stark auf
Korrektheit und Geschwindigkeit ausgelegt.

**Crate**
Ein einzelnes Rust-Paket/Baustein (Bibliothek oder Programm). Firefly besteht aus
mehreren Crates, jede mit einer klaren Aufgabe.

**Workspace**
Ein Verbund mehrerer Crates, die zusammen verwaltet und gebaut werden — wie ein
Projektordner mit mehreren Teil-Modulen.

**nalgebra**
Eine etablierte, reine Rust-Bibliothek für lineare Algebra (Vektoren, Matrizen).
Ab dem Tracker (M2) unsere erste externe Abhängigkeit — siehe ADR 0005.

**Serialisierung / serde**
*Serialisieren* = einen Programmzustand in eine speicher-/sendbare Form bringen
(und *deserialisieren* = zurück). **serde** ist der Rust-Standard dafür,
format-neutral (JSON, binär, …). Grundlage für Snapshot/Replay (ADR 0007).

**Ports & Adapters (Hexagonale Architektur)**
Ein Bauprinzip: Der fachliche *Kern* (hier der Tracker) kennt nur neutrale
Schnittstellen („Ports") und bleibt unabhängig von der Außenwelt; konkrete
Anbindungen (Formate wie CAT062, Transporte) stecken in austauschbaren
*Adaptern*. Hält den Kern testbar, portabel und (für uns) zertifizierungs-
freundlich entkoppelt.

**System-Track**
Der *fertige*, aufbereitete Track, wie ihn das Gesamtsystem nach außen gibt
(Position, Geschwindigkeit, Identität, Qualität, Status). In ASTERIX die
Kategorie **CAT062**. Abgrenzung: ein *Plot* ist eine Rohmeldung, ein
*(internal) Track* die laufende Schätzung, der *System-Track* das ausgegebene
Ergebnis.

**Test (Unit-/Integrationstest)**
Kleines Prüfprogramm, das automatisch nachweist, dass ein Stück Code das
Richtige tut. Unsere Absicherung gegen Fehler.

**Clippy / `cargo fmt`**
Werkzeuge, die den Code auf typische Fehler prüfen (Clippy) bzw. einheitlich
formatieren (`fmt`).

**PRNG / Seed**
Ein *Pseudo*-Zufallsgenerator erzeugt Zufallszahlen rechnerisch. Mit demselben
Startwert („Seed") liefert er **exakt dieselbe** Folge — so werden unsere
verrauschten Szenarien reproduzierbar.

**ADR** (*Architecture Decision Record*)
Eine kurze Notiz, die eine wichtige Entscheidung samt Begründung festhält —
damit man später nachvollziehen kann, *warum* etwas so gebaut wurde.

**async / await & Runtime (Tokio)**
*Synchron* heißt: ein Programm tut eine Sache nach der anderen. Ein Server muss
aber vieles **gleichzeitig** bedienen (Verbindungen, Datenstrom, Health-Checks).
*Asynchrones* Programmieren (`async`/`await`) erlaubt das, ohne für jede Aufgabe
einen eigenen Betriebssystem-Thread zu binden; eine **Runtime** verteilt die
Aufgaben auf wenige Threads. **Tokio** ist die verbreitetste Async-Runtime in
Rust — unser Fundament für den M3-Server (ADR 0009).

**axum**
Ein Web-Framework auf Tokio-Basis (aus demselben Ökosystem). Es nimmt
HTTP-/WebSocket-Verbindungen an und ordnet sie „Routen" zu; über Tower-Middleware
lassen sich Health-/Readiness-Probes und sauberes Herunterfahren sauber bauen.
Unsere Wahl für den M3-Server (ADR 0009).

**WebSocket**
Eine **dauerhafte, beidseitige** Verbindung zwischen Browser und Server. Anders
als eine klassische HTTP-Anfrage (Frage → Antwort → zu) bleibt sie offen, sodass
der Server laufend neue Daten „pushen" kann — ideal, um Track-Positionen Scan für
Scan an die Karte zu schicken.

**Frame (Ausgabe-Bild)**
Bei uns: ein **Ausgabe-Paket pro Zeitschritt** — `{ Zeit, Sensor, Liste der
System-Tracks }`, das über die WebSocket-Leitung geht. Nicht zu verwechseln mit
*LocalFrame* (dem geodätischen Bezugssystem); hier meint „Frame" ein einzelnes
Momentbild des Lagebildes.

**Player (Frame-Strom-Erzeuger)**
Die Komponente, die ein Szenario (M1) durch den Tracker (M2) schiebt und daraus
den **Frame-Strom** macht — eine `Frame`-Liste, ein Eintrag pro Scan-Zeit. Der
Player selbst ist **rein und deterministisch** (kein Netz, keine Wanduhr,
ADR 0003): *wann* und *wie schnell* dieser Strom später nach außen geht (Server,
Demo-Tempo), ist eine getrennte Hülle darum.

---

## Frontend & Karte (was der Browser zeigt)

**Frontend**
Der Teil, der im Browser läuft und das Lagebild **darstellt** (HTML/JavaScript +
Karte). Er *rendert* nur, was der Tracker liefert, und trifft **keine**
safety-relevante Entscheidung (ADR 0008).

**Leaflet**
Eine klassische, sehr einfache Karten-Bibliothek (zeichnet Kacheln und Symbole
per Canvas/SVG/DOM). Großer Beispiel-Fundus, flacher Einstieg — für kleine bis
mittlere Objektzahlen völlig ausreichend. (In Firefly *erwogen*, aber zugunsten
von MapLibre verworfen — ADR 0009.)

**MapLibre GL**
Eine quelloffene, **GPU-gestützte** Karten-Bibliothek (zeichnet per WebGL). Sie
skaliert gut zu vielen, häufig aktualisierten Objekten und lässt sich
anbieter-neutral selbst hosten. Unsere Wahl fürs M3-Frontend, mit Blick auf den
dichteren Verkehr in M4 (ADR 0009).

**WebGL**
Eine Browser-Schnittstelle, die Zeichnen direkt über die **Grafikkarte (GPU)**
erlaubt — schneller bei vielen/animierten Objekten als klassisches Zeichnen über
die Seitenstruktur (DOM).

**Vektor-Kachel (*Vector Tile*)**
Karten-Daten, die als **Geometrie** (Linien, Flächen, Punkte) statt als fertiges
Bild ausgeliefert werden. Der Browser zeichnet sie selbst — scharf bei jedem
Zoom, klein in der Übertragung, frei im Stil. Grundlage moderner Vektorkarten wie
MapLibre.

**GeoJSON**
Ein verbreitetes JSON-Format für Geo-Objekte (Punkte, Linien, Flächen mit
Eigenschaften). Das Frontend baut aus jedem `Frame` GeoJSON-Objekte für die
Track-Symbole, Unsicherheits-Ringe und Geschwindigkeitsvektoren und gibt sie an
MapLibre zum Zeichnen.

**demotiles (MapLibre)**
Ein von MapLibre frei gehosteter, einfacher Karten-Stil
(`demotiles.maplibre.org`), den wir als Hintergrund nutzen. Bequem für die
Lern-/Demo-Phase; bedeutet eine *externe* Anfrage zur Laufzeit (ADR 0009). Ein
selbst-gehosteter Stil für volle Souveränität bleibt ein späterer Schritt.

---

## Cloud & Betrieb

**Cloud-nativ**
Software, die *für* die Cloud gebaut ist statt nur *in* die Cloud verschoben:
Sie nimmt an, dass Recheninstanzen jederzeit verschwinden können, dass skaliert
wird und dass Ausfälle normal sind — und kommt damit klar.

**Lift & Shift**
Das Gegenteil: eine alte Anwendung unverändert in einen Container packen und in
der Cloud betreiben. Läuft, nutzt aber keine Cloud-Stärken und „kämpft" oft
gegen die Umgebung.

**Container**
Ein abgeschlossenes „Paket" mit Anwendung und allem, was sie zum Laufen braucht.
Läuft überall gleich, unabhängig vom Wirts-System.

**Kubernetes**
Ein „Dirigent" für Container: startet, überwacht, skaliert und ersetzt sie
automatisch über viele Maschinen hinweg. Anbieter-neutral (läuft bei allen
großen Clouds und On-Prem).

**Anbieter-neutral / On-Prem / Souveräne Cloud**
- *Anbieter-neutral:* nicht an einen Cloud-Konzern gebunden.
- *On-Prem:* im eigenen Rechenzentrum betrieben.
- *Souveräne Cloud:* Betrieb unter rechtlicher/territorialer Kontrolle (z. B.
  EU-/national), für Datensouveränität — bei ANSP oft relevant.

**Zustandsbehaftet (*stateful*) vs. zustandslos (*stateless*)**
Ein *zustandsloser* Dienst kann jederzeit neu gestartet werden, ohne etwas zu
„vergessen". Ein *zustandsbehafteter* Dienst (wie ein Tracker, der Tracks über
die Zeit führt) merkt sich etwas — und genau dieses Gedächtnis muss in der Cloud
wiederherstellbar gemacht werden.

**Datenzeit (*Event-Time*) vs. Verarbeitungszeit**
- *Datenzeit:* der Zeitstempel, der *in* der Meldung steht (wann die Messung
  wirklich war).
- *Verarbeitungszeit:* wann der Rechner sie zufällig bearbeitet.
Wir rechnen nach Datenzeit — das macht das Ergebnis reproduzierbar und
unabhängig von Server-Launen.

**Snapshot / Replay**
- *Snapshot:* ein gespeicherter Stand des Zustands zu einem Zeitpunkt.
- *Replay:* das erneute Abspielen des Eingangsstroms ab einem Snapshot, um den
  Zustand exakt wiederherzustellen — die Grundlage für Ausfallsicherheit.

**Message Bus / Datenstrom**
Ein „Förderband" für Nachrichten zwischen Bausteinen (z. B. Sensoren →
Tracker → Anzeige). Entkoppelt die Teile, erlaubt Skalierung, Puffern und
Wiederabspielen.

**UDP-Multicast**
Ein Netzwerk-Versandverfahren: ein Sender schickt Pakete an eine
*Multicast-Adresse* (eine Art „Funkkanal"), und beliebig viele Empfänger
(ASD, EFS, Recorder, ...) können „mithören", ohne dass der Sender sie kennen
oder einzeln adressieren muss — wie ein Radiosender, der einfach sendet, egal
wie viele Radios gerade eingeschaltet sind. Basiert auf **UDP** (schnell,
verbindungslos, ohne Zustellgarantie) — Standard-Transport für ASTERIX-
Radardaten in der Flugsicherung (ED-109A-Umfeld).

**Back-Pressure (Lastpuffer/Gegendruck)**
Mechanismus, der einen überlasteten Empfänger schützt, indem der Sender
gebremst wird — statt dass etwas abstürzt oder Daten verloren gehen.

**12-Factor**
Eine bekannte Sammlung von Bau-Prinzipien für cloud-taugliche Dienste (z. B.
Konfiguration über Umgebungsvariablen statt fest im Code).

**Health-/Readiness-Probe**
Kleine Selbstauskünfte eines Dienstes: „Lebe ich noch?" (health) und „Bin ich
bereit, Last anzunehmen?" (readiness). Kubernetes nutzt sie zum Steuern.

**Geordnetes Herunterfahren (Graceful Shutdown)**
Beim Stopp-Signal (z. B. SIGTERM, das Kubernetes vor dem Beenden schickt) fährt
der Dienst *kontrolliert* herunter: keine neuen Verbindungen mehr annehmen,
laufende sauber beenden, dann erst aussteigen — statt einfach „abgewürgt" zu
werden. Wichtig, damit beim Skalieren/Neustart in der Cloud nichts hart abreißt.

**Tempo-Faktor / Playback-Geschwindigkeit**
Beim Abspielen eines aufgezeichneten/simulierten Datenstroms: das Verhältnis von
**Datenzeit zu Wanduhr** (z. B. „2× so schnell"). Liegt bewusst am *Ausgabe-Rand*
(Server), nicht im Tracker — den Strom schneller, langsamer oder pausiert
*zuzustellen* ändert keine einzige Track-Entscheidung (ADR 0003, NFR-CLOUD-004).

**Observability (Beobachtbarkeit)**
Die Fähigkeit, von außen zu verstehen, was ein laufendes System tut — über
**Logs** (Ereignis-Protokolle), **Metriken** (Messzahlen) und **Tracing**
(Verfolgen einer Anfrage durch das System). Dient Betrieb *und* Audit.

**Latenz**
Die Verzögerung zwischen Eingang einer Meldung und fertiger Reaktion. Bei
Luftlage soft-echtzeitkritisch — sie muss klein *und vorhersagbar* sein.

---

## Zertifizierung & Assurance

**Zertifizierung / Audit**
Der formale Nachweis (und seine Prüfung), dass ein System die geltenden Vorgaben
erfüllt und sicher betrieben werden darf. Bei ANS überwacht durch
Aufsichtsbehörden.

**Zertifizierungs-*fähig* (unsere Haltung)**
So gebaut und dokumentiert, dass das System in ein Zertifizierungsprogramm
*hineingehen* kann — ohne zu behaupten, das Lernprojekt sei selbst zertifiziert.

**EU 2017/373**
EU-Verordnung mit gemeinsamen Anforderungen an die Erbringung von Flugsicherung.

**ED-153**
EUROCONTROL/EUROCAE-Leitfaden zur Software-Sicherheitsabsicherung; legt das
**SWAL** fest.

**SWAL** (*Software Assurance Level*) / **Assurance Level (AL)**
Eine Einstufung, *wie streng* ein Stück Software abgesichert werden muss —
abhängig davon, wie schlimm ein Fehler wäre. Höhere Stufe = mehr Nachweise.

**ED-109A / DO-278A**
Der maßgebliche Standard für die Software-Integrität von CNS/ATM-*Boden*systemen
(das Boden-Pendant zum Flugzeug-Standard DO-178C). Verlangt u. a. lückenlose
Rückverfolgbarkeit und Verifikationsnachweise.

**Rückverfolgbarkeit (*Traceability*)**
Die durchgehende, in beide Richtungen prüfbare Kette Anforderung → Design →
Code → Test. Kernforderung jedes Audits.

**Verifikation & Validierung (V&V)**
- *Verifikation:* „Bauen wir das System richtig?" (erfüllt es die Anforderungen?)
- *Validierung:* „Bauen wir das richtige System?" (sind es die richtigen
  Anforderungen?)

**Code-Abdeckung (*Coverage*)**
Maß dafür, welcher Anteil des Codes von Tests durchlaufen wird. Höhere
Assurance-Stufen verlangen strengere Abdeckungsarten (bis hin zu *MC-DC*).

**Konfigurationsmanagement (CM)**
Diszipliniertes Verwalten von Versionen, Baselines und Änderungen, sodass
jederzeit klar ist, *welcher* Stand wann galt. (Bei uns: Git, Tags, ADRs.)

**Baseline**
Ein festgehaltener, benannter Stand (z. B. „M1 fertig"), auf den man sich
verlässlich beziehen kann.

**Safety Case**
Eine strukturierte, belegte Argumentation, dass ein System hinreichend sicher
ist. Organisatorisch, nicht bloß technisch.

**FHA / PSSA / SSA** (Sicherheitsbewertung)
Schritte einer Gefährdungs-/Sicherheitsanalyse: *Functional Hazard Assessment*
(welche Fehlfunktionen sind wie schlimm?), *Preliminary/System Safety
Assessment*. Liefert u. a. die nötige Assurance-Stufe.

**Part-IS / ED-205**
Regelwerke zur **Informationssicherheit** in der Luftfahrt (Cyber-Security) am
Boden — zunehmend Pflichtbestandteil.

**`unsafe` (in Rust)**
Ein Schlüsselwort, mit dem man die Sicherheitsgarantien der Sprache bewusst
aushebelt. Wir vermeiden es — sein Fehlen ist ein starkes Assurance-Argument.
