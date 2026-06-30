# ADR 0025 — Assurance-Dokumenten-Architektur (CR → SSS → SRS → STD, Rückverfolgbarkeit)

- **Status:** akzeptiert
- **Datum:** 2026-06-30
- **Schnittstellen-relevant:** nein (Draht-Verträge CAT062/`FIREFLY_SOURCES`
  unverändert). **Assurance-/Prozess-relevant:** ja — etabliert die gestufte
  Anforderungs- und Verifikations-Dokumentation.
- **Auslöser:** Projektverantwortlicher — Wunsch nach einer sektor-üblichen,
  gestuften Struktur (Change Request → SSS → SRS → STD) mit durchgehender
  Rückverfolgbarkeit, als Grundlage für einen **ED-129C-Konformitätsnachweis**.
- **Erweitert:** ADR 0004 (Assurance & Zertifizierungs-Fähigkeit). ADR 0004 bleibt
  gültig; dieser ADR konkretisiert *die Dokumentenform* der dort geforderten
  Rückverfolgbarkeit.

## Kontext

ADR 0004 verlangt durchgehende Rückverfolgbarkeit (Anforderung → Design → Code →
Test) als Zertifizierungs-Nachweis. Umgesetzt ist das heute als **eine flache
Tabelle** in `docs/requirements/README.md` (`FR-…`/`NFR-…`/`CON-…`, Spalten
ID · Anforderung · Status · Nachweis). Das ist eine gute Basis, aber:

- Es gibt **nur eine Ebene** — faktisch Software-Anforderungen. Eine **System**-Sicht
  (was das Gesamtsystem leisten soll, inkl. der Querschnitts-NFRs aus Charta §8)
  ist nicht als verifizierbare System-Anforderung gefasst.
- Es gibt **keinen Bedarfs-Vordersatz** (Change Request) mit eigener ID/Lebenszyklus.
- Die Software-Anforderungen liegen in **einer** Monster-Tabelle statt **je
  Subsystem** (1..n SRS).
- Verifikation steht **inline** als Testname; es fehlt ein eigenständiges
  **Software Test Document** mit Test-Case-IDs, Verifikationsmethode und Status.
- Rückverfolgbarkeit ist **handgepflegt** (Drift-Risiko), ohne maschinelles Tor.

Der ATM-Sektor arbeitet mit einer gestuften, normierten Dokumentenkette. Diese
Entscheidung führt sie ein — **ohne** das Bestehende wegzuwerfen (das Register ist
ein starker SRS-Startpunkt).

### Norm-Rahmen (bewusst getrennt)

| Norm | Rolle hier |
|------|------------|
| **ED-109A / DO-278A** + **ED-153** | **Assurance-/Prozess-Gerüst.** Definiert *wie* man Korrektheit nachweist: Lebenszyklusdaten, Rückverfolgbarkeit, Verifikations-Objectives. Liefert **die Form** dieser Dokumentenkette. |
| **ED-129C** (*Technical Specification for a 1090 MHz Extended Squitter ADS-B Surveillance System*) | **Anforderungs-*Quelle*.** Eine technische/Interoperabilitäts-Spezifikation — sie liefert den **Inhalt** von System-Anforderungen (Funktion/Performance/Interface) für den **ADS-B-Surveillance-Pfad**. Sie definiert **kein** Dokumenten-/Prozess-Gerüst. |

**Konformitätsnachweis-Logik:** ED-129C-Konformität wird *nachgewiesen*, indem
ihre einschlägigen Klauseln als CR/SYS-Anforderungen in die SSS aufgenommen und
bis zum Test runter-traced werden; das Trace-Gerüst dafür ist ED-109A/ED-153.

> **Ehrliche Scope-Grenze.** Firefly ist ein **Multi-Sensor-Fusions-Tracker**
> (PSR/SSR/ADS-B → CAT062), kein reines 1090ES-ADS-B-Bodensystem. ED-129C trifft
> daher am direktesten den **ADS-B-Eingangs-/Surveillance-Pfad** (`adsb_opensky`
> und künftige ADS-B-Adapter), nicht zwingend den ganzen Fusions-Kern. Der Kern
> wird vom Assurance-Gerüst getragen. Wie ADR 0004 versprechen wir
> *Zertifizierungs-Fähigkeit*, nicht die formale Zertifizierung selbst.

## Entscheidung

### 1. Gestufte Dokumentenkette

