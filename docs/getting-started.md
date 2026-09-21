# Getting started

<!-- kicker: Setup -->

Create a Context, attach an instrument, schedule events on the Transport, then start the clock. The same steps apply to live widgets and offline tests.

## Install

### Library

Add the crate to an app that already uses Cargo:

```toml
# Cargo.toml
[dependencies]
drywet = { git = "https://github.com/markschellhas/drywet" }
```

Or clone this repo and use the path:

```toml
drywet = { path = "../drywet" }
```

`Context` defaults to `BufferSink` so tests never open PipeWire. For live output, pass a `PipeWireSink` or use the engine binary.

### Engine binary

Build and run the stdio host:

```text
cargo run --bin drywet-engine
cargo run --bin drywet-engine -- --buffer   # BufferSink, no sound server
```

Install a local binary if a host should spawn it by name:

```text
cargo install --path . --bin drywet-engine
```

### Vendored widget (primary v1 path)

Plugin hosts that clone git and skip a compiler should vendor a prebuilt `drywet-engine` next to the UI. The end user does not install drywet separately.

```text
my-widget/
  ui/                    # QML, CLI, or other host files
  samples/C4.wav
  bin/drywet-engine      # vendored Linux binary
```

The host spawns `bin/drywet-engine` and speaks NDJSON. Sample banks stay in the consumer’s repo — drywet does not download sounds.

> [!NOTE]
> **Sample banks are not included.** Callers pass WAV paths. A widget that vendors the engine should also vendor its own `samples/`.

## Play a synth note

The built-in additive Synth can fill chord and key widgets when no sample bank is loaded. Live triggers omit `time` (`None`) and mix at the current write cursor.

```rust
use drywet::{Context, ContextConfig, PipeWireSink, Synth};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let synth = Synth::new(&ctx);

ctx.transport().set_bpm(100)?;
ctx.transport().start()?;
synth.trigger_attack_release("C4", "2n", None)?; // now, on the write cursor
```

`start()` returns when the sink has accepted the first audio (immediately for `BufferSink`) and exposes `latency_ms` so a UI playhead can align.

## Schedule a sequence

A Sequence is silent until `Transport::start()`. The callback receives a sample-accurate `time` and should pass it into the instrument.

```rust
use drywet::{Context, ContextConfig, PipeWireSink, Sequence, Synth};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let synth = Synth::new(&ctx);

let seq = Sequence::new(
    |time, note| synth.trigger_attack_release(note, "8n", Some(time)),
    ["C4", "E4", "G4", "B4"],
    "4n",
);
seq.start(0)?;

let t = ctx.transport();
t.set_loop(true);
t.set_loop_points("0:0:0", "1:0:0")?;
t.set_bpm(120)?;
t.start()?;
```

## Render without a sound server

CI and unit tests use `BufferSink` so they do not open PipeWire. That is also the `Context` default.

```rust
use drywet::{BufferSink, Context, ContextConfig, Synth};

let sink = BufferSink::new();
let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(sink)),
    ..Default::default()
});
let synth = Synth::new(&ctx);
synth.trigger_attack_release("A4", "4n", Some(0.0))?;

let pcm = ctx.transport().render("1m")?;
assert!(!pcm.is_empty());
```

Or rely on the default sink:

```rust
let ctx = Context::new(ContextConfig::default());
```

## Mix a live note onto playback

A chord widget, MIDI keyboard, or click can fire notes while a Sequence is running. Those notes mix onto the same stream. A second Transport, or a process per note, is not needed.

```rust
use drywet::{Context, ContextConfig, Drum, Loop, PipeWireSink, Synth};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let synth = Synth::new(&ctx);
let drum = Drum::new(&ctx);

let click = Loop::new(
    |time, _| drum.trigger_attack_release("kick", "16n", Some(time)),
    "4n",
);
click.start(0)?;
ctx.transport().start()?;

// UI key-down: mix immediately, no time argument
synth.trigger_attack_release("E4", "8n", None)?;
```

## Spawn the engine from a UI

Hosts that cannot link Rust spawn one persistent engine process and write NDJSON lines to stdin.

```text
{"cmd": "warmup", "instrument": "synth"}
{"cmd": "start", "bpm": 120, "loop": true}
{"cmd": "play-midi", "note": "C4", "duration": "8n"}
{"cmd": "stop"}
{"cmd": "shutdown"}
```

```text
cargo run --bin drywet-engine
```

Full command list and schedule payloads: [Stdio engine](engine.md).

## Next

- [Context & Transport](context-transport.md) — playhead, loop, BPM, latency
- [Musical time](time.md) — `"4n"`, dotted, triplets, bars:beats:sixteenths
- [Examples](examples.md) — metronome, drums, sampler, CI
