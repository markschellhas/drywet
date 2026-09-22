# Getting started

<!-- kicker: First steps -->

## Install

Add the crate to `Cargo.toml`:

```toml
[dependencies]
drywet = "0.1"
```

On macOS, CPAL uses CoreAudio. On Linux and other platforms it uses the host's default CPAL backend. The examples use `DeviceSink`, so they play through the default device rather than a mock sink.

## Play one note

```rust
use drywet::{Context, DeviceSink, Synth};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sink = DeviceSink::new()?;
    let mut ctx = Context::with(sink.sample_rate(), sink.channels(), sink);
    let mut synth = Synth::new(&ctx);
    synth.trigger_attack_release(&ctx, "C4", "2n", None)?;
    ctx.render("1m")?;
    ctx.sink_mut().play()?;
    ctx.sink().wait_until_end()?;
    Ok(())
}
```

`Context::new()` is the convenient `BufferSink` context for offline work. `Context::with(rate, channels, sink)` is used when the sink determines the audio format, as with `DeviceSink`.

## Schedule a phrase

Callbacks receive the scheduled time in seconds and an optional event value. Instrument methods take the context explicitly:

```rust
use drywet::{Context, DeviceSink, Sequence, Synth};
use std::{cell::RefCell, rc::Rc};

let sink = DeviceSink::new()?;
let mut ctx = Context::with(sink.sample_rate(), sink.channels(), sink);
let synth = Rc::new(RefCell::new(Synth::new(&ctx)));
let voice = Rc::clone(&synth);
let mut seq = Sequence::new(
    move |time, note| {
        if let Some(note) = note {
            let _ = voice.borrow_mut().trigger_attack_release(
                &ctx, note, "16n", Some(time.into()),
            );
        }
    },
    vec!["C4", "D4", "E4", "G4"],
    "8n",
);
ctx.transport().set_bpm(124.0)?;
seq.start(&mut ctx.transport(), 0)?;
ctx.render("2m")?;
ctx.sink_mut().play()?;
ctx.sink().wait_until_end()?;
```

Use `ctx.transport().start()` only for a live, long-running transport. It returns the transport reference, not a `Result`, and is a state change — it does not fire callbacks or spawn a clock. In-process hosts must call `ctx.tick(lookahead)` (40 ms is [`drywet::limits::DEFAULT_LOOKAHEAD_S`]) from their frame loop. Stdio hosts spawn `drywet-engine`, which pumps `tick` while Transport is started. Offline clips still use `render` then `play` then drain.

## Offline rendering

```rust
let mut ctx = Context::new();
let pcm = ctx.render("4n")?;
```

`render` advances the transport and fills a `BufferSink`. It does not open an audio device. Use `BufferSink::into_samples()` or `ctx.sink().samples()` to inspect the result.

## Engine mode

Run `cargo run --bin drywet-engine` for native device output, or pass `--buffer` for a deterministic in-memory sink. The engine reads one JSON object per line; see [Engine protocol](engine.md).