```
 Bedarf        CR-####         Change Request: WAS soll es tun + WARUM (Klasse, Begründung, Status)
   │
   ▼
 System        SSS             System/Subsystem Specification — SYS-<DOM>-### (verifizierbar)
   │ (1 je Repo)               Quellen: CR + ED-129C-Klauseln + Charta-Querschnitts-NFRs
   ▼
 Software      SRS (1..n)      je CSCI ein SRS — SWE-<CSCI>-### (heutige FR-/NFR-IDs leben hier weiter)
   │
   ▼
 Verifikation  STD             Software Test Document — TC-<CSCI>-### (Methode T/A/I/D, → SWE/SYS, → echter Test)
   │
   ▼
 Ausführung    cargo test      die automatisierten Tests = ausführbarer Nachweis
```

Bereits vorhanden und eingebunden: **ADRs** (Design-Rationale) und **ICDs**
(`ICD-CAT062.md` Ausgabe, `source-input-contract.md` Eingang) als
**Interface-Requirements**.

### 2. Schnitt: je Repo gespiegelt

Firefly und Wayfinder bekommen **je einen eigenen** CR/SSS/SRS/STD-Baum — **keine**
repo-übergreifende Mega-SSS. Das ist treu zur Charta (getrennte Repos, Kopplung
nur über den Draht-Vertrag). Die **CAT062-ICD** ist auf beiden Seiten das
verbindende **Interface-Requirement** (`SYS-IF-…` referenziert die ICD-Version).
Eine kurze, in beiden Repos gespiegelte Konventions-Notiz hält die Schemata
synchron (wie ROADMAP §3). Wayfinder zieht mit einem **eigenen, gespiegelten ADR**
nach.

### 3. ID-Schema (abwärtskompatibel — keine Umnummerierung)

| Ebene | ID-Form | Bemerkung |
|-------|---------|-----------|
| Change Request | `CR-####` | chronologisch je Repo |
| System (SSS) | `SYS-<DOM>-###` | `DOM` ∈ Klassen-Taxonomie (§4) |
| Software (SRS) | `SWE-<CSCI>-###` | **bestehende `FR-GEO-001` etc. bleiben als SRS-IDs erhalten** — kein Umbenennen, `// REQ:`-Tags im Code bleiben gültig |
| Test (STD) | `TC-<CSCI>-###` | mit Verifikationsmethode + Verweis auf echten Test |

IDs werden **nie wiederverwendet**; ungültige wandern auf Status `superseded`/`retired`.

### 4. Anforderungs-Klassen (SSS-Ebene)

| Domäne | Code | Bedeutung |
|--------|------|-----------|
| Funktional | `FUN` | Was das System tut |
| Performance | `PRF` | Genauigkeit, Latenz, Kapazität, Update-Rate (← ED-129C-Kern) |
| Interface | `IF` | Externe Verträge (CAT062-ICD, Quell-Kontrakt) |
| Safety | `SAF` | Aus Hazard-Analyse/FHA abgeleitet (Roadmap-Paket, künftiger Lieferant) |
| Security | `SEC` | Vertrauensgrenzen, Auth, Netz-Isolation (ADR 0017) |
| Operational | `OPS` | Health/Readiness, Shutdown, Replay, Determinismus |
| Resource | `RES` | Speicher-/CPU-/Netz-Budgets |
| Constraint | `CON` | Gesetzte Randbedingungen (Norm, Stack, Sprache) |

Auf SRS-Ebene werden die bestehenden `FR-…`/`NFR-…`/`CON-…`-Klassen je CSCI
weitergeführt.

### 5. CSCI = Crate (Granularität justierbar)

Ein **CSCI** (Computer Software Configuration Item) ist die als Einheit
entwickelte/getestete/versionierte Software-Komponente — sie bekommt **je ein
SRS**. In Firefly fällt das natürlich auf die **Workspace-Crates**:

| Crate | CSCI | Crate | CSCI |
|-------|------|-------|------|
| `firefly-core` | `CORE` | `firefly-io` / `firefly-player` | `IO` |
| `firefly-geo` | `GEO` | `firefly-opensky` | `OSK` |
| `firefly-track` | `TRK` | `firefly-multicast` | `MC` |
| `firefly-asterix` | `AST` | `firefly-recorder` | `REC` |
| `firefly-sim` | `SIM` | `firefly-server` | `SRV` |

Zusammenlegungen (z. B. `CORE`+`IO`, `MC`+`REC`) sind beim Rollout möglich, wenn
ein eigenes SRS zu dünn wäre.

### 6. Verzeichnis-Layout (je Repo)

```
docs/requirements/
  README.md            Index + Konventionen + Status-Lebenszyklus
  change-requests/     CR-0001.md … (oder ein CR-Register.md)
  SSS.md               System/Subsystem Specification (SYS-…)
  srs/                 SRS-GEO.md, SRS-TRK.md, …   (SWE-…)
  STD.md               Test-Case-Katalog (TC-…, Methode, Status)
  trace-matrix.md      ⚙️ GENERIERT (nicht handgepflegt)
```

