# Track-Nummern-Pool für I062/040 (ARTAS-Roadmap QW.1)

> **Anforderung:** FR-TRK-035 · **ICD:** 3.1.1 (dokumentarisch) ·
> **Einstufung:** S3 (Schnittstellen-Wirkung → umgesetzt auf Fable 5)

## Fachlich: Warum braucht das ASD das?

Die Track-Nummer (CAT062 I062/040) ist die **Draht-Identität** eines Tracks —
der Konsument (Wayfinder, künftig Safety-Nets/Recorder) schlüsselt sein
gesamtes Lagebild danach: Label, History, Selektion und vor allem das
**TSE-Löschsignal** (ADR 0016).

Bis zu diesem Häppchen vergab Firefly intern monoton steigende `u32`-IDs und
schnitt beim Encoding schlicht die unteren 16 Bit ab. Im Dauerbetrieb einer
verkehrsreichen Region ist das eine tickende Uhr: Nach 65 536 Track-Geburten
(jeder ein-/ausfliegende Flieger, jeder Neuaufbau nach Coasting) beginnt der
Zähler auf dem Draht implizit von vorn. Kollidiert eine neue Nummer mit einem
**noch lebenden** Alt-Track, zeigt das ASD zwei Luftfahrzeuge unter einer
Identität: Label springen, die History vermischt sich, und ein TSE für den
einen Track löscht beim Lotsen den anderen — ein stiller, sicherheitsrelevanter
Anzeigefehler.

Echte SDPS (ARTAS) **bewirtschaften** den Nummernraum, statt ihn aus einer
internen ID abzuleiten: Nummern gelöschter Tracks werden erst nach einer
Karenzzeit wiederverwendet. Genau das macht Firefly jetzt auch.

## Technisch

### Baustein: `firefly-track::track_number::TrackNumberPool`

- **Frische Nummern zuerst**, aufsteigend ab `1`; `0` wird nie vergeben
  (Sentinel für Konsumenten bleibt frei). Solange frische Nummern existieren,
  ist das Verhalten mit dem alten (Nummer = ID) für die ersten 65 535 Tracks
  identisch — die Referenz-Dumps blieben unverändert grün.
- **Quarantäne:** Bei Track-Löschung wird die Nummer zum **Löschzeitpunkt in
  Datenzeit** freigegeben und ist für `TRACK_NUMBER_QUARANTINE_SECS` = 60 s
  gesperrt. Datenzeit (nicht Wanduhr) hält den Pool deterministisch und
  replay-fähig (ADR 0003). Da die Datenzeit per Watermark monoton ist, bleibt
  die Quarantäne-Queue per Konstruktion sortiert — Freigabe-Prüfung ist O(1)
  am Queue-Kopf.
- **FIFO-Wiederverwendung:** Nach der Karenz kommt die am längsten freie
  Nummer zuerst zurück (größtmöglicher Abstand zwischen TSE des alten und
  Geburt des neuen Tracks).
- **Ehrliche Erschöpfung:** Sind alle Nummern belegt oder quarantänisiert
  (> 65 535 gleichzeitige Tracks — weit jenseits realer Kapazität), lehnt der
  Tracker die Track-Initiierung mit `tracing::warn!` ab, statt eine
  Duplikat-Nummer zu senden. Dokumentiert in TECHNICAL.md §11.

### Trennung intern / Draht

`TrackId` (u32, prozess-eindeutig, nie wiederverwendet) bleibt die **interne**
Identität für Assoziation und Buchhaltung. Neu trägt `Track.number` (und
additiv `SystemTrack.track_number`) die **Draht**-Nummer aus dem Pool. Der
CAT062-Encoder kodiert I062/040 ausschließlich aus `track_number` — die
frühere Trunkierung (`track.id.0 as u16`) ist entfernt.

Der Pool ist Feld des `Tracker` und damit Teil des serialisierbaren
Zustands (ADR 0007) — wichtig für die spätere HA-Arbeit (SDPS-002): ein
wiederhergestellter Tracker vergibt keine Nummern doppelt.

### Schnittstellen-Wirkung (Wayfinder)

**Kein Wire-Format-Bruch** — I062/040 bleibt u16 BE. ICD 3.1.1 schreibt die
Vergabe-Semantik dokumentarisch fest (Abschnitt 4.6, Konsumenten-Garantie:
nie zwei lebende Tracks unter einer Nummer; ≥ 60 s Datenzeit zwischen TSE und
Wiedergeburt einer Nummer). Wayfinder muss nichts nachziehen.

## Tests

- `track_number::*` (5 Tests): Vergabe ab 1, Quarantäne-Sperre, FIFO-Reihenfolge,
  Erschöpfung ohne Duplikat, Fresh-Space-Ende bei `u16::MAX` ohne Wrap.
- `tracker::track_number_is_quarantined_after_deletion_then_reused`:
  Lebenszyklus Ende-zu-Ende gegen einen auf 1 Nummer verkleinerten Pool —
  Geburt, TSE trägt die Nummer, Initiierung während der Karenz abgelehnt,
  Wiederverwendung danach.
- `cat062::track_number_is_pool_number_not_truncated_id`: Regression — der
  Encoder nutzt die Pool-Nummer, nie `id mod 2¹⁶`.

Gates: `cargo test --workspace` (alle Suiten grün), `cargo clippy` ohne
Befunde, `cargo fmt`.
