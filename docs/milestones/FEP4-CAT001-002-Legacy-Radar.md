# FEP.4 — CAT001/002-Legacy-Radar-Eingang

> **Anforderungen:** FR-IO-011 (Decoder), FR-NET-016 (Dispatch + Zeit-Anker) ·
> **Quell-Kontrakt:** unverändert · **Ausgabe-ICD:** unverändert ·
> **Einstufung:** S3 · umgesetzt auf Fable 5

## Fachlich: Warum?

CAT048/CAT034 sind die *moderne* Monoradar-Formatgeneration — aber ein
erheblicher Teil der real verbauten Radarköpfe (ältere PSR/SSR-Anlagen, auch
im Phoenix-Umfeld) sendet noch das **Legacy-Paar CAT001/CAT002**: CAT001
trägt die Zielmeldungen, CAT002 die Servicemeldungen (Nordmarke, Sektoren —
das Gegenstück zu CAT034). ARTAS konsumiert beide Generationen; ein SDPS,
das nur CAT048 versteht, kann viele Bestandsradare schlicht nicht
anschließen. FEP.4 macht Firefly an genau diesen Anlagen anschlussfähig —
ohne neue Quelle, ohne neuen Kontrakt: Ein Legacy-Radar wird unverändert als
`radar_asterix` konfiguriert.

## Technik

**Die zweigeteilte UAP (die Legacy-Falle).** Anders als jede spätere
Kategorie hat CAT001 **zwei** User Application Profiles: eines für Plots,
eines für Tracks. Dasselbe FSPEC-Bit bedeutet je nach Record-Art ein
**anderes Item** — FRN 3 ist im Plot-Profil die Position (I001/040), im
Track-Profil die Track-Nummer (I001/161). Selektor ist das **TYP-Bit** in
I001/020 (FRN 2, in beiden Profilen gleich): `0` = Plot, `1` = Track. Der
Decoder wählt die UAP **je Record**; ein Record, der Items ab FRN 3 markiert,
aber kein I001/020 trägt, wird **abgelehnt statt geraten** — jede andere
Wahl wäre ein stiller Fehl-Parse aller Folge-Bytes. Beide UAP-Tabellen
wurden gegen die aus der EUROCONTROL-Spezifikation generierte Referenz
(asterix-specs/libasterix, cat001 ed 1.4) verifiziert.

**Trunkierte Zeit (I001/141) + Anker (I002/030).** CAT001-Records tragen
keine volle Tageszeit — I001/141 ist ein 16-Bit-Zähler in 1/128 s, der alle
**512 s wickelt**. Die volle Zeit liefert klassisch der CAT002-Service-Strom.
Der Listener führt deshalb einen **Zeit-Anker** (letzter voller ToD aus
CAT002/CAT034) und expandiert je Meldung: gewählt wird der zum Anker
**nächstgelegene** kongruente Wert (`expand_truncated_tod`, rein und
getestet; tolerant bis ±256 s Versatz, Mitternachts-Wrap in den
Vortags-Rest statt negativer Zeit). **Ehrliche Grenze:** Ohne Anker (vor der
ersten Servicemeldung) wird ein Legacy-Plot **verworfen** statt mit
erfundener Zeit versehen — ein verlorener Plot in der ersten
Antennen-Umdrehung schlägt eine falsche Datenzeit im deterministischen
Tracker (ADR 0003).

**Format-Agnostik der Konsumenten.** CAT001-Reports konvertieren per
`into_target_report` in denselben neutralen `DecodedTargetReport` wie
CAT048 (ohne Mode-S-Felder — die Generation kennt sie nicht) und laufen
durch **dasselbe** `target_report_to_plot`-Mapping; CAT002 liefert dasselbe
`DecodedServiceMessage` wie CAT034 — Nordmarken speisen unverändert den
`ScanPeriodEstimator` (FEP.1) und die CAT063-Liveness. Tracker, Fusion und
Sensor-Überwachung sehen keinen Generations-Unterschied. Die
Typ-Code-Tabellen unterscheiden sich (CAT002-Typ 3 = **Süd**-Marker, nicht
CAT034s Geo-Filter) — explizit gemappt, Unbekanntes bleibt `Other`.

**Robustheit.** Wie alle Eingangs-Decoder (Charta §8): grenzen-geprüfte
Cursor, kein Panic auf Eingabe, Spare-FRNs und der nie genutzte
RFS-Indikator (Random Field Sequencing — ohne Interpretation nicht
überspringbar) sind harte Fehler. Simulierte Meldungen (SIM-Bit) werden im
Adapter verworfen (FR-TRK-036). Fuzz-Targets `cat001_decode` (5,5 M Läufe)
und `cat002_decode` (7,0 M Läufe) ohne Befund; beide laufen im CI-Fuzz-Job
automatisch mit.

## Schnittstellen-Wirkung

- **Ausgabe-ICD (CAT062/063/065): unverändert** — reiner Eingangs-Pfad.
- **Quell-Eingangs-Kontrakt: unverändert** — kein neuer Typ, keine neuen
  Felder, keine neuen Env-Variablen. Kein Wayfinder-Nachzug nötig.

## Ehrliche Grenzen (FEP.4)

- **Plots vor der ersten CAT002-Zeit gehen verloren** (kein Anker). Real
  unkritisch: Servicemeldungen kommen viele Male pro Umdrehung.
- **I001/042/200** (kartesische Position / Track-Geschwindigkeit) werden
  längen-korrekt übersprungen, nicht genutzt — der Tracker rechnet aus der
  polaren Messung; Gleiches gilt für Doppler (I001/120).
- **RFS** wird abgelehnt, nicht unterstützt — in realen Feeds ungenutzt;
  sollte je ein Kopf es senden, scheitert er laut im Log.
