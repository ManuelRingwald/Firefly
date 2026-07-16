# ADR 0042 — Arbeitsteilung Firefly + Wayfinder = SDPS-Server-Funktion (CAT252-Ersatz)

- **Status:** **AKZEPTIERT** ✅ (2026-07-16, Betreiber-Go).
- **Datum:** 2026-07-16
- **Schnittstellen-relevant:** **nein** — der CAT062/063/065-Draht-Vertrag
  (ICD 3.7.0) bleibt byte-identisch. Dieser ADR ist **rein dokumentarisch**:
  er schreibt eine **gelebte** Architektur-Entscheidung fest, statt neue zu
  treffen; **kein Code, keine neuen Env-Variablen** in diesem Schritt.
- **Bezug:** **ADR 0006** (Integrationsziel: CAT062/UDP-Multicast als
  Ausgabe-Kontrakt, Ports & Adapters), **ADR 0014** (Wayfinder als produktiver
  ASD-Konsument), **ADR 0017** (Multicast-Vertrauensgrenze), **ADR 0018**
  (CAT065-Heartbeat = Service-Status), **ADR 0038** (Korrelation im SDPS,
  ARTAS-Arbeitsteilung SDPS vs. CWP). Wayfinder-seitig: **ADR 0005/0014**
  (Multi-Mandanten als einziger Modus), **ADR 0007** (Cloud-Ingest &
  Feed-Fan-out: `FeedSource`, Ingest-Gateway, NATS), **ADR 0012**
  (Mandanten-Tracker-Orchestrierung: 1 Firefly je Feed/Mandant),
  **WF2-21.2** (server-seitiger, fail-closed AOI/FL-View-Filter),
  **ADR 0021** (AoR/AoI/Track-Scope-Begriffsmodell). Referenzrahmen:
  EUROCONTROL **ARTAS** (Tracker **und** Server) mit dem
  Konsumenten-Protokoll **CAT252**.
- **Anforderungs-Register:** **keine neue Anforderung** — die
  Roadmap-Messlatte (`docs/design/artas-gap-roadmap.md` §1) definiert
  ausdrücklich: bewusste Abweichungen vom ARTAS-Vorbild zählen als erfüllt,
  **sobald sie per ADR festgeschrieben sind**. Genau das leistet dieser ADR
  (SRV.1); funktionale Folge-Anforderungen entstehen erst mit SRV.2
  (Laufzeit-Steuerung).

> ℹ️ **Auslöser:** Roadmap-Häppchen SRV.1. ARTAS — unser funktionales
> Vorbild — besteht aus **zwei** Hälften: dem *Tracker* (rechnet das
> Luftlagebild) und dem *Server* (verwaltet Konsumenten und liefert jedem
> einen **zugeschnittenen** Track-Dienst über CAT252). Firefly hat bewusst
> keinen solchen Server eingebaut; die Server-Leistungen erbringt
> arbeitsteilig Wayfinder. Diese Arbeitsteilung wird seit Monaten gelebt
> (ADR 0006/0014 hier, ADR 0005/0007/0012 dort), war aber nie als
> **Entscheidung mit Begründung und Grenzen** festgeschrieben. Das holt
> dieser ADR nach — einschließlich der Frage, was ein Konsument tut, der
> in keines der vorgesehenen Muster passt.

---

## Kontext

### Was die ARTAS-Server-Funktion leistet (Referenz)

Der ARTAS-Server bedient **„User"** (Konsumenten-Systeme) individuell:

1. **Subscription-Verwaltung** — ein Konsument meldet sich an und bestellt
   einen Dienst (Track-Information-Service); der Server kennt und verwaltet
   jeden Konsumenten einzeln.
2. **Zuschnitt je Konsument** — Liefergebiet (AOI), Filterkriterien,
   Update-Verhalten werden **pro Abo** vereinbart; jeder User bekommt
   *seinen* Strom.
