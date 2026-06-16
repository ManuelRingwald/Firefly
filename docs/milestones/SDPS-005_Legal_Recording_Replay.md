# SDPS-005 — Legal Recording & Replay

**Datum:** 2026-06-16
**Anforderung:** FR-OPS-005
**Dateien:** `crates/firefly-recorder/` (neu: `Cargo.toml`, `src/lib.rs`, `src/bin/record.rs`, `src/bin/replay.rs`)
**Komplexität:** S2 · Sonnet 4.6

---

## Fachliche Motivation

Im operativen SDPS-Betrieb ist die Fähigkeit, jede vergangene Lagesituation
bit-genau zu rekonstruieren, keine optionale Komfortfunktion — sie ist eine
regulatorische und operative Kernaufgabe:

- **Vorfalls-Untersuchung:** Nach einem Near-Miss oder Runway-Incursion muss
  die zuständige Behörde (z. B. BFU/ANSV) den exakten Verlauf der ASD-Anzeige
  rekonstruieren können.
- **Training & Simulation:** Echte Verkehrssituationen sind die wertvollsten
  Trainingsdaten. Ein Replay-Feed füttert denselben Wayfinder-Client wie der
  Live-Feed — ohne Anpassung an der Client-Seite.
- **Abnahme & Regression:** Neue Firefly-Versionen können gegen aufgezeichnete
  Szenen getestet werden, um Regressionspfade zu reproduzieren.

**Determinismus als Schlüsseleigenschaft.** Firefly verarbeitet nach Datenzeit
(ASTERIX Time-of-Day, nicht Wanduhr). Gleicher Input → gleicher Output. Das
bedeutet: Wer die Datagramme auf dem Multicast-Bus aufzeichnet, hat implizit
den vollständigen Systemzustand erfasst. Ein späteres Replay dieser Bytes an
Wayfinder reproduziert das Lagebild exakt, ohne dass der Wayfinder-Client
etwas davon weiß.

---

## Architektur: Sidecar-Prinzip

Der Recorder ist ein **separater Prozess** (Sidecar), der nichts weiter tut als
auf demselben Multicast-Bus zu lauschen, den Wayfinder und jeder andere Konsument
ebenfalls benutzen. Er schreibt nie in Firefly-interne Datenstrukturen; er ändert
keine Schnittstelle; er kennt den Firefly-Kern nicht.

```
                     UDP-Multicast 239.255.0.62:8600
Firefly ─────────────────────────────┬──────────────
                                     │
                              ┌──────┴──────┐
                         ┌────┴────┐   ┌────┴───────────┐
                         │Wayfinder│   │firefly-record  │
                         │  (ASD)  │   │  (Sidecar)     │
                         └─────────┘   └────┬───────────┘
                                            │ .ffrec-Datei
                                       ┌────┴───────────┐
                                       │firefly-replay  │
                                       └────┬───────────┘
                                            │ UDP-Multicast (Replay)
                                       ┌────┴────┐
                                       │Wayfinder│
                                       │(Analyse)│
                                       └─────────┘
```

---

## Dateiformat `.ffrec`

Einfaches Binärformat; kein Index, keine Prüfsummen, keine Kompression —
Einfachheit ist eine Audit-Tugend, jedes Byte ist direkt erklärbar.

```
┌─────────────────────────────────────────────────┐
│  File header (16 Byte)                          │
│    magic:    8 Byte  = b"FFREC\x00\x00\x00"    │
│    version:  1 Byte  = 0x01                     │
│    reserved: 7 Byte  = 0x00…                    │
├─────────────────────────────────────────────────┤
│  Record 0                                       │
│    timestamp_unix_ns: u64 big-endian            │
│    length:            u16 big-endian            │
│    payload:           <length> Byte             │
├─────────────────────────────────────────────────┤
│  Record 1 …                                     │
└─────────────────────────────────────────────────┘
```

Der Zeitstempel ist **Unix-Nanosekunden** (Wall-Clock bei Empfang).

---

## Technische Umsetzung

### `crates/firefly-recorder/src/lib.rs` — Format-Bibliothek

Öffentliche API:
- `write_file_header(w: &mut impl Write)` — schreibt den 16-Byte-Header
- `read_file_header(r: &mut impl Read) → Result<(), ReadError>` — liest und prüft Header
- `write_record(w, ts_ns, payload)` — schreibt einen Datensatz
- `read_record(r) → Result<Option<(u64, Vec<u8>)>, ReadError>` — liest nächsten Datensatz, `None` = sauberes EOF

Fehlertypen in `ReadError`: `Io`, `BadMagic`, `UnsupportedVersion`, `PayloadTooLarge`.

### `src/bin/record.rs` — `firefly-record`

1. Socket auf `UNSPECIFIED:PORT` binden, Multicast-Gruppe joinen.
2. Datei anlegen, Header schreiben (`BufWriter` für Effizienz).
3. `tokio::select!`-Loop: `recv_from` vs. `Ctrl+C` — bei Ctrl+C flush + exit.
4. Jeder Datagramm-Empfang: `SystemTime::now()` → Nanosekunden → `write_record`.

### `src/bin/replay.rs` — `firefly-replay`

1. Datei öffnen, Header lesen.
2. Sender-Socket binden (ephemerer Port, kein Multicast-Beitritt nötig).
3. Ersten Zeitstempel als `origin` merken; `wall_start = Instant::now()`.
4. Pro Record: `target = wall_start + (ts - origin) / speed` — `sleep_until(target)` — `send_to`.

**Drift-Freiheit:** Statt Inter-Paket-Gaps aufzusummieren, berechnet der Replayer
jeden Absende-Zeitpunkt **absolut** relativ zum Startpunkt. Das verhindert
kumulative Zeitdrift bei langen Aufzeichnungen.

---

## Konfiguration (Env-Variablen, 12-Factor)

| Variable | Default | Bedeutung |
|----------|---------|-----------|
| `FIREFLY_CAT062_GROUP` | `239.255.0.62` | Multicast-Gruppe |
| `FIREFLY_CAT062_PORT` | `8600` | UDP-Port |
| `FIREFLY_RECORD_OUTPUT` | `recording.ffrec` | Ausgabe-Datei (record) |
| `FIREFLY_REPLAY_INPUT` | `recording.ffrec` | Eingabe-Datei (replay) |
| `FIREFLY_REPLAY_SPEED` | `1.0` | Wiedergabe-Geschwindigkeit |
| `RUST_LOG` | `info` | Log-Level (tracing) |

---

## Schnittstellen-Wirkung

Keine. Weder CAT062-ICD noch Wayfinder-Protokoll werden berührt. Der Recorder
ist ein passiver Konsument; der Replayer verhält sich gegenüber Wayfinder exakt
wie Firefly selbst.

---

## Qualitäts-Gates

- `cargo test --workspace` ✅ (6 Unit-Tests in `firefly-recorder::lib`)
- `cargo clippy --workspace --all-targets` ✅ (keine Warnungen)
- `cargo fmt` ✅
- FR-OPS-005 im Anforderungs-Register eingetragen ✅
