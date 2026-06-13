# Firefly — Projekt-Charter & Arbeitsregeln

> Dieses Dokument ist die **verbindliche Arbeitsvereinbarung** zwischen dem
> Projektverantwortlichen (Mensch) und dem KI-Assistenten (Claude). Claude liest
> diese Datei zu Beginn jeder Sitzung und hält sich an die hier festgelegten
> Regeln.

> 📌 **Sitzungsstart:** Zuerst `docs/STATUS.md` lesen — dort steht der aktuelle
> Arbeitsstand und der nächste Schritt (wichtig fürs geräteübergreifende
> Weiterarbeiten). Am Sitzungsende `docs/STATUS.md` aktualisieren.

> ⚙️ **Betriebsmodus:** Dieses Projekt wird **für den realen Betrieb** gebaut,
> nicht als Lernübung. Maßstab ist Produktionsreife: Korrektheit, Robustheit,
> Sicherheit, Betreibbarkeit und Zertifizierungs-Fähigkeit. (Frühere Sitzungen
> hatten einen ausdrücklichen Lern-/Didaktik-Charakter; der ist bewusst
> aufgegeben — siehe ADR 0014.)

---

## 1. Worum es geht

Firefly ist ein **Radar-Tracker** — das Rechen-Herzstück einer
Luftlagedarstellung. Aus den verrauschten Einzelmeldungen von Primär- (PSR) und
Sekundärradar (SSR) werden saubere, durchgehende **Tracks** berechnet (Position,
Geschwindigkeit, Identität jedes Luftfahrzeugs).

**Integrationsziel (ADR 0006).** Firefly soll perspektivisch den
Legacy-Phoenix-Tracker in der Plattform *Phoenix WebInnovation* ablösen und das
bestehende **ASD** (Air Situation Display) sowie weitere Konsumenten über
**ASTERIX CAT062 über UDP-Multicast** speisen. Der Tracker-Kern bleibt
format-neutral (`SystemTrack`); Encoder und Transport sind austauschbare Adapter
(Ports & Adapters). Daraus folgt der Maßstab: **operativ einsetzbare Genauigkeit,
deterministische Reproduzierbarkeit und ein stabiler, dokumentierter
Ausgabe-Kontrakt.**

**Schwester-Projekt Wayfinder (ADR 0014).** Der CAT062/UDP-Multicast-Strom von
Firefly wird von **Wayfinder** (eigenes Repo, eigener Charter) als produktiver
ASD-Konsument empfangen, dekodiert und dargestellt. Die Schnittstelle zwischen
beiden ist der **CAT062-Draht-Vertrag** — kein gemeinsamer Code, keine
Punkt-zu-Punkt-Kopplung. Änderungen am Ausgabe-Format sind
**schnittstellen-relevant** und werden via ADR dokumentiert und mit Wayfinder
abgeglichen (Abschnitt 9).

---

## 2. Arbeitsablauf: **Erst abstimmen, dann bauen**

Claude baut **keinen** nennenswerten Code, ohne vorher den nächsten Schritt
angekündigt und eine Freigabe eingeholt zu haben. Das ist ein **Design-/
Review-Tor**, kein Lern-Ritual: Es verhindert überraschende Architektur-Sprünge
und hält die Richtung abgestimmt. Pro Arbeitsschritt gilt dieser Ablauf:

1. **Ankündigen** — Was kommt als Nächstes? Getrennt nach:
   - **Fachlich** (*Warum* braucht ein Radar-Tracker / das ASD das? Welches
     operative Problem löst es?)
   - **Technisch** (*Wie* setzen wir es um? Welche Bausteine, welche Mathematik,
     welche Dateien, welche Schnittstellen-Wirkung?)
   - **Komplexität & Modell** (Einstufung **S1–S5** mit Modell-Empfehlung, siehe
     unten).
2. **Freigabe abwarten** — Claude hält an und wartet auf Rückfragen oder ein
   „Go". Erst dann wird implementiert.
3. **In kleinen, testbaren Häppchen umsetzen** — Lieber drei kleine, je für sich
   abgeschlossene Schritte als ein großer Sprung.
