# A musician runtime for Rust audio apps

<!-- kicker: Overview -->

drywet is a Rust library and a small stdio binary that give desktop music widgets a musician-facing runtime: one Transport, musical time, Sampler, simple synths and drums, Sequence / Part / Loop, and live notes mixed onto one playing stream.

Tone.js feels complete because it is two layers: a musician API (Transport, `"4n"` time, Sampler, Sequence) and a hard audio engine (the browser). drywet takes the first layer and implements the second in Rust on PipeWire.

```rust
use drywet::{Context, ContextConfig, Sequence, Synth};

let ctx = Context::new(ContextConfig::default());
let synth = Synth::new(&ctx);
let seq = Sequence::new(
    |time, note| synth.trigger_attack_release(note, "8n", Some(time)),
    ["C4", "G3", "A3", "F3"],
    "1m",
);
seq.start(0)?;
ctx.transport().set_bpm(120)?;
ctx.transport().start()?;
```

Instruments use `trigger_attack_release`. Sequences attach to the Transport. The callback’s `time` argument is the sample clock, rather than `thread::sleep` or a UI timer.

A Python prototype of the same API lives in [drywet-py](https://github.com/markschellhas/drywet-py). That package is reference only. This repo does not depend on it.

## Who it is for

**In-process Rust.** CLI tools, desktop helpers, and later standalone apps import `drywet` and call Context, Transport, and instruments directly.

**Out-of-process UIs.** QML overlays (Omarchy bar or menu plugins) spawn `drywet-engine` and speak NDJSON. The library does not depend on Qt.

**Vendored widgets.** Plugin hosts vendor a prebuilt Linux binary. End users do not run `cargo` or `pip`.

**Tests and CI.** `BufferSink` and `transport.render()` keep tests off a sound server.

## Architecture

drywet follows Tone.js’s musician layer: a playhead, `"4n"` time, Sampler, and Sequence. Linux output is a persistent PipeWire callback (not one `pw-play` per hit).

| Layer | Role |
| --- | --- |
| UI process | QML, CLI, or other host |
| NDJSON | optional; in-process apps skip this |
| Rust runtime | `drywet` Context + Transport |
| raw PCM | s16le, 44100, default mono |
| PipeWireSink | one persistent stream |
| BufferSink | offline / CI path |

## Feature map

| You want to… | Use | Docs |
| --- | --- | --- |
| Own sample rate, channels, sink, and one clock | `Context` | [Context & Transport](context-transport.md) |
| Start, stop, loop, set BPM, read the playhead | `ctx.transport()` | [Context & Transport](context-transport.md) |
| Write `"8n"`, `"1m"`, `"0:2:0"` | `to_seconds` / `to_ticks` | [Musical time](time.md) |
| Map WAV files to notes and pitch-fill the rest | `Sampler` | [Instruments](instruments.md) |
| Play a chord widget with no sample bank | `Synth` | [Instruments](instruments.md) |
| Kick / snare / hi-hat grids | `Drum` | [Instruments](instruments.md) |
| Repeat events on the Transport | `Sequence`, `Part`, `Loop` | [Scheduling](scheduling.md) |
| Hear audio on Linux, or capture PCM in tests | `PipeWireSink`, `BufferSink` | [Output sinks](output.md) |
| Drive drywet from a non-Rust UI | `drywet-engine` | [Stdio engine](engine.md) |

## Constraints

- **Sample clock.** Arrangement is rendered onto integer sample frames. Language timers are not used as the clock.
- **Single sink.** Live notes mix onto the write cursor of the playing stream. A process per note is not supported.
- **PipeWire is output.** A callback stream plays PCM. The Transport is the playhead.
- **Playhead alignment.** After `start()`, use `started` plus `latency_ms` (the negotiated stream latency) for a UI needle. `QTimer` guesses will drift.
- **Stop keeps the process.** Sample cache lives across bars. `shutdown` tears the engine down.

> [!NOTE]
> **Scope.** drywet does not include `AudioContext`, Gain nodes, effects, swing, velocity layers, or a mixer UI. Rust names are snake_case. Type names stay Transport, Sampler, Sequence, Part, Loop.

## Install paths

**Vendored widget (v1 primary).** The plugin repo vendors a prebuilt `drywet-engine` Linux binary. The end user does not install Rust.

**In-process crate.** Apps that already use Cargo depend on `drywet` and call the library directly.

See [Getting started](getting-started.md) for a first sound, or [Examples](examples.md) for longer sketches.
