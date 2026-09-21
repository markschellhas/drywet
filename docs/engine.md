# Stdio engine

<!-- kicker: Host adapter -->

QML overlays and other UIs that cannot link Rust spawn `drywet-engine`. The process reads NDJSON on stdin and writes NDJSON on stdout. In-process apps import the `drywet` crate instead of spawning the engine.

The verbs match [drywet-py](https://github.com/markschellhas/drywet-py) so an Omarchy widget can switch hosts without a new protocol.

## Spawn

Spawn one persistent child. Sample cache lives across bars. `stop` keeps the process; `shutdown` exits.

```text
cargo run --bin drywet-engine
# or a vendored binary:
./bin/drywet-engine
# tests / CI:
cargo run --bin drywet-engine -- --buffer
```

Default sink is PipeWire. `--buffer` uses `BufferSink`.

```rust
use std::io::Write;
use std::process::{Command, Stdio};

let mut proc = Command::new("drywet-engine")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;

{
    let stdin = proc.stdin.as_mut().unwrap();
    writeln!(stdin, r#"{{"cmd":"warmup","instrument":"synth"}}"#)?;
}
```

Each message is one JSON object on one line. Pretty-printing across lines will break the parser.

## Commands

| cmd | Tone analogue | Role |
| --- | --- | --- |
| `warmup` | Sampler load | Load Synth, Drum, or Sampler; open the sink |
| `start` | `Transport.start` | Start Transport (optional loop, bpm, schedule payload) |
| `stop` | `Transport.stop` | Stop Transport; destination keeps writing silence |
| `pause` | `pause` | Pause clock |
| `resume` | `start` from pause | Resume clock |
| `play-midi` | `triggerAttackRelease` now | One-shot live notes on the current instrument |
| `note-on` / `note-off` | `triggerAttack` / `triggerRelease` | Held voice; also accepted as `trigger_attack` / `trigger_release` |
| `bpm` | `Transport.bpm` | Set tempo (applied on the next `start` if already running) |
| `shutdown` | `dispose` | Close sink and exit |

### warmup

```text
{"cmd": "warmup", "instrument": "synth"}
{"cmd": "warmup", "instrument": "drum"}
{"cmd": "warmup", "instrument": "sampler", "directory": "samples/"}
{"cmd": "warmup", "instrument": "sampler", "urls": {"C4": "samples/C4.wav", "G4": "samples/G4.wav"}}
```

### start / stop / pause / resume

```text
{"cmd": "start", "bpm": 120, "loop": true, "loopStart": "0:0:0", "loopEnd": "1:0:0"}
{"cmd": "pause"}
{"cmd": "resume"}
{"cmd": "stop"}
```

### play-midi, note-on / note-off, and bpm

```text
{"cmd": "play-midi", "note": "C4", "duration": "8n"}
{"cmd": "note-on", "note": "C4"}
{"cmd": "note-off", "note": "C4"}
{"cmd": "play-midi", "note": 60, "duration": "4n"}
{"cmd": "play-midi", "note": "kick", "duration": "16n"}
{"cmd": "bpm", "value": 96}
```

`warmup` starts the destination clock. After `stop`, `note-on` still sounds on the same stream.

BPM writes while started apply on the next `start`. There is no live ramp.

### shutdown

```text
{"cmd": "shutdown"}
```

## Events out

The engine writes one JSON object per line on stdout.

```text
{"event": "ok", "cmd": "warmup"}
{"event": "started", "latencyMs": 12, "position": "0:0:0", "frames": 0}
{"event": "error", "cmd": "bpm", "message": "BPM 12 out of range 40-240"}
```

A UI playhead should treat `started.latencyMs` as a constant offset from the wall-clock moment the event arrived. A QTimer guess will drift. For PipeWire this is the negotiated stream latency, not a fixed 80 ms.

## Schedule payloads

Schedule payloads are drywet Sequence / Part / Loop JSON (generic events on the Transport), not a host app’s song document. Host apps that have their own documents translate into this JSON.

```text
{
  "cmd": "start",
  "bpm": 110,
  "loop": true,
  "schedule": {
    "type": "sequence",
    "subdivision": "8n",
    "events": ["C3", null, "G3", "C3"],
    "start": 0
  }
}
```

```text
{
  "cmd": "start",
  "schedule": {
    "type": "part",
    "events": [
      ["0:0:0", "C4"],
      ["0:1:0", "E4"],
      ["0:2:0", ["G4", "B4"]]
    ]
  }
}
```

```text
{
  "cmd": "start",
  "bpm": 80,
  "schedule": {
    "type": "loop",
    "interval": "4n",
    "note": "hat",
    "duration": "32n"
  }
}
```

A list of schedule objects is also valid when a host needs drums plus a bassline:

```text
{
  "cmd": "start",
  "bpm": 96,
  "schedule": [
    {"type": "sequence", "instrument": "drum", "subdivision": "16n", "events": ["kick", null, "hat", null, "snare", null, "hat", null]},
    {"type": "sequence", "instrument": "synth", "subdivision": "8n", "events": ["C2", null, "G2", "C2"]}
  ]
}
```

## Full session

Host → engine:

```text
{"cmd": "warmup", "instrument": "sampler", "directory": "samples/piano"}
{"cmd": "start", "bpm": 100, "loop": true, "schedule": {"type": "sequence", "subdivision": "4n", "events": ["C4", "E4", "G4", "B3"]}}
{"cmd": "play-midi", "note": "C5", "duration": "8n"}
{"cmd": "stop"}
{"cmd": "shutdown"}
```

Engine → host:

```text
{"event": "ok", "cmd": "warmup"}
{"event": "started", "latencyMs": 12, "position": "0:0:0"}
{"event": "ok", "cmd": "play-midi"}
{"event": "ok", "cmd": "stop"}
```

```text
printf '%s\n' \
  '{"cmd":"warmup","instrument":"synth"}' \
  '{"cmd":"start","bpm":120,"loop":true}' \
  '{"cmd":"play-midi","note":"C4","duration":"8n"}' \
  '{"cmd":"stop"}' \
  '{"cmd":"shutdown"}' \
  | cargo run --quiet --bin drywet-engine -- --buffer
```

> [!NOTE]
> **The engine is headless.** It does not implement circle-of-fifths widgets, song documents, or QML. Translate those in the host, then send Sequence / Part / Loop JSON.