4. **Nachbereiten** — Doku aktualisieren (Meilenstein-/Feature-Doku, ggf. ADR,
   Anforderungs-Register), Tests grün, dann committen.

**Verboten:** „Durchrattern" — mehrere Bausteine ungefragt hintereinanderweg
bauen, ohne Abstimmung und Freigabe dazwischen.

### Komplexitäts-Skala & Modell-Angabe (Pflicht)

Jeder angekündigte Schritt bekommt eine Einstufung, **und das verwendete bzw.
empfohlene Modell wird genannt** — sowohl für den Schritt selbst als auch für
jede an einen Subagenten/Task delegierte Arbeit (Werkzeug-Läufe). Die Einstufung
schätzt, *wie anspruchsvoll* das saubere Umsetzen ist (Mathe, Algorithmik,
Architektur-Abwägung, Testumfang) — nicht bloß die Zeilenzahl.

| Stufe | Bedeutung | Modell-Empfehlung | Effort-Level |
|-------|-----------|-------------------|--------------|
| **S1** | Trivial/mechanisch (Doku-Kleinkram, Umbenennen) | Haiku 4.5 | niedrig |
| **S2** | Leicht (klar umrissen, wenig Logik) | Haiku 4.5 / Sonnet 4.6 | niedrig–mittel |
| **S3** | Mittel (etwas Mathe/Logik, überschaubarer Umfang) | Sonnet 4.6 | mittel |
| **S4** | Anspruchsvoll (subtile Mathe/Algorithmen, Architektur, viele Tests) | Opus 4.8 / Fable 5 | hoch |
| **S5** | Sehr anspruchsvoll (tiefe Mathe, Fusion, große Architektur-Abwägungen) | Fable 5 / Opus 4.8 | hoch–max |

Faustregel: **S1–S2 → Haiku**, **S3 → Sonnet**, **S4–S5 → Opus 4.8 oder Fable 5**.
Das **Effort-Level** mit der Stufe mitziehen. Bei Grenzfällen mit Sicherheits-
oder Schnittstellen-Wirkung lieber das stärkere Modell.

> **Hinweis Fable 5:** Spitzenmodell der Claude-Familie; gleichrangig mit
> Opus 4.8 für S4–S5 — nach Erfahrung kalibrieren.

---

## 3. Sprache

- **Chat und Dokumentation (`docs/`, `CLAUDE.md`):** Deutsch.
- **Quellcode (Bezeichner, Kommentare im Code):** Englisch — internationaler
  Industriestandard, hält den Code portabel und anschlussfähig.
- Bewusste Entscheidung (ADR 0002), jederzeit änderbar.

---

## 4. Dokumentationspflichten

Dokumentation ist **Teil der Leistung** — sie ist zugleich Audit- und
Zertifizierungs-Nachweis (ADR 0004). Vier Ebenen:

| Ebene | Ort | Zweck |
|-------|-----|-------|
| **Code-Doku** | Doc-Kommentare (`//!`, `///`) | Erklären das *Warum* eines Moduls/Typs, nicht nur das *Was*. |
| **Feature-/Meilenstein-Doku** | `docs/milestones/` | Pro Baustein eine präzise Erklärung: Fachlichkeit + Technik + Mathematik. |
| **Glossar** | `docs/glossary.md` | Domänen-Referenz; jeder Fachbegriff einmal sauber definiert (Onboarding/Audit). |
| **Entscheidungen** | `docs/decisions/` | Architecture Decision Records (ADR): *welche* Entscheidung *warum*. |

Regeln:
- Jeder neue Baustein bekommt **vor dem Abschluss** seine Doku in
  `docs/milestones/`.
- Jede architektonisch relevante Weichenstellung bekommt einen ADR.
- **Schnittstellen-Änderungen** (CAT062-Ausgabe-Vertrag) werden als ADR
  festgehalten und mit Wayfinder abgeglichen (Abschnitt 9).
- Das Glossar wird bei jedem neuen Begriff gepflegt.

---

## 5. Qualitäts-Gates (vor jedem Commit)

Ein Schritt gilt erst als fertig, wenn:

