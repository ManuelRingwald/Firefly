# Firefly — Projekt-Charter & Arbeitsregeln

> Dieses Dokument ist die **verbindliche Arbeitsvereinbarung** zwischen dem
> Projektverantwortlichen (Mensch) und dem KI-Assistenten (Claude). Claude liest
> diese Datei zu Beginn jeder Sitzung und hält sich an die hier festgelegten
> Regeln. Es ist zugleich ein menschenlesbares Manifest: *So* arbeiten wir.

---

## 1. Worum es geht

Firefly ist ein web-basierter **Radar-Tracker** — das Rechen-Herzstück einer
Luftlagedarstellung. Aus den verrauschten Einzelmeldungen von Primär- (PSR) und
Sekundärradar (SSR) werden saubere, durchgehende **Tracks** berechnet (Position,
Geschwindigkeit, Identität jedes Luftfahrzeugs).

Das fachliche Ziel ist anspruchsvoll. Das **didaktische** Ziel ist genauso
wichtig:

> **Der Weg ist das Ziel.** Der Projektverantwortliche ist IT-Projektleiter bei
> einem ANSP (Flugsicherungsorganisation), **ohne formale IT-Ausbildung**, und
> will dieses Projekt nutzen, um Technik *und* Fachlichkeit wirklich zu
> verstehen. Tempo ist zweitrangig. Verständnis ist erstrangig.

---

## 2. Die goldene Regel: **Erst erklären, dann bauen**

Claude darf **keinen** nennenswerten Code schreiben, ohne vorher den nächsten
Schritt verständlich erklärt und eine Freigabe eingeholt zu haben. Pro
Arbeitsschritt gilt dieser Ablauf:

1. **Ankündigen** — Was kommt als Nächstes? In einfachen Worten, getrennt nach:
   - **Fachlich** (*Warum* braucht ein Radar-Tracker das? Was ist das Problem
     aus Sicht der Luftlage?)
   - **Technisch** (*Wie* setzen wir es um? Welche Bausteine, welche Mathematik,
     welche Dateien?)
2. **Begriffe klären** — Jeder neue Fachbegriff wird beim ersten Auftreten
   erklärt und in `docs/glossary.md` aufgenommen. Keine unerklärten Abkürzungen.
3. **Freigabe abwarten** — Claude hält an und wartet auf Rückfragen oder ein
   „Go". Erst dann wird implementiert.
4. **In kleinen, testbaren Häppchen umsetzen** — Lieber drei kleine, je für sich
   verständliche Schritte als ein großer Sprung.
5. **Nachbereiten** — Doku aktualisieren (Meilenstein-Erklärung, Glossar, ggf.
   Entscheidungs-Log), Tests grün, dann committen.

**Verboten:** „Durchrattern" — also mehrere Bausteine ungefragt
hintereinanderweg bauen, ohne Erklärung und Freigabe dazwischen.

---

## 3. Sprache

- **Erklärungen, Chat und Dokumentation (`docs/`, `CLAUDE.md`):** Deutsch.
- **Quellcode (Bezeichner, Kommentare im Code):** Englisch — internationaler
  Industriestandard, hält den Code portabel und anschlussfähig. Die *Erklärung*
  des Codes erfolgt dann auf Deutsch in den `docs/` bzw. im Chat.
- Diese Aufteilung ist eine bewusste Entscheidung (siehe ADR 0002) und kann
  jederzeit geändert werden, wenn der Projektverantwortliche es wünscht.

---

## 4. Dokumentationspflichten

Dokumentation ist in diesem Projekt **kein Nachgedanke, sondern Teil der
Leistung**. Es gibt drei Ebenen:

| Ebene | Ort | Zweck |
|-------|-----|-------|
| **Code-Doku** | Doc-Kommentare (`//!`, `///`) im Code | Erklären das *Warum* eines Moduls/Typs, nicht nur das *Was*. |
| **Lern-/Fach-Doku** | `docs/milestones/` | Pro Meilenstein eine verständliche Erklärung in Deutsch: Fachlichkeit + Technik + Mathematik in Worten. |
| **Glossar** | `docs/glossary.md` | Wächst mit. Jeder Fachbegriff einmal in einfacher Sprache, gern mit Analogie. |
| **Entscheidungen** | `docs/decisions/` | Architecture Decision Records (ADR): *welche* wichtige Entscheidung *warum* getroffen wurde. |

Regeln:
- Jeder neue Meilenstein bekommt **vor dem Abschluss** seine Erklärung in
  `docs/milestones/`.
- Jede architektonisch relevante Weichenstellung bekommt einen kurzen ADR.
- Das Glossar wird bei jedem neuen Begriff gepflegt — nicht „später".

---

## 5. Qualitäts-Gates (vor jedem Commit)

Ein Schritt gilt erst als fertig, wenn:

- [ ] `cargo test --workspace` ist grün.
- [ ] `cargo clippy --workspace --all-targets` ist ohne Warnungen.
- [ ] `cargo fmt` wurde ausgeführt.
- [ ] Die zugehörige Doku wurde aktualisiert.
- [ ] Der Commit hat eine klare, beschreibende Nachricht.

---

## 6. Git & Branches

- Entwicklung **immer** auf dem vereinbarten Feature-Branch
  (`claude/radar-track-calculator-BoaU8`).
- Niemals ungefragt auf einen anderen Branch pushen.
- **Kein** Pull Request, außer der Projektverantwortliche bittet ausdrücklich
  darum.
- Commits klein und thematisch geschnitten; eine Sache pro Commit.

---

## 7. Inkrementelles Vorgehen — die Meilensteine

| Meilenstein | Inhalt | Status |
|-------------|--------|--------|
| **M1** | Szenario- + Radar-Plot-Simulator (Datenquelle) | ✅ fertig |
| **M2** | Single-Radar-Tracker: Gating + GNN + Kalman, Track-Lifecycle | ⏳ als Nächstes |
| **M3** | Web-Frontend mit Live-2D-Karte über WebSocket | ⏳ |
| **M4** | SSR/ADS-B-Identitätskorrelation + Multi-Radar-Fusion | ⏳ |
| **M5** | IMM / JPDA für Manöver & dichten Verkehr | ⏳ |

Innerhalb eines Meilensteins arbeiten wir in kleinen Schritten nach der goldenen
Regel (Abschnitt 2).

---

## 8. Was Claude NICHT tut

- Keine unerklärten Fachbegriffe oder Abkürzungen verwenden.
- Nicht mehrere Bausteine ungefragt hintereinander bauen.
- Keine großen, überraschenden Architektur-Sprünge ohne ADR und Freigabe.
- Nicht „fertig" melden, solange die Qualitäts-Gates (Abschnitt 5) nicht erfüllt
  sind.
- Tempo nicht über Verständnis stellen.
