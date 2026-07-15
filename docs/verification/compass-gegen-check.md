# Unabhängiger Gegen-Check mit OpenATS COMPASS (HA.5)

> **Anforderung:** NFR-SAFE-003 · **Zweck:** Ein unabhängiges
> Dritt-Werkzeug mit **eigenem** ASTERIX-Decoder liest Fireflys echten
> CAT062/065/063-Mitschnitt und muss zu denselben Aussagen kommen wie
> unsere eigene Messung (HA.4, `firefly-eval`) und unsere Metriken.
> Das entkräftet den „selbst benotete Hausaufgabe"-Einwand und ist
> zugleich der stärkste Konformitätsbeleg für das Draht-Format jenseits
> unserer eigenen Referenz-Vektoren.

**Rollen:** Dieses Dokument liefert den geprüften, reproduzierbaren
**Weg** (Mitschnitt → Import → Checkliste → Bericht). Der eigentliche
COMPASS-Lauf ist ein **Betreiber-Schritt** (GUI-Werkzeug, echte
Quellen-Konfiguration); der ausgefüllte Bericht (Abschnitt 5) wird als
Verifikations-Artefakt unter `docs/verification/` eingecheckt.

---

## 1. Voraussetzungen

- Eine laufende Firefly-Instanz mit aktiviertem Feed
  (`FIREFLY_CAT062_ENABLED=true`; Gruppe/Port Default `239.255.0.62:8600`)
  und mindestens einer echten Quelle (sonst ist der Himmel leer — für
  den Format-Check genügt auch das: CAT065/CAT063 laufen immer).
- Ein Mitschnitt-Host, auf dem die Multicast-Gruppe tatsächlich ankommt.
  **Praktisch am einfachsten:** der Host, auf dem ohnehin ein Konsument
  läuft (Wayfinder oder eine Firefly-Standby-Instanz) — der Konsument
  hält per IGMP die Gruppen-Mitgliedschaft, sodass geswitchte Netze den
  Verkehr überhaupt zustellen. `tcpdump` allein tritt der Gruppe nicht
  bei.