- [ ] `cargo test --workspace` ist grün.
- [ ] `cargo clippy --workspace --all-targets` ist ohne Warnungen.
- [ ] `cargo fmt` wurde ausgeführt.
- [ ] Kein `unsafe`-Code ohne ausdrückliche, dokumentierte Begründung
      (Assurance: Speicher-/Thread-Sicherheit ist ein Kernargument, ADR 0004).
- [ ] Neue/​geänderte Anforderungen sind im Anforderungs-Register
      (`docs/requirements/`) eingetragen und mit Code/Test rückverfolgbar.
- [ ] Bei Änderung der CAT062-Ausgabe: Schnittstellen-Doku aktualisiert,
      Auswirkung auf Wayfinder bewertet (Abschnitt 9).
- [ ] Die zugehörige Doku wurde aktualisiert.
- [ ] Der Commit hat eine klare, beschreibende Nachricht.

---

## 6. Git & Branches

- Entwicklung **immer** auf dem vereinbarten Feature-Branch
  (`claude/loving-turing-2obzk6`).
- Niemals ungefragt auf einen anderen Branch pushen.
- **Kein** Pull Request, außer der Projektverantwortliche bittet ausdrücklich
  darum.
- Commits klein und thematisch geschnitten; eine Sache pro Commit.

---

## 7. Stand & Roadmap

Die Meilensteine **M1–M6** sind implementiert und zu `main` gemergt
(Simulator, Single-Radar-Tracker, Live-Lagebild, Multi-Radar-Fusion, IMM/JPDA,
Showcase + Container). Der aktuelle Detail-Stand und der konkrete nächste Schritt
stehen in `docs/STATUS.md`.

**Produktions-Phase (laufend, ADR 0014):**

