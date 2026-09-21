# drywet

<img src="drywet.jpg" alt="DryWet mix knob" width="96" height="96" />

A musician-facing runtime for desktop audio apps: one Transport, Tone-style time strings, Sampler / Synth / Drum, Sequence / Part / Loop, and live notes mixed onto a single playing stream.

Tone.js feels complete because it is two layers — a musician API and a hard audio engine (the browser). drywet keeps the first layer and implements the second in Rust with native device output through CPAL.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use drywet::{Context, DeviceSink, Sequence, Synth};

let sink = DeviceSink::new()?;
let ctx = Rc::new(Context::with(sink.sample_rate(), sink.channels(), sink));
let synth = Rc::new(RefCell::new(Synth::new(ctx.as_ref())));
ctx.transport().set_bpm(120.0)?;

let ctx_cb = Rc::clone(&ctx);
let synth_cb = Rc::clone(&synth);
let mut seq = Sequence::new(
    move |time, note| {
        if let Some(note) = note {
            synth_cb.borrow_mut()
                .trigger_attack_release(ctx_cb.as_ref(), note, "8n", Some(time.into()))
                .expect("sequence note");
        }
    },
    ["C4", "G3", "A3", "F3"],
    "4n",
);
seq.start(&mut ctx.transport(), 0)?;
ctx.render("1m")?;
ctx.sink_mut().play()?;
ctx.sink().wait_until_end()?;
```

The callback `time` is the sample clock. Language timers (`thread::sleep`, `QTimer`) are not. Live UI hits omit `time` (`None`) and mix at the current write cursor without stopping playback.

This repo is the Rust crate (`drywet`) plus the NDJSON stdio binary (`drywet-engine`). A Python prototype of the same API lives in [drywet-py](https://github.com/markschellhas/drywet-py) and is reference only.

The pages below are the v1 contract. Implementation follows [the plan](docs/plans/2026-09-20-drywet.md); `cargo test` and `cargo run --example` compile as those tasks land.

## Documentation

| Format | Start here |
| --- | --- |
| **HTML** | [docs/html/index.html](docs/html/index.html) |
| **Markdown** | [docs/index.md](docs/index.md) |

| Topic | Markdown | HTML |
| --- | --- | --- |
| Getting started | [docs/getting-started.md](docs/getting-started.md) | [html](docs/html/getting-started.html) |
| Context & Transport | [docs/context-transport.md](docs/context-transport.md) | [html](docs/html/context-transport.html) |
| Musical time | [docs/time.md](docs/time.md) | [html](docs/html/time.html) |
| Instruments | [docs/instruments.md](docs/instruments.md) | [html](docs/html/instruments.html) |
| Sequence, Part, Loop | [docs/scheduling.md](docs/scheduling.md) | [html](docs/html/scheduling.html) |
| Output sinks | [docs/output.md](docs/output.md) | [html](docs/html/output.html) |
| Stdio engine | [docs/engine.md](docs/engine.md) | [html](docs/html/engine.html) |
| Examples | [docs/examples.md](docs/examples.md) | [html](docs/html/examples.html) |
| API cheat sheet | [docs/api.md](docs/api.md) | [html](docs/html/api.html) |

Internal specs: [PRD](docs/prds/prd-drywet.md), [implementation plan](docs/plans/2026-09-20-drywet.md). Heritage maps: [drywet-py `.features/`](https://github.com/markschellhas/drywet-py/tree/master/.features).

Open the HTML docs locally from a clone:

```text
xdg-open docs/html/index.html
```

Regenerate HTML from Markdown after edits:

```text
python3 docs/html/build.py
```

## How to use

There are two entry points. In-process apps import the crate. QML hosts (Omarchy bar or menu plugins) spawn `drywet-engine` and speak NDJSON. End users of a vendored widget do not run `cargo`.

### Library

```toml
# Cargo.toml
[dependencies]
drywet = { git = "https://github.com/markschellhas/drywet" }
```

`Context` owns a sample rate, channel count, one sink, and one Transport. The default sink is `BufferSink`, so tests never open an audio device. For live output on macOS, Linux, or Windows, use `DeviceSink` and its negotiated device format:

```rust
use drywet::{Context, DeviceSink, Synth};

let sink = DeviceSink::new()?;
let ctx = Context::with(sink.sample_rate(), sink.channels(), sink);
let mut synth = Synth::new(&ctx);
synth.trigger_attack_release(&ctx, "C4", "2n", None)?;
ctx.render("1m")?;
ctx.sink_mut().play()?;
ctx.sink().wait_until_end()?;
```

Offline / CI — no sound server:

```rust
let ctx = Context::new();
let pcm = ctx.render("1m")?;
```

Musical time strings (`"4n"`, `"8n."`, `"8t"`, `"1m"`, `"+4n"`, `"4:0:0"`) resolve against that Transport. Invalid strings and out-of-range pitch or BPM return `Err`.

### Engine binary

Same NDJSON verbs as drywet-py, so a widget can switch hosts without a new protocol.

```text
cargo run --bin drywet-engine
cargo run --bin drywet-engine -- --buffer   # BufferSink, no audio device
```

```text
{"cmd": "warmup", "instrument": "synth"}
{"cmd": "start", "bpm": 120, "loop": true}
{"cmd": "play-midi", "note": "C4", "duration": "8n"}
{"cmd": "stop"}
{"cmd": "shutdown"}
```

Events out: `started` (`latencyMs`, position), `ok`, `error`. Schedule payloads are Sequence / Part / Loop JSON, not a host song document. Full command list: [docs/engine.md](docs/engine.md).

Omarchy plugins vendor a prebuilt `x86_64-unknown-linux-gnu` binary next to the QML. The plugin UI is QML; this repo does not ship QML or a `manifest.json`.

## How to run examples

Sketches live in [`examples/`](examples) and are documented in [docs/examples.md](docs/examples.md). After the crate builds:

```text
# Live output through the system's default audio device
cargo run --example metronome
cargo run --example chords
cargo run --example drums
cargo run --example sixeight
cargo run --example bassline
cargo run --example piano      # needs samples/piano/*.wav
cargo run --example jam

# Offline BufferSink → phrase.pcm (no sound server)
cargo run --example render

# NDJSON session against the engine (BufferSink)
cargo run --example engine_session
```

Or pipe a session yourself:

```text
printf '%s\n' \
  '{"cmd":"warmup","instrument":"drum"}' \
  '{"cmd":"start","bpm":100,"loop":true,"schedule":{"type":"loop","interval":"4n","note":"kick","duration":"16n"}}' \
  '{"cmd":"play-midi","note":"hat","duration":"32n"}' \
  '{"cmd":"stop"}' \
  '{"cmd":"shutdown"}' \
  | cargo run --quiet --bin drywet-engine -- --buffer
```

Tests never open a sound server:

```text
cargo test
```

## What v1 does not include

Web Audio nodes, effects, swing, velocity layers, Ableton Link, a mixer UI, recording, or a Python runtime. See the [API cheat sheet](docs/api.md#not-in-v1).

## License

MIT