3. **Adressierte Zustellung** — die Verbindung ist Punkt-zu-Punkt zum
   angemeldeten User (klassisch über das Protokoll **CAT252**), nicht
   Broadcast an alle.
4. **Dienst-Status** — der Server meldet seinen Service-Zustand an die User.

### Was bei uns bereits existiert (am Code geerdet)

| ARTAS-Server-Leistung | Bei uns erbracht durch | Beleg |
|-----------------------|------------------------|-------|
| Subscription-Verwaltung | **Wayfinder**: Mandanten + Feed-Katalog + Abos (`subscriptions`), Feed-Join zur Laufzeit | Wayfinder ADR 0011/0012 |
| Zuschnitt je Konsument (AOI/FL) | **Wayfinder**: server-seitiger, **fail-closed** View-Filter am WS-Rand (AOI-BBox + FL-Band je Mandant), property-/fuzz-getestet | WF2-21.2, Wayfinder ADR 0021 |
| Zuschnitt je Konsument (Sensor-Mix/Coverage) | **Wayfinder-Orchestrierung an der Quelle**: 1 Firefly-Instanz je Feed/Mandant mit eigenem `FIREFLY_SOURCES`-Mix + `FIREFLY_COVERAGE_BBOX` | Wayfinder ADR 0012, Firefly ADR 0023 |
| Adressierte Zustellung | **Wayfinder**: WebSocket = authentifizierte Punkt-zu-Punkt-Verbindung je Client (Browser-Rand, TLS/Auth) | Wayfinder ADR 0003/0014 |
| Zustellung über Netzgrenzen | **Wayfinder**: Ingest-Gateway + Bus transportiert die **Roh-Datagramme** unverändert in die Cloud (Fan-out an N Instanzen) | Wayfinder ADR 0007 |
| Dienst-Status | **Firefly**: CAT065-SDPS-Heartbeat (I065/040 NOGO) an alle — im Strom selbst statt je User | ADR 0018 |

Firefly selbst kennt **keinen einzigen Konsumenten**: Es sendet ein
Lagebild als Fire-and-Forget-Multicast (CAT062/063/065) und hält keinerlei
Empfänger-Zustand.

### Spannungsfeld

Ohne Festschreibung drohen zwei Fehlentwicklungen: (a) jemand „vervollständigt"
Firefly später um einen CAT252-artigen Session-Server — und zieht damit
Konsumenten-Zustand in den sicherheitskritischen Pfad; (b) die Roadmap-Lücke
„Server-Funktion" bleibt formal offen, obwohl die Leistung längst erbracht
wird — nur eben an anderer Stelle der Gesamtarchitektur.

---

## Entscheidung

### 1. Die SDPS-Server-Funktion ist eine **Verbund-Leistung** aus Firefly + Wayfinder

Das Gesamtsystem „Firefly + Wayfinder" erbringt die ARTAS-Server-Funktion
**arbeitsteilig**, mit einer harten Rollengrenze:

- **Firefly = Erzeugung + Verteilung an alle.** Ein Lagebild je Instanz,
  ausgesandt als selbstbeschreibender ASTERIX-Strom (CAT062/063/065) über
  UDP-Multicast — **fire and forget**. Firefly verwaltet keine Konsumenten,
  hält keinen Empfänger-Zustand, passt nichts pro Empfänger an. Der native
  Multicast-Fanout *ist* die Verteil-Schicht: N Konsumenten kosten den
  Tracker nichts.
- **Wayfinder = Konsumenten-Verwaltung + Zuschnitt.** Anmeldung,
  Berechtigung, Abo (Mandant ↔ Feeds), Liefergebiet (AOI-BBox + FL-Band,
  server-seitig, fail-closed) und adressierte Zustellung (authentifizierter
  WebSocket je Client) liegen bei Wayfinder — **außerhalb** des
  Track-Rechenpfads. Der Sensor-Mix je Konsument wird **an der Quelle**
  realisiert: eine dedizierte Firefly-Instanz je Feed (Wayfinder ADR 0012),
  konfiguriert über den generischen Quell-Kontrakt (ADR 0023) — Firefly
  bleibt dabei mandanten-blind.