| Vorhaben | Inhalt | Status |
|----------|--------|--------|
| **ADR 0014** | Pivot Lernprojekt → Produktion; Wayfinder konsumiert CAT062/UDP | ✅ akzeptiert |
| **UTC Time-of-Day** | Echtes ASTERIX-ToD statt "Sekunden seit Szenario-Start" in I062/070 (ehem. Issue #9) | ⏳ geplant |
| **Multicast-Feed-Sicherheit** | Authentizität/Netz-Isolation des CAT062-Eingangspfads (ehem. Issue #7, transformiert) | ⏳ geplant |
| **Konfigurierbarer System-Referenzpunkt** | I062/100-Referenzpunkt jenseits des Demo-Ursprungs (ADR 0006, offen) | ⏳ geplant |
| **CAT062-ICD** | Versionierte Schnittstellen-Doku für Wayfinder | ⏳ geplant |
| **ADR 0013** | Asynchrone Pro-Plot-Verarbeitung + periodischer Ausgabetakt (13.1–13.7) | ⏳ angenommen, Umsetzung offen |
| **Betriebs-Härtung** | Observability-Ausbau, Lastfestigkeit, Deployment | ⏳ |

Innerhalb jedes Vorhabens wird in kleinen Schritten nach Abschnitt 2 gearbeitet.

---

## 8. Querschnitts-Prinzipien (gelten in *jedem* Schritt)

Zwei nicht-funktionale Anforderungen prägen die gesamte Architektur und werden
**nicht nachträglich angebaut**, sondern von Anfang an mitgedacht:

### Cloud-nativ (anbieter-neutral, Kubernetes-tauglich) — siehe ADR 0003
- **Deterministische Verarbeitung nach Datenzeit**: Der Tracker wird durch die
  Zeitstempel *in den Daten* (ASTERIX Time-of-Day) getrieben, nicht durch die
  Wanduhr. Gleicher Input → gleicher Output. (Reproduzierbarkeit, Replay,
  Wiederherstellung.)
- **Zustand ist explizit und wiederherstellbar**, nicht im Prozess „versteckt".
- **Entkopplung über Datenstrom**, keine harten Punkt-zu-Punkt-Bindungen.
- **12-Factor-Konfiguration**, Health-/Readiness-Probes, sauberes Herunterfahren.
- **Observability** (strukturierte Logs, Metriken, Tracing) ist Pflicht — sie
  dient zugleich als Audit-Nachweis.

### Zertifizierungs-fähig (Orientierung ED-153 + ED-109A/DO-278A) — siehe ADR 0004
- **Rückverfolgbarkeit**: Anforderung → Design → Code → Test, in beide Richtungen.
- **Verifikationsnachweise**: Tests mit gemessener Abdeckung, Reviews, Analysen.
- **Konfigurationsmanagement**: Versionskontrolle, getaggte Baselines, ADRs.
- **Analysierbarkeit**: Rusts Sicherheit nutzen, `unsafe` vermeiden.
- **Ehrliche Grenze**: Wir bauen *zertifizierungs-fähig*. Die formale
  Zertifizierung selbst (Safety Case, SMS, unabhängige V&V, Regulator) ist ein
  organisatorisch-regulatorischer Schritt und nicht Teil dieses Code-Projekts —
  das versprechen wir nicht.

---

## 9. Cross-Project-Todos (Firefly ↔ Wayfinder)

Firefly und Wayfinder sind getrennte Projekte mit getrennten Claude-Sitzungen.
Eine Claude-Sitzung hat Zugriff auf **beide** Repos. Das ermöglicht **direkte
Cross-Project-Kommunikation über GitHub Issues**.

### Workflow

1. **Beobachtung** — Während der Arbeit an Firefly erkennt Claude ein Thema,
   das Wayfinder betreffen könnte (z. B. Schnittstellen-Änderungen am
   CAT062-Ausgabe-Kontrakt).
2. **Issue erstellen** — Claude erstellt ein Issue im anderen Repo mit Label
   `from-firefly` (oder `from-wayfinder`).
3. **Dokumentieren** — Die Issue wird referenziert in
   `docs/cross-project/todo-for-<anderes-projekt>.md`.
4. **Checken** — Beim Sitzungsstart: Offene Issues aus der anderen Sitzung
   ansehen (GitHub-Issues mit `from-firefly` oder `from-wayfinder`).
5. **Aktualisieren** — Wenn ein Issue erledigt ist, wird es geschlossen und
   die Referenz in der `.md`-Datei aktualisiert.

### Dateien

- **`docs/cross-project/todo-for-wayfinder.md`** — Probleme/Wünsche aus
  Firefly, die Wayfinder-Arbeit beeinflussen.
- **`docs/cross-project/todo-for-firefly.md`** — Probleme/Wünsche aus
  Wayfinder, die Firefly-Arbeit beeinflussen.

Siehe auch `docs/cross-project/README.md` für die Hintergrund-Erklärung.

### Stand nach ADR 0014 (CAT062-Pivot)

Die ursprünglichen Issues #6–#10 (`from-wayfinder`) waren gegen den
JSON/WebSocket-Pfad formuliert. Nach dem Pivot auf CAT062/UDP (ADR 0014):

- **#6** (Pub/Sub-Fan-out), **#8** (Typ-Diskriminator), **#10** (Schema-
  Versionierung) — **geschlossen**, durch die Multicast-/ASTERIX-Architektur
  gegenstandslos.
- **#7** (Auth auf `/ws`) — **transformiert**: Sicherheitsfrage verschiebt sich
  auf Netz-Isolation des Multicast-Pfads + Browser-Rand von Wayfinder.
- **#9** (UTC Time-of-Day) — **bleibt offen und wird zentraler**: CAT062
  I062/070 ist das Time-of-Day-Feld; siehe Roadmap (Abschnitt 7).

---

## 10. Was Claude NICHT tut

- Nicht mehrere Bausteine ungefragt hintereinander bauen (kein „Durchrattern").
- Keine großen, überraschenden Architektur-Sprünge ohne ADR und Freigabe.
- Den CAT062-Ausgabe-Vertrag nicht still ändern — Schnittstellen-Wirkung auf
  Wayfinder immer bewerten (Abschnitt 9).
- Nicht „fertig" melden, solange die Qualitäts-Gates (Abschnitt 5) nicht erfüllt
  sind.
- Korrektheit und Sicherheit nicht dem Tempo opfern.
