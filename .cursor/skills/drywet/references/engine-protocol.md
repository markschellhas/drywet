# drywet-engine NDJSON (as implemented)

Binary: `src/bin/drywet-engine.rs`. Handler: `src/engine.rs`. Tests: `tests/engine.rs`.

```text
cargo run --bin drywet-engine              # PipeWireSink<MockStream> (callback sink, no device)
cargo run --bin drywet-engine -- --buffer  # BufferSink (inspectable PCM; what tests use)
```

Context is always 44100 Hz, 1 channel. One persistent process. `stop` keeps the process and sink. `shutdown` disposes and exits.

Each stdin line is one JSON **object**. Empty lines are skipped. Pretty-printed multi-line JSON fails.

## Commands the binary accepts

| `cmd` | Also accepted | Effect |
| --- | --- | --- |
| `warmup` | | Replace the live instrument; `start_clock` on the sink |
| `start` | | Optional bpm + loop flag + attach schedule; start Transport; reply `started` |
| `stop` | | Stop Transport (sink stays) |
| `pause` | | Pause if started |
| `resume` | | `transport.start()` |
| `play-midi` | | `trigger_attack_release` now (`time` omitted) |
| `note-on` | `trigger_attack` | `trigger_attack` (optional `time`) |
| `note-off` | `trigger_release` | Decrement voice count |
| `bpm` | | `set_bpm` from `value` |
| `shutdown` | | `dispose`, emit `ok`, exit `run` |

Unknown `cmd` → `{"error":"unknown cmd: ..."}`. Invalid JSON → `{"error":"..."}`.

## Replies (stdout, one object per line)

Implemented shapes:

```text
{"ok": true}
{"event": "started", "latencyMs": 0, "position": "0:0:0"}
{"error": "BPM must be 40–240, got 12"}
```

Not implemented (docs only): `{"event":"ok","cmd":"warmup"}`, `{"event":"error","cmd":"...","message":"..."}`, `frames` on `started`.

Hosts should accept `ok: true`, `event == "started"`, and a present `error` string.

## warmup

```text
{"cmd":"warmup","instrument":"synth"}
{"cmd":"warmup","instrument":"drum"}
{"cmd":"warmup","instrument":"sampler","directory":"samples/"}
{"cmd":"warmup","instrument":"sampler","urls":{"C4":"samples/C4.wav"}}
{"cmd":"warmup","instrument":"sampler","map":{"C4":"samples/C4.wav"},"loop":false}
```

`instrument` defaults to `"synth"`. Sampler `map` and `urls` are the same note→path object. `loop` is stored and unused (no held-loop mixer). Unknown instrument → error.

## start

```text
{"cmd":"start","bpm":120,"loop":false}
```

| Field | Implemented? | Notes |
| --- | --- | --- |
| `bpm` | yes | Number or numeric string |
| `loop` boolean | yes | Transport loop on/off. Null/absent → off |
| `loop` object | schedule only | Does **not** set Transport loop. See Loop below |
| `loopStart` / `loopEnd` | **no** | Ignored |
| `sequence` | yes | See below |
| `part` | yes | See below |
| `schedule` | **no** | Silently ignored |

You may send `sequence` and/or `part` and/or a Loop object on the same `start`. All attach to the current warmup instrument.

### Sequence payload (implemented)

```text
{"cmd":"start","bpm":120,"sequence":{"events":["C4","G3",null,["E4","G4"]],"subdivision":"4n"}}
```

- `events` must be a JSON array. `null` is a rest. Nested arrays subdivide.
- `subdivision` string, default `"4n"`.
- Callback plays `trigger_attack_release(..., "8n", Some(time))`.

### Part payload (implemented)

```text
{"cmd":"start","part":{"events":[["0:0:0","C4"],{"time":"4n","event":"E4"}]}}
```

Each event is `[time, note]` or `{"time":...,"event"|"note":...}`. `time` is a string or number.

### Loop payload (implemented)

```text
{"cmd":"start","loop":{"interval":"4n"}}
```

`interval` string, default `"4n"`. The callback **always** plays `"C4"` for `"8n"` — there is no `note` or `duration` field.

To also enable Transport looping, send a **boolean** `loop` on a different command, or do not combine a Loop object with `"loop": true` on the same object (the object wins and Transport loop is left untouched).

Typical Transport loop from a host: `{"cmd":"start","bpm":120,"loop":true}` with no Loop schedule.

## play-midi / note-on / note-off / bpm

```text
{"cmd":"play-midi","note":"C4","duration":"8n"}
{"cmd":"play-midi","note":60,"duration":0.25}
{"cmd":"note-on","note":"E4"}
{"cmd":"note-off","note":"E4","time":null}
{"cmd":"bpm","value":100}
```

Defaults: note `"C4"`, duration `"8n"`, time omitted (now). `note` may be a string or number (numbers stringify, so MIDI `60` becomes `"60"` and is parsed as a note name — prefer `"C4"` or a drum name). Drum names work after `warmup` with `"drum"`.

After `stop`, `note-on` / `play-midi` still mix on the same sink.

BPM writes while started apply on the next start from stopped.

## Host spawn pattern

```rust
use std::io::Write;
use std::process::{Command, Stdio};

let mut child = Command::new("drywet-engine") // or cargo run --bin drywet-engine -- --buffer
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;
writeln!(child.stdin.as_mut().unwrap(), r#"{{"cmd":"warmup","instrument":"synth"}}"#)?;
```

Vendored widgets ship a prebuilt `x86_64-unknown-linux-gnu` binary next to QML. This repo does not ship QML or `manifest.json`. Sample banks stay in the consumer repo.

Worked QML / Ply notes: `docs/gui.md`. In-process tests of the handler: `drywet::run` + `BufferSink` (see `tests/engine.rs`).
