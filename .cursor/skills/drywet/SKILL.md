---
name: drywet
description: Use the drywet Rust musician runtime (Context, Transport, Synth/Drum/Sampler, Sequence/Part/Loop, BufferSink/PipeWireSink, drywet-engine NDJSON). Use when writing or reviewing code that plays notes, schedules sequences, renders PCM, or drives the stdio engine — and whenever README/docs/examples disagree with src/.
license: MIT
---

# drywet — use the compiling crate

drywet is a Tone-style musician layer (one Transport, `"4n"` time, Sampler / Synth / Drum, Sequence / Part / Loop) with a Rust audio engine. Two host paths:

| Host | Entry | When |
| --- | --- | --- |
| In-process Rust | `drywet` crate | CLI, Ply, egui, tests |
| Out-of-process UI | `drywet-engine` NDJSON | Omarchy QML, any child process |

Language timers (`thread::sleep`, `QTimer`) are **not** the clock. Callback `time` is. Live UI hits omit `time` (`None`) and mix at the sink write cursor.

## Source of truth (read this first)

The Markdown docs and `examples/*.rs` describe a **planned** friendlier API (`ContextConfig`, `drywet::Result`, `seq.start(0)`, instruments without `&ctx`, `Event::{Note, Rest, Group}`, engine `"schedule": { "type": ... }`). **Those sketches do not compile or speak the current protocol.**

| Trust | Do not copy as code |
| --- | --- |
| `src/**/*.rs` | `README.md` snippets |
| `tests/**/*.rs` | `docs/*.md` except `docs/gui.md` |
| `docs/gui.md` (Ply / egui) | `examples/*.rs` (except `engine_session.rs` process spawn) |
| `src/engine.rs` for NDJSON | `docs/engine.md` `schedule` / `event: ok` shapes |

If you are **changing the public API** toward those docs, say so and update tests in the same change. If you are **using** the library, write the signatures in [references/rust-api.md](references/rust-api.md).

## Decide the host

1. **Can the app link Rust?** Use `Context` in-process. `Context` / `Synth` are `!Send` (`RefCell`). Keep them on one thread. Share with `Rc<RefCell<_>>` from `'static` UI callbacks.
2. **QML / other language?** Spawn one `drywet-engine` process. One JSON object per stdin line. Do not pretty-print across lines. See [references/engine-protocol.md](references/engine-protocol.md).
3. **Tests / CI / offline render?** Default `Context::new()` is `BufferSink`. Never open PipeWire in `cargo test`.

```toml
[dependencies]
drywet = { git = "https://github.com/markschellhas/drywet" }
# or: drywet = { path = "../drywet" }
```

## Working recipes (this crate, today)

### Live synth pad (in-process)

```rust
use drywet::{Context, Synth};

let ctx = Context::new(); // BufferSink. Swap via Context::with(..., PipeWireSink::new(sr, ch))
let mut synth = Synth::new(&ctx);
ctx.sink_mut().start_clock(); // no-op on BufferSink; opens PipeWire
ctx.transport().set_bpm(120)?;
ctx.transport().start();
synth.trigger_attack_release(&ctx, "C4", "8n", None)?; // now
```

`PipeWireSink::new` takes `(sample_rate, channels)` — not `PipeWireSink::new()?`. Defaults: 44100 Hz, 1 channel.

### Sequence that actually sounds

Instruments need `&Context`. Sequence callbacks are `'static`, so share with `Rc`:

```rust
use std::cell::RefCell;
use std::rc::Rc;
use drywet::{Context, Sequence, Synth};

let ctx = Rc::new(Context::new());
ctx.transport().set_bpm(120)?;
let synth = Rc::new(RefCell::new(Synth::new(&ctx)));

let ctx_cb = Rc::clone(&ctx);
let synth_cb = Rc::clone(&synth);
let mut seq = Sequence::new(
    move |time, note| {
        if let Some(note) = note {
            let _ = synth_cb.borrow_mut().trigger_attack_release(
                ctx_cb.as_ref(),
                note,
                "8n",
                Some(time.into()),
            );
        }
    },
    ["C4", "E4", "G4", "B4"],
    "4n",
);
seq.start(&mut ctx.transport(), 0)?;
let pcm = ctx.render("1m")?; // starts the clock, fires events, pads BufferSink
```

Rules:

- `Sequence::start` takes **`&mut impl SequenceClock` and an offset**. There is no `seq.start(0)`.
- Callback is `Fn(f64, Option<&str>)`. Rests pass `None`.
- Nested subdivision uses `drywet::event::SequenceEvent` (`Value`, `Rest`, `Group`), not `drywet::Event`.
- Callbacks must not block and must not write PCM. They only trigger instruments at `time`.
- `.start()` only registers ids. Nothing fires until `transport.start()`, `fire_until`, or `ctx.render`.

### Drum grid