### 2. Konsumenten-Matrix: welcher Konsument bekommt den Strom **wie**?

Für Konsumenten jenseits des Wayfinder-Browsers gilt diese abgestufte
Anschluss-Leiter (adressierte Dienste sind **Optionen**, kein Pflichtausbau):

| # | Konsument | Anschlussweg | Status |
|---|-----------|--------------|--------|
| K1 | ASTERIX-fähiges System **im Multicast-Segment** (z. B. weiteres ASD, Recorder, COMPASS) | **Direkt der Gruppe beitreten** — die ICD (`docs/ICD-CAT062.md`) ist der vollständige Vertrag; kein Anmelden nötig, Staleness via CAT065 | ✅ heute möglich |
| K2 | ASTERIX-fähiges System **ohne Multicast-Zugang** (Cloud, geroutetes Netz) | **Ingest-Gateway/Bus** (Wayfinder ADR 0007): transportiert die **unveränderten Roh-Datagramme** Punkt-zu-Punkt — kein Format-Bruch, Firefly unberührt | ✅ Architektur steht |
| K3 | System, das einen **zugeschnittenen** Dienst braucht (nur mein Gebiet/FL-Band, Web-tauglich) | **Wayfinder als Serving-Punkt**: Mandant + View-Konfiguration + WS-Abo (WF2-21.2) — liefert Wayfinders **JSON-Vertrag**, kein ASTERIX | ✅ heute möglich |
| K4 | Konsument mit eigenem **Sensor-Mix-/Coverage-Vertrag** | **Eigene Firefly-Instanz je Feed** (Wayfinder ADR 0012) — der „adressierte Dienst" entsteht an der Quelle, nicht im Verteilweg | ✅ Architektur steht |
| K5 | Legacy-System, das **CAT252 spricht** und nichts anderes | **Nicht bedienbar** — bewusst; siehe Punkt 3 und „Ehrliche Grenzen" | ❌ bewusst nicht gebaut |

### 3. CAT252 wird **nicht** implementiert

Ein CAT252-artiger Subscription-Server in Firefly ist verworfen — nicht
aufgeschoben, sondern entschieden:

- **Konsumenten-Zustand im kritischen Pfad:** CAT252 ist ein
  session-behaftetes Anmelde-Protokoll. Jede User-Verwaltung im Tracker
  koppelt die Lagebild-Erzeugung an das Verhalten einzelner Empfänger
  (Reconnects, Backpressure, Abo-Mutationen) — genau die Kopplungsklasse,
  die ADR 0006 mit „Entkopplung über Datenstrom" ausgeschlossen hat.
- **Kein Replay-/Fanout-Gewinn:** Der Multicast-Fanout skaliert im Netz;
  ein Session-Server müsste denselben Strom je User erneut erzeugen.
- **Kein Bedarf im Zielbild:** Der einzige produktive Konsument (Wayfinder)
  und alle in ADR 0012 vorgesehenen Konsumenten-Muster kommen ohne aus (K1–K4).

**Falls** ein künftiger Konsument CAT252 **vertraglich** erzwingt, entsteht
ein **separater Protokoll-Adapter am Rand** (eigener Dienst, der als
gewöhnlicher K1/K2-Konsument den Multicast liest und CAT252 nach außen
spricht) — per neuem ADR, ohne Änderung am Tracker-Kern. Die
Ports-&-Adapters-Struktur (ADR 0006) hält diesen Weg offen.

---

## Begründung

- **Die Leistung existiert — nur der Nachweis fehlte.** Jede Zeile der
  ARTAS-Server-Leistungstabelle ist mit gelebtem Code/ADR belegt; dieser ADR
  macht die Verteilung der Verantwortung explizit und auditierbar
  (Roadmap-Messlatte §1: Abweichung + ADR = erfüllt).
