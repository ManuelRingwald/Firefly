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

**ASTERIX** (*All-purpose Structured Eurocontrol Surveillance Information
Exchange*)
Das europäische Standard-Datenformat, in dem Radare und Systeme ihre Meldungen
austauschen. In „Kategorien" gegliedert:
- **CAT048:** Einzelradar-Zielmeldungen (Plots/Tracks eines Radars).
- **CAT021:** ADS-B-Meldungen.
- **CAT062:** fertige System-Tracks (die fusionierte Luftlage).

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

**Track-Lebenszyklus (tentativ / bestätigt)**
Die „Lebensphasen" eines Tracks: Er wird **tentativ** (auf Probe) geboren,
wird nach Bewährung (M-aus-N) **bestätigt** (*confirmed*) und der Luftlage
gemeldet, und wird wieder **gelöscht**, wenn er zu lange nicht mehr gesehen
wird. Verhindert, dass Falschalarme sofort als „echte" Flugzeuge erscheinen.

**M-aus-N**
Die Bestätigungsregel: Ein tentativer Track wird bestätigt, sobald er in den
letzten **N** Scans mindestens **M** Treffer hatte (z. B. 3 aus 5).

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
- **CT** (*Coordinated Turn*): saubere Kurve mit konstanter Drehrate.

**IMM** (*Interacting Multiple Model*)
Lässt mehrere Bewegungsmodelle parallel laufen und gewichtet sie laufend — gut
für Flugzeuge, die mal geradeaus fliegen, mal Kurven fliegen.

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
wird.

**χ²-Verteilung (Chi-Quadrat) & Freiheitsgrade**
Die Verteilung, der die quadrierte Mahalanobis-Distanz folgt, wenn die Modelle
stimmen. Die *Freiheitsgrade* entsprechen der Zahl der Messdimensionen (bei uns
2: Ost/Nord). Aus ihr leiten wir die Gate-Schwelle ab.

**Gate-Wahrscheinlichkeit `P_G`**
Die Chance, dass ein *echter* Plot innerhalb des Gates landet. Größer = weniger
verpasste echte Plots, aber mehr hereingelassener Clutter. Bestimmt die
χ²-Schwelle `γ` (für 2 Freiheitsgrade: `γ = −2·ln(1−P_G)`).

**RMSE** (*Root Mean Square Error*, Wurzel des mittleren quadratischen Fehlers)
Eine Kennzahl, wie weit die Schätzung im Schnitt von der Wahrheit abweicht.
Damit messen wir später, *ob* der Tracker gut funktioniert.

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

**Back-Pressure (Lastpuffer/Gegendruck)**
Mechanismus, der einen überlasteten Empfänger schützt, indem der Sender
gebremst wird — statt dass etwas abstürzt oder Daten verloren gehen.

**12-Factor**
Eine bekannte Sammlung von Bau-Prinzipien für cloud-taugliche Dienste (z. B.
Konfiguration über Umgebungsvariablen statt fest im Code).

**Health-/Readiness-Probe**
Kleine Selbstauskünfte eines Dienstes: „Lebe ich noch?" (health) und „Bin ich
bereit, Last anzunehmen?" (readiness). Kubernetes nutzt sie zum Steuern.

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