```rust
use drywet::{Context, Drum};

let ctx = Context::new();
let mut drum = Drum::new(&ctx);
drum.trigger(&ctx, "kick", Some(0.0.into()))?;
drum.trigger(&ctx, "snare", Some(0.05.into()))?;
drum.trigger(&ctx, "hat", None)?; // hi-hat / hihat also work
```

Unknown names (`"cowbell"`) are `Err`. Beat steps: `Drum::steps_per_bar((4, 4)) == 16`.

### Sampler

WAV only, 16-bit PCM. Resampled on load. Missing pitches pitch-shift the nearest sample.

```rust
use drywet::{Context, Sampler};

let ctx = Context::new();
let mut sampler = Sampler::from_directory(&ctx, "samples/")?; // C4.wav stems
// or Sampler::with_map(&ctx, [("C4", path)], 32)?;
sampler.add("D4", "samples/D4.wav")?;
sampler.trigger_attack_release(&ctx, "C5", "8n", Some(0.0.into()))?;
```

Empty map → `InstrumentError::EmptySampler`. Polyphony default 32; `with_max_voices` to cap.

### Offline render vs live mix

```rust
let pcm = ctx.render("1m")?;                 // Vec<f32>, starts if needed
synth.trigger_attack_release(&ctx, "A4", 0.05, None)?; // live, does not stop clock
```

`render` lives on **`Context`**, not `Transport`. Playhead after render is the converted duration.

### Engine child (implemented verbs)

```text
{"cmd":"warmup","instrument":"synth"}
{"cmd":"start","bpm":120,"sequence":{"events":["C4","G3"],"subdivision":"4n"}}
{"cmd":"play-midi","note":"C4","duration":"8n"}
{"cmd":"stop"}
{"cmd":"shutdown"}
```

Replies today: `{"ok":true}`, `{"event":"started","latencyMs":0,"position":"0:0:0"}`, `{"error":"..."}`.

Do **not** send `"schedule":{"type":"sequence",...}` — `src/engine.rs` only reads top-level `sequence`, `part`, and a `loop` **object**. A boolean `"loop": true` is the Transport loop flag, not a `Loop` event.

## Time, pitch, limits

| Input | Examples | Notes |
| --- | --- | --- |
| Note value | `"4n"`, `"8n."`, `"8t"`, `"1m"` | At 120 BPM 4/4: `"4n"` = 0.5 s |
| Relative | `"+4n"` | Adds current playhead (`now`) |
| BBS | `"0:1:0"`, `"1:0:0"` | 0-based bars:beats:sixteenths |
| Seconds | `0.5`, `1.0` | Numerics pass through |
| Pitch | `"C4"`, `"C#4"`, `"Db4"`, `60` | MIDI 0–127. `"H4"` is `Err` |

BPM 40–240. Writes while **started** apply on the **next start from stopped**. PPQ 192. Max schedule 600 s. Hz 20–20000 (note names like `C0` are allowed even if < 20 Hz).

`ctx.to_seconds(value)` is safe from a schedule callback. `to_ticks` / `to_frequency` are on `Transport` / `TransportRef`.

## Transport

`ctx.transport()` is a short-lived `TransportRef` (`RefCell` guard). States: `stopped` → `start` → `started` → `pause` → `paused` → `start`/`toggle` → `started` → `stop` → `stopped`.

`stop` stops the arrangement clock; the sink stays open. `dispose` clears the schedule, stops, and closes the sink.

```rust
let mut t = ctx.transport();
t.set_time_signature((4, 4))?; // or `4` → (4, 4)
t.set_loop(true);
t.set_loop_points(0, "1m")?;
t.schedule(|time| { /* trigger at time; do not write PCM */ }, "4n")?;
t.schedule_repeat(cb, "4n", 0)?;
t.cancel("2m")?;
t.clear();
t.fire_until("1m")?;
```

Playhead: `seconds()`, `ticks()`, `position()` → `"bars:beats:sixteenths"`. After `start()` from stopped, seconds reset to 0. UI needles should offset wall clock by `latency_ms()` (0 on BufferSink).

## What v1 does not include

Web Audio nodes, effects, swing, velocity, Ableton Link, mixer UI, recording, SoundFonts, VST/CLAP, a Python runtime. Drum has kick / snare / hat only. `Sampler` `loop_flag` is stored but the held-loop mixer is not implemented. `trigger_release` / `release_all` decrement the voice count; they do **not** silence already-mixed PCM.

## Verify

```text
cargo test -q
```

Tests use BufferSink only. Live PipeWire tests are ignored unless `DRYWET_LIVE_AUDIO=1`. After an API change, run the test file that matches the module (`cargo test -q sequence`, `engine`, `render`, …).

## Load more detail

- Compiling signatures and types: [references/rust-api.md](references/rust-api.md)
- NDJSON as `src/engine.rs` implements it: [references/engine-protocol.md](references/engine-protocol.md)
- GUI hookup (accurate): `docs/gui.md`
- Concepts only (time strings, goals): `docs/time.md`, `docs/prds/prd-drywet.md`