- **Sicherheitsargument:** Der Track-Rechenpfad bleibt frei von
  Empfänger-Zustand. Ausfall, Überlast oder Fehlverhalten eines Konsumenten
  kann die Lagebild-Erzeugung **prinzipiell nicht** beeinflussen — beim
  klassischen Server-Modell ist das eine zu erkämpfende Eigenschaft, bei
  Fire-and-Forget-Multicast eine strukturelle.
- **Klare Zuständigkeit statt Doppel-Logik:** Zuschnitt (AOI/FL) lebt genau
  einmal, in Wayfinder, fail-closed und getestet (WF2-21.2). Firefly-seitige
  Coverage (ADR 0012) ist davon getrennt begründet (Rechenlast an der
  Quelle), nicht als zweiter Anzeige-Filter.

### Verworfene Alternativen

- **CAT252-Server in Firefly:** siehe Punkt 3 — Konsumenten-Zustand im
  kritischen Pfad, ohne Gewinn. Verworfen.
- **Eigenständiger Track-Server-Dienst zwischen Firefly und Konsumenten**
  (dritte Komponente, die abonniert/filtert/adressiert): reproduziert
  Wayfinders Serving-Schicht als Parallelbau und schafft eine zweite
  Filter-Wahrheit. Verworfen — Wayfinder *ist* dieser Dienst.
- **Per-Konsument-Unicast direkt aus Firefly** (Liste von Ziel-Adressen):
  wäre Empfänger-Zustand light — Retries, tote Adressen, Konfig-Drift landen
  im Tracker. Verworfen; K2 löst denselben Fall am Rand (Gateway).

---

## Konsequenzen

- **Rein dokumentarisch:** dieser ADR + Glossar-Einträge (ARTAS-Server-Funktion,
  CAT252) + Roadmap-Haken SRV.1. Kein Code, keine Env-Variablen; ICD,
  INSTALLATION und TECHNICAL unverändert (geprüft).
- **Wayfinder-Spiegel:** Issue (`from-firefly`) im Wayfinder-Repo, damit die
  dortige Doku (Charter §1/ADR-Verzeichnis) den Verbund-Charakter der
  Server-Funktion mit einem Verweis auf diesen ADR festhält — Wayfinders
  eigene ADRs (0005/0007/0012, WF2-21.2) tragen ihre Hälfte bereits.
- **SRV.2 bleibt eigenes Häppchen:** Laufzeit-Steuerung (Sensor an/aus,
  Service-Kommandos via API) + Supervision-Ausbau — dort entstehen
  funktionale Anforderungen und Code, getrennt anzukündigen.
- **Leitplanke für die Zukunft:** Wer einen neuen Konsumenten anbindet,
  ordnet ihn zuerst in die Matrix K1–K5 ein. Ein Vorschlag, der auf „Firefly
  merkt sich Empfänger" hinausläuft, widerspricht diesem ADR und braucht
  dessen explizite Ablösung.

## Ehrliche Grenzen

- **Kein ARTAS-kompatibler CAT252-Endpunkt.** Ein Bestands-System, das
  ausschließlich CAT252 spricht, kann heute **nicht** andocken (K5). Der
  dokumentierte Ausweg (Adapter am Rand) ist ein Konzept, kein Code.
- **Zugeschnittene Dienste liefern Wayfinders JSON, kein ASTERIX.** Wer
  „gefiltertes CAT062" braucht, fällt derzeit auf K1/K2 (ungefiltert) oder
  einen künftigen Adapter zurück.
- **Keine Ratenanpassung je Konsument.** Firefly sendet einen Ausgabetakt
  für alle (ADR 0013); ein Konsument, der z. B. nur alle 30 s ein Bild
  will, muss selbst ausdünnen.
- **Dienst-Status ist Broadcast, nicht Abo-Quittung.** CAT065 sagt „der
  Dienst lebt/degradiert" an alle; eine per-User-Bestätigung („dein Abo
  ist aktiv") existiert nur am Wayfinder-Rand (WS-Session), nicht im Strom.