### 7. Verifikationsmethoden & Status-Lebenszyklus

- Test-Case-Methode: **T**est / **A**nalyse / **I**nspektion / **D**emonstration.
  Macht heutige „manuell verifiziert"/„Code-Review"/„quantitativer Test offen"
  ehrlich und trackbar.
- Requirement-Status: `proposed → approved → implemented → verified →
  (superseded | retired)`.

### 8. Rückverfolgbarkeits-Werkzeug (Trace-Tool + CI-Tor)

Ein Repo-eigenes Werkzeug (Firefly: `cargo xtask trace`) das:

1. Anforderungs-IDs aus den Docs und `// REQ:`/`// VERIFIES:`-Tags + Testnamen aus
   dem Code einliest,
2. `docs/requirements/trace-matrix.md` **generiert** (CR→SYS→SWE→TC→Test, in beide
   Richtungen),
3. als **CI-Tor** fehlschlägt bei: Anforderung ohne Test, Test-Tag ohne
   Anforderung, SWE ohne SSS-Eltern, Status-Widersprüchen.

Das ersetzt die handgepflegte Drift und liefert den maschinellen Nachweis, den ein
ED-109A-Audit erwartet.

### 9. Migration (kein Code-Rewrite)

1. **SRS:** flache Tabelle → je CSCI eine Datei; **IDs unverändert**.
2. **SSS:** dünn neu darüberlegen (System-Funktion + Charta-NFRs als `SYS-…`),
   bestehende SWE-Reqs nach oben verlinken.
3. **STD:** Test-Spalte → Test-Case-Katalog mit Methode + Status.
4. **CR:** rückwirkend leichtgewichtig; **ab jetzt verpflichtend** für Neues.
5. **Trace-Tool + CI-Tor.**

### 10. Rollout in Häppchen

- **H0** *(dieser ADR)* — Architektur, ID-Schema, Klassen, CSCI, Trace-Konzept
  festschreiben. Reines Papier.
- **H1 — Pilot-Durchstich an *einem* CSCI** (`GEO`): SSS-Auszug + `SRS-GEO` +
  STD-Auszug + Trace-Tool-MVP + CI-Tor. Beweist das Schema an echtem Material.
- **H2..Hn** — restliche CSCIs je Repo nachziehen; Wayfinder gespiegelt.

## Konsequenzen

- **Positiv:** Audit-fähige, gestufte Rückverfolgbarkeit (CR→SSS→SRS→STD), je
  CSCI handhabbar; ED-129C-Klauseln sauber als SYS-Quellen verankerbar;
  maschinelles Trace-Tor verhindert Drift. Bestehende IDs/Tags bleiben gültig.
- **Negativ / Aufwand:** Mehr Dokument-Disziplin pro Häppchen (CR + SSS-/SRS-/STD-
  Pflege); ein Trace-Tool muss gebaut und in CI verdrahtet werden. Wird durch den
  Pilot-Durchstich (H1) kalibriert, bevor breit ausgerollt wird.
- **Folgearbeit:** H1 (GEO-Pilot) nach Review dieses ADR; Wayfinder-Spiegel-ADR;
  SAF-Klasse füllt sich, sobald die FHA/Hazard-Analyse (Roadmap) vorliegt.

## Alternativen erwogen

- **Flache Tabelle beibehalten:** verworfen — keine System-Ebene, kein CR-Vordersatz,
  kein Test-Dokument, handgepflegte Traceability skaliert nicht audit-fest.
- **Übergreifende System-SSS über beide Repos:** verworfen — bräche die strikte
  Entkopplung (Charta) und schüfe ein geteiltes Artefakt über zwei Repos/Sessions.
  Stattdessen je Repo gespiegelt + ICD als Interface-Requirement.
- **Schwere SDD-Pflicht je CSCI sofort:** zurückgestellt — Design-Rationale tragen
  vorerst die ADRs; eine dünne SDD-Schicht kann je CSCI später folgen, wo nötig.

## Querverweise

- ADR 0004 (Assurance & Zertifizierungs-Fähigkeit) — erweitert.
- ADR 0017 (Multicast-Vertrauensgrenze) — Quelle für `SYS-SEC-…`.
- ICDs: `docs/ICD-CAT062.md`, `docs/source-input-contract.md` — Interface-Requirements.
- ED-129C (EUROCAE) — System-Anforderungs-Quelle (ADS-B-Surveillance-Pfad).
- Wayfinder: gespiegelter ADR (Assurance-Dokumenten-Architektur) folgt dort.
