# M3.X — Der CAT062-Encoder: vom Tracker zum ASD-Format

> Verständliche Erklärung von Häppchen 3.X. Begriffe stehen ausführlicher im
> [Glossar](../glossary.md).

M3 hat den Tracker live in den Browser gebracht — als JSON über WebSocket,
gut lesbar und einfach zu debuggen. Das **ASD** (die Lagedarstellung der
Lotsen, ADR 0006) spricht aber kein JSON, sondern **ASTERIX CAT062**: ein
**bit-genaues Binärformat**. Häppchen 3.X baut den zweiten Ausgabe-Adapter,
der genau dieses Format erzeugt — *neben* JSON, ohne den Tracker-Kern
anzufassen (Ports & Adapters, ADR 0006).

---

## Häppchen 3.X.1 — Framing + FSPEC/UAP-Mechanik

**Status:** ✅ umgesetzt · Anforderung `FR-IO-003`

### Das Problem (fachlich)

ASTERIX-Nachrichten sind keine selbstbeschreibenden Datensätze wie JSON
(`{"lat": 47.5, ...}`). Stattdessen ist jedes Bit vorab in einer
**Spezifikation** (Kategorie 062, „CAT062") festgelegt: welches Datenfeld an
welcher Stelle steht, wie viele Bytes es belegt und mit welchem
Skalierungsfaktor (**LSB**, *Least Significant Bit* — der Wert eines
Zähl-Schritts) eine Zahl kodiert wird.

### Die Lösung (technisch)

Neue Crate **`firefly-asterix`**, mit zwei Bausteinen:

- **Framing** (`cat062::data_block`): `[CAT=62][LEN: 2 Byte]`, gefolgt von den
  Datensätzen (*Records*) — `LEN` ist die Gesamtlänge inkl. der drei
  Header-Bytes, big-endian.
- **FSPEC/UAP** (`fspec.rs`): Jeder Record beginnt mit dem **FSPEC** — einer
  Bitmaske, die sagt, *welche* Datenfelder im Record vorkommen. Jedes Bit hat
  eine **FRN** (*Field Reference Number*), die laut **UAP** (*User Application
  Profile* — die feste Reihenfolge der Felder für CAT062) auf ein konkretes
  Datenfeld zeigt. Volle Oktette werden per **FX**-Bit (*Field Extension*)
  verkettet: ist das letzte Bit eines Oktetts gesetzt, folgt ein weiteres
  FSPEC-Oktett.

Erste drei Felder: **I062/010** (Datenquelle SAC/SIC — *System Area Code* /
*System Identification Code*, identifiziert den Sensor/das System),
**I062/070** (Time of Track, LSB 1/128 s, 24 Bit, Tagesumbruch) und
**I062/040** (Tracknummer, 16 Bit).

---

## Häppchen 3.X.2 — Position und Geschwindigkeit

**Status:** ✅ umgesetzt · Anforderung `FR-IO-003`

### Das Problem (fachlich)

Das ASD braucht für jeden Track **wo** er ist (Position) und **wohin** er
sich bewegt (Geschwindigkeit) — als feste Bitmuster, nicht als
Gleitkommazahlen.

### Die Lösung (technisch)

- **I062/105** (Position WGS-84): Breite und Länge je als **vorzeichenbehaftete
  32-Bit-Zahl**, LSB = 180/2²⁵ Grad (~5.4·10⁻⁶°, das entspricht knapp 60 cm am
  Äquator).
- **I062/185** (Geschwindigkeit, kartesisch): Ost-/Nord-Komponente je als
  **vorzeichenbehaftete 16-Bit-Zahl**, LSB = 0.25 m/s.

Negative Werte (z. B. Westwärts-Geschwindigkeit) brauchen kein
Sonderfall-Handling: Rusts `i32`/`i16` `to_be_bytes()` liefert direkt die
**Zweierkomplement**-Darstellung, die ASTERIX vorschreibt.

---

## Häppchen 3.X.3 — Status-Felder (Track-Status, Alter, Genauigkeit)

**Status:** ✅ umgesetzt · Anforderung `FR-IO-003`, `FR-TRK-008`

### Das Problem (fachlich)

Position und Geschwindigkeit allein reichen dem Lotsen nicht — er muss auch
sehen, *wie sehr* er einem Track trauen kann (ADR 0008): Ist er schon
**bestätigt** oder noch vorläufig? Wird er gerade **gecoastet**
(extrapoliert, ohne frische Messung)? Wie **alt** ist die letzte Messung, und
wie **genau** ist die geschätzte Position?

### Die Lösung (technisch)

- **I062/080** (Track-Status): variable Länge über das interne **FX**-Bit.
  Im einfachsten Fall (bestätigt, frisch) genügt **ein** Oktett mit dem
  **CNF**-Bit (*Confirmed*, hier invertiert: gesetzt = noch *vorläufig*). Ist
  der Track **CST** (*Coasting*), wird über drei FX-verkettete Oktette bis ins
  vierte Oktett verlängert, wo das CST-Bit sitzt.
- **I062/290** (System Track Update Ages): ein **Compound Item** — ein
  *Primary-Subfield*-Oktett sagt, welche der bis zu zehn Alters-Unterfelder
  folgen (Track-Alter, PSR-Alter, SSR-Alter, …). Wir setzen nur das
  **PSR-Bit** (Bit 15, `0x40`) und liefern das **PSR-Alter** in Schritten von
  ¼ Sekunde.
- **I062/500** (Estimated Accuracies): ebenfalls ein Compound Item. Wir setzen
  nur das **APC-Bit** (*Accuracy of Position, Cartesian*, Bit 16, `0x80`) und
  liefern die geschätzte Standardabweichung (X- und Y-Komponente, je 16 Bit,
  LSB 0.5 m).

### Spezifikations-Verifikation (Nachtrag)

Die LSB-Werte und Subfeld-Bits für I062/290 und I062/500 wurden zunächst aus
dem Gedächtnis kodiert und als **unverifiziert** dokumentiert. Der
Projektverantwortliche hat danach den passenden Auszug aus
*SUR.ET1.ST05.2000-STD-09-01, Edition 1.10* (EUROCONTROL) bereitgestellt.
Ergebnis: **alle Werte waren korrekt** — PSR-Alter `0x40`/¼ s und
APC-Genauigkeit `0x80`/0.5 m (vorzeichenlos, X+Y je 16 Bit) stimmen exakt mit
der Spezifikation überein. Keine Code-Änderung nötig, nur die Kommentare in
`cat062.rs` verweisen jetzt auf die Spec-Paragraphen.

---

## Häppchen 3.X.4 — Adapter-Abschluss & Architektur-Entscheidung

**Status:** ✅ umgesetzt

### Die Frage

Sollte `firefly-asterix` eine Komfortfunktion **`Frame → CAT062`** bekommen,
die direkt das JSON-Zwischenformat aus `firefly-io` (Häppchen 3.1) in CAT062
übersetzt?

### Die Entscheidung (und warum)

**Nein.** `Cat062Encoder::encode(time, tracks: &[SystemTrack])` arbeitet
bereits direkt auf dem **neutralen `SystemTrack`** aus `firefly-core` — genau
dem Typ, den auch `firefly-io::Frame::new` als Eingabe nimmt. Beide Adapter
sitzen **unabhängig nebeneinander** auf demselben `SystemTrack`:

```text
SystemTrack[] ──┬──> Frame::new(...)            → JSON   (WebSocket, M3)
                └──> Cat062Encoder::encode(...)  → CAT062 (ASD, ADR 0006)
```

Eine `Frame → CAT062`-Funktion hätte zwei Probleme aufgeworfen:

1. **Neue Kopplung zwischen den Adaptern.** `firefly-asterix` müsste von
   `firefly-io` abhängen — obwohl beide unabhängige „Übersetzer" desselben
   neutralen Outputs sein sollen.
2. **Verlustbehaftete Rückrechnung.** `FrameTrack` (die JSON-Drahtform) trägt
   Geschwindigkeit als **Betrag + Kurswinkel** (web-freundlich), CAT062
   I062/185 braucht aber die **Ost-/Nord-Komponenten**. Aus Betrag/Winkel
   müsste man die Komponenten per Sinus/Cosinus zurückrechnen — unnötige
   Rechnung mit Rundungsfehlern, obwohl `SystemTrack` die Komponenten direkt
   hat.

Diese Entscheidung bestätigt und konkretisiert ADR 0006 (Ports & Adapters):
**ein** neutraler Kern-Output, **mehrere** unabhängige Adapter.

---

## M3.X — Fazit

`firefly-asterix` kodiert pro Track acht CAT062-Felder
(I062/010, /070, /105, /185, /040, /080, /290, /500) bit-genau, mit
hand-abgeleiteten Referenz-Dumps als Regressionstests und einer
spec-geprüften Übersetzung von Tracker-Status in I062/080, /290, /500. Der
Adapter ist **fertig im Sinne der Architektur** — er ist bewusst nicht
„komfortabler" gemacht worden, um die saubere Trennung der Adapter zu
erhalten.

**Offen (ADR 0006, später klären):** der **Transport** (UDP-Multicast /
Message-Bus / WebSocket) für CAT062 zum ASD, und der **Koordinatenbezug**
(WGS-84 vs. System-Stereografisch). Außerdem ist das Mapping
„`update_age` → PSR-Alter" eine **Single-Sensor-Vereinfachung** — die
Mehr-Sensor-Provenienz (welcher Sensortyp zuletzt aktualisiert hat) kommt erst
mit der Multi-Radar-Fusion in **M4**.

---

## Nachtrag AP7 — I062/245 Target Identification (Callsign), ICD 2.1.0

**Status:** ✅ umgesetzt

### Fachlich

Der Lotse identifiziert ein Flugzeug primär über sein **Rufzeichen
(Callsign / Flight ID)**, nicht über die 24-Bit-ICAO-Adresse oder den
Mode-3/A-Code. CAT062 trägt das Rufzeichen in **I062/245** — ohne dieses Item
müsste das ASD den Track-Label allein aus Track-Nummer und Mode 3/A bilden,
was für den operativen Betrieb unzureichend ist.

### Technisch

I062/245 sitzt auf **FRN 10** — anders als I062/136/I062/500 (ADR 0015) liegt
FRN 10 im **zweiten FSPEC-Oktett**, das wegen FRN 9/11/12/13/14 in jedem
Record bereits vorhanden ist. Die Änderung ist daher **additiv**: kein
Wachstum der FSPEC-Länge, bestehende Decoder bleiben gültig (sie überspringen
das unbekannte FRN-10-Bit, bis sie nachziehen). ICD-Bump auf **2.1.0**
(nicht-breaking), im Gegensatz zu ADR 0015 (2.0.0, breaking).

Die Architektur folgt exakt dem **Pass-through-Muster von I062/136/FR-TRK-027**:

```text
ModeAC.callsign (Option<Callsign>, von firefly-sim)
  → Track::update_identity (sticky: vorhandener Wert überschreibt, fehlender
    Wert löscht nicht — wie mode_3a)
  → SystemTrack.callsign (FR-TRK-028)
  → Cat062Encoder: I062/245, FRN 10 (nur wenn Some)
```

**Wire-Format** (7 Oktette): Oktett 1 = STI (Source of Target Identification,
Bits 8/7) + 6 Spare-Bits, hier `0x00` = *Downlinked Target Identification* —
dieselbe ehrliche „durchgereicht, nicht von uns berechnet"-Framing wie bei der
Flugfläche. Oktette 2–7 = 8 Zeichen × 6-Bit-IA-5-Code (ICAO Annex 10),
MSB-first: `A`–`Z` → 1–26, `0`–`9` → 48–57, Leerzeichen → 32. Der neue Typ
`Callsign([u8; 8])` (in `firefly-core`) kapselt das space-gepolsterte
8-Byte-ASCII-Feld.

Der Decoder (`decode_target_identification`) ist die Umkehrung: er verwirft
das STI/Spare-Oktett, entpackt die sechs 6-Bit-Codes und bildet jeden
Fremd-Code außerhalb {1–26, 32, 48–57} defensiv auf Leerzeichen ab — ein
fremdes/fehlerhaftes Datagramm kann den Decoder nicht zum Absturz bringen
(CLAUDE.md §8, Robustheit).

### Tests

- `cat062::target_identification_packs_eight_six_bit_ia5_codes` — Bit-Layout
  für "DLH123" gegen Hand-Rechnung verifiziert.
- `cat062::decode_recovers_callsign_when_present` — Encoder/Decoder-Rundreise.
- `single_track_matches_reference_dump` bleibt **unverändert grün** (Track
  ohne Callsign → FRN 10 nicht gesetzt, Referenz-Dump unverändert) — der
  empirische Beleg für „additiv".

### Wayfinder (AP8)

Wayfinder zieht den Decoder für I062/245 nach (eigener Schritt, eigenes Repo)
und zeigt das Callsign als primäre Track-Label-Zeile.
