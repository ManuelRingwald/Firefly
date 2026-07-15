# HA.5 — Unabhängiger Gegen-Check mit OpenATS COMPASS

> **Anforderung:** NFR-SAFE-003 · **ADR:** — · **ICD:** unberührt
> (wird gegengeprüft, nicht geändert) · **Einstufung:** S2–S3 ·
> umgesetzt auf Fable 5 (Roadmap-Empfehlung: Sonnet)

## Fachlich

HA.4 misst die Tracker-Güte mit unserem eigenen Messstand — es bleibt
der Einwand „selbst benotete Hausaufgabe". HA.5 liefert die **zweite,
unabhängige Meinung**: OpenATS COMPASS (Open-Source-Analysewerkzeug der
Surveillance-Community) liest unseren echten CAT062/065/063-Mitschnitt
mit seinem **eigenen, fremden ASTERIX-Decoder** und muss zu denselben
Aussagen kommen wie unsere Metriken und der HA.4-Bericht. Das prüft
zwei Dinge auf einmal: unsere Zahlen — und die **Standard-Konformität
des Draht-Formats** über die ganze Kette (Tracker → Encoder →
Multicast → Fremd-Decoder). Unabhängige Verifikation im Sinne von
ED-153/DO-278A.

## Technik

- **`docs/verification/compass-gegen-check.md`** — der geprüfte,
  reproduzierbare Weg:
  1. PCAP-Mitschnitt mit Standard-Bordmitteln (`tcpdump`); wichtiges
     Detail: der Mitschnitt-Host braucht einen IGMP-haltenden
     Konsumenten (Wayfinder/Standby), sonst stellt ein geswitchtes Netz
     die Gruppe gar nicht zu.
  2. Schnell-Sichtung mit Wiresharks eingebautem ASTERIX-Dissector —
     kostenlos ein **zweiter** unabhängiger Decoder.
  3. COMPASS-Import + Auswertung.
  4. **Checkliste C1–C6** mit Soll-Ergebnissen: 0 Dekodier-Fehler;
     exakt die drei Kategorien; Item-Abdeckung gegen ICD 3.7.0
     (Pflicht-Items je Record, situative Items, I062/390 nur bei
     korreliertem Track); Update-Raten (Ausgabe-Takt, 1-Hz-Heartbeat,
     5-s-CAT063); Track-Konsistenz gegen `/metrics`-Schnappschüsse;
     Korrelations-Abgleich.
  5. **Abgleich-Bericht-Template**; ausgefüllte Berichte werden je Lauf
     als `compass-bericht-<datum>.md` eingecheckt
     (Konfigurationsmanagement, ADR 0004). Abweichungen werden
     klassifiziert: Format-Fehler (⇒ Fix vor allem anderen),
     Konsistenz-Abweichung, Werkzeug-Differenz.

## Ehrliche Grenzen

- **Der COMPASS-Lauf selbst ist ein Betreiber-Schritt** (GUI-Werkzeug,
  echte Quellen) — dieses Häppchen liefert den Weg und die Messlatte,
  nicht den ausgefüllten Bericht. Kein CI-Gate; Wiederholung nach jedem
  ICD-Bump empfohlen.
- **Keine wahrheitsbasierte Genauigkeit** aus Track-only-Daten: COMPASS
  kennt die echten Flugzeugpositionen ebenso wenig wie wir — der
  Gegen-Check bestätigt Konformität und Konsistenz; die absolute
  Genauigkeit bleibt beim HA.4-Messstand (Simulator-Wahrheit).
- Werkzeug-Differenzen (Spezifikations-Editionen) werden dokumentiert,
  nicht stillschweigend wegdiskutiert.
