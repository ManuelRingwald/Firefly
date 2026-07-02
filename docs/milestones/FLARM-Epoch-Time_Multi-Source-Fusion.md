# FLARM-Epoch-Zeitstempel — Fix für die Multi-Source-Live-Fusion

> Bezug: Wayfinder-Issue **#120** („Bei zugewiesenem Feed mit ADS-B + FLARM
> kommen keine Tracks an"). Betrifft `firefly-flarm` (Adapter), kein
> CAT062-Wire-Format-Change.

## Fachlich — welches Problem

Ein **kombinierter Live-Feed** aus **ADS-B (OpenSky)** und **FLARM (OGN/APRS)**
lieferte im ASD **keine Tracks**, obwohl **jede Quelle allein** funktioniert.
Für den Lotsen heißt das: ein Feed, der eigentlich *mehr* Lagebild liefern soll
(kooperative Motorluftfahrt **plus** Segelflug/GA), zeigt in Summe **weniger** —
ein stiller Totalausfall der Zusammenführung. Für ein ASD ist „still weniger
statt mehr" die gefährlichste Fehlerklasse.

## Technisch — Root Cause

Der Live-Tracker fusioniert **alle** Quellen in **einem** Kanal und ist
**datenzeit-getrieben** (ADR 0003). `Tracker::process_plots`
(`firefly-track`) führt einen **globalen, monotonen Datenzeit-Wasserstand**
(`data_time_watermark`): eine Plot-Gruppe mit `t < watermark` wird als
„out-of-order" **verworfen** (Rückwärts-Kalman-Prädiktion ist undefiniert —
Robustheit gegen umsortierte Eingaben, FR-TRK-033).

Die beiden Adapter stempelten die Plot-Zeit aber auf **unterschiedlichen Uhren**:

| Quelle | Plot-Zeit bisher | Größenordnung |
|--------|------------------|---------------|
| OpenSky (`poller.rs`) | `resp.time` = **Unix-Epoch-Sekunde** | ~1,78 × 10⁹ |
| FLARM (`plot.rs`) | `time_of_day_s` = **Sekunden seit UTC-Mitternacht** (oder `0.0`) | 0 … 86 400 |

Sobald ein OpenSky-Plot den Wasserstand auf Epoch-Niveau zog, lag **jeder**
FLARM-Plot ~1,78 Mrd. Sekunden „in der Vergangenheit" → wurde verworfen. Allein
lief jede Quelle mit **in sich konsistenter** Uhr, daher fielen die
Einzel-Feeds nicht auf. Der Encoder maskierte den Fehler zusätzlich, weil
I062/070 ohnehin `… rem_euclid(86400)` rechnet — beide Uhren erzeugen *einzeln*
korrekte Tageszeit, nur ihre **absoluten Beträge** sind inkompatibel.

Der alte Code-Kommentar hatte die Lücke sogar markiert: *„the integration layer
reconciles the time origin (Schritt C)"* — dieser Reconcile-Schritt fehlte.

## Der Fix

`position_to_plot` (`firefly-flarm/src/plot.rs`) stempelt die Plot-Zeit jetzt als
**volle Unix-Epoch-Sekunde**, auf derselben Uhr wie OpenSky:

```
epoch = utc_midnight(now) + time_of_day_s          (Beacon mit HHMMSSh)
epoch = now                                         (Beacon ohne Zeitstempel)
```

- **Tages-Anker** ist die Wanduhr-Empfangszeit (`unix_now_s()` in `aprsis.rs`,
  aus `SystemTime`). Der OGN-Push-Strom ist Nahe-Echtzeit, daher ist der
  Empfangszeitpunkt der korrekte Tagesbezug für die reine Tageszeit des Beacons.
- **Tageswechsel-Korrektur:** liegt `epoch` mehr als einen halben Tag vor/nach
  `now` (Beacon kurz vor Mitternacht, Empfang kurz danach — oder umgekehrt), wird
  auf den nächstgelegenen Tag geschnappt, damit die Zeit nie ~24 h daneben liegt.
- **Determinismus/Replay** bleibt gewahrt: die aufgelöste Epoch-Zeit wird in den
  Plot (und damit in die `.ffplots`-Aufzeichnung) eingebrannt; der Replay ist
  reproduzierbar. Wanduhr wird nur beim **Live-Empfang** als Tagesanker benutzt —
  genau wie OpenSky seine Epoch-Zeit live vom API bezieht.

## Warum kein Wire-Format-Change

Der CAT062-Ausgabe-Vertrag ist **unberührt**: I062/070 wird weiterhin als
Tageszeit `(start + t) rem_euclid 86400` kodiert. Geändert wird nur die
**interne Datenzeit-Basis** eines Quell-Adapters, damit der gemeinsame
Fusions-Wasserstand konsistent ist. Kein ADR-pflichtiger Schnittstellen-Change;
Nachtrag im Anforderungs-Register (FR-NET-012) genügt.

## Tests

`firefly-flarm` (`plot.rs`):
- `plot_time_is_unix_epoch_anchored_to_the_receive_day` — Epoch, nicht ToD.
- `missing_beacon_time_falls_back_to_receive_time` — Fallback.
- `day_boundary_beacon_before_midnight_received_after_snaps_back` — Tageswechsel.

`cargo test --workspace`, `cargo clippy --workspace`, `cargo fmt` grün.