- [OpenATS COMPASS](https://github.com/hpuhr/OpenATS-COMPASS) auf einer
  Arbeitsplatz-Maschine (Linux; Release-AppImage genügt).
- Optional, empfohlen: Wireshark — es bringt einen eigenen
  ASTERIX-Dissector mit und ist damit ein **zweiter** unabhängiger
  Decoder für die Schnell-Sichtung.

## 2. Mitschnitt (PCAP, Standard-Bordmittel)

```bash
# Auf dem Mitschnitt-Host; <if> = Interface, auf dem die Gruppe ankommt.
# Empfohlene Dauer: >= 10 Minuten mit echtem Verkehr.
sudo tcpdump -i <if> -w firefly-$(date +%Y%m%d-%H%M).pcap \
  'udp and dst host 239.255.0.62 and dst port 8600'
```

Während des Mitschnitts festhalten (für den Konsistenz-Abgleich in
Abschnitt 4):

```bash
# Metriken-Schnappschuss zu Beginn und am Ende des Mitschnitts:
curl -s http://<firefly>:8080/metrics | grep -E \
  'firefly_(tracks_active|cat062_scans_sent_total|cat065_heartbeats_sent_total|cat063_status_sent_total|tracks_correlated)'
```

**Schnell-Sichtung mit Wireshark (zweiter Fremd-Decoder):** PCAP öffnen,
Filter `udp.dstport == 8600`, Protokollspalte muss **ASTERIX** zeigen;
stichprobenartig Records aufklappen — Kategorie 062/065/063, keine
„Malformed Packet"-Markierungen.

## 3. Import & Auswertung in COMPASS

1. Neue Datenbank anlegen (*File → New DB*).
2. PCAP importieren (*Import → ASTERIX Recording*, Framing **PCAP**);
   Kategorien 062/065/063 aktivieren, Edition für CAT062 passend zur
   EUROCONTROL-Spezifikation (Ed. 1.10+) wählen.
3. Import-Log prüfen → Checkliste C1/C2.
4. Listbox-/Statistik-Ansichten öffnen (Records je Kategorie, Items je
   Record, Update-Intervalle, Tracks) → Checkliste C3–C6.

## 4. Prüf-Checkliste (Soll-Ergebnisse)

| # | Prüfung | Soll | Beleg |
|---|---------|------|-------|
| C1 | Dekodier-Fehler beim Import | **0** über den gesamten Mitschnitt (Format-Konformität; der Fremd-Decoder liest jeden Record) | COMPASS-Import-Log |
| C2 | Vorhandene Kategorien | genau 062, 065, 063 — keine unbekannten | Import-Statistik |
| C3 | CAT062-Item-Abdeckung | Pflicht-Items in **jedem** Record: I062/010, 070, 105, 100, 185, 040, 080, 290, 500. Optionale Items nur situativ: 060/245/380 (SSR-Identität), 136/130/135/220 (Vertikal), 200/210 (Kinematik), **390 nur bei korreliertem Track** (ICD 3.7.0) | Item-Statistik vs. `docs/ICD-CAT062.md` §4 |
| C4 | Update-Raten | CAT062-Blöcke im Ausgabe-Takt der Instanz; CAT065 ≈ 1 Hz; CAT063 ≈ alle 5 s (bzw. konfigurierte Perioden) | COMPASS-Intervall-Statistik vs. Konfiguration |
| C5 | Track-Konsistenz | Track-Anzahl im Mitschnitt ≈ `firefly_tracks_active` im Fenster; Track-Nummern kontinuierlich (kein unerklärter Sprung); Track-Ende nur mit TSE-Record | COMPASS-Track-Ansicht vs. Metriken-Schnappschüsse |
| C6 | Korrelation (falls `FIREFLY_FLIGHT_PLANS` gesetzt) | I062/390-CSN/DEP/DST erscheinen an den korrelierten Tracks; Anzahl ≈ `firefly_tracks_correlated` | Item-Ansicht vs. Metrik |

**Bewertung:** Jede Abweichung wird im Bericht dokumentiert und
klassifiziert — *Format-Fehler* (C1/C2/C3: unser Encoder oder die ICD
sind falsch → Issue + Fix vor allem anderen), *Konsistenz-Abweichung*
(C4–C6: erklärbar? z. B. Mitschnitt-Fenster ≠ Metrik-Fenster) oder
*Werkzeug-Differenz* (COMPASS-Edition/Interpretation — dokumentieren).

## 5. Abgleich-Bericht (Template)

Beim Betreiber-Lauf ausfüllen und als
`docs/verification/compass-bericht-<datum>.md` einchecken:

```markdown
# COMPASS-Gegen-Check — Bericht <YYYY-MM-DD>

- Firefly-Version/Commit: …
- ICD-Version: … · COMPASS-Version: … · Wireshark-Version: …
- Mitschnitt: <Datei>, Dauer …, Quellen-Konfiguration: …
- Metriken-Schnappschüsse: Beginn … / Ende …

| # | Ergebnis (ok/abweichend) | Ist-Wert / Befund |
|---|--------------------------|--------------------|
| C1 | | |
| C2 | | |
| C3 | | |
| C4 | | |
| C5 | | |
| C6 | | |

Abweichungen & Klassifikation: …
Fazit: …
```

## 6. Ehrliche Grenzen

- **Keine wahrheitsbasierte Genauigkeit:** Aus einem Track-only-
  Mitschnitt kann auch COMPASS keine Positions-RMSE gegen die echten
  Flugzeugpositionen rechnen — es kennt die Wahrheit ebenso wenig wie
  wir. Der Gegen-Check bestätigt **Konformität und Konsistenz**; die
  absolute Genauigkeit misst der HA.4-Messstand gegen Simulator-Wahrheit.
- Der COMPASS-Lauf selbst ist nicht CI-fähig (GUI, echte Umgebung) —
  er ist ein **Abnahme-Schritt**, kein Regressions-Gate. Wiederholung
  empfohlen nach jedem ICD-Bump.
- COMPASS-Interpretationen können von der Spezifikations-Edition
  abhängen; Werkzeug-Differenzen sind zu dokumentieren, nicht
  stillschweigend zu „beheben".
