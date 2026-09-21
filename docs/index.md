# A musician runtime for Rust audio apps

<!-- kicker: Overview -->

`drywet` is a small Rust runtime for arranging notes, rendering a sample-accurate timeline, and sending the result to an audio output. It supports native device playback through CPAL, deterministic offline rendering, and a line-oriented engine for UI integrations.

## Quick start

```rust
use drywet::{Context, DeviceSink, Sequence, Synth};
use std::{cell::RefCell, rc::Rc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sink = DeviceSink::new()?;
    let mut ctx = Context::with(sink.sample_rate(), sink.channels(), sink);
    let synth = Rc::new(RefCell::new(Synth::new(&ctx)));
    let voice = Rc::clone(&synth);
    let mut seq = Sequence::new(
        move |time, note| {
            if let Some(note) = note {
                let _ = voice.borrow_mut().trigger_attack_release(
                    &ctx, note, "8n", Some(time.into()),
                );
            }
        },
        vec!["C4", "E4", "G4", "B4"],
        "8n",
    );

    ctx.transport().set_bpm(120.0)?;
    seq.start(&mut ctx.transport(), 0)?;
    ctx.render("1m")?;
    ctx.sink_mut().play()?;
    ctx.sink().wait_until_end()?;
    Ok(())
}
```

The complete runnable versions are in [`examples/`](../examples/README.md). `DeviceSink` selects the native default output device and negotiates its sample rate and channel count.

## Runtime pieces

| Piece | Role |
| --- | --- |
| `Context` | Owns the transport, timeline, and sink. |
| `Sequence`, `Part`, `Loop` | Schedule callbacks on musical time. |
| `Synth`, `Drum`, `Sampler` | Generate voices and sample-backed sounds. |
| `DeviceSink` | Native live output through CPAL. |
| `BufferSink` | Deterministic in-memory rendering for tests and exports. |
| `PipeWireSink` | Callback sink retained for integration/testing; it is not the live example output. |
| `drywet-engine` | JSON-lines control process for external UIs. |

See [Getting started](getting-started.md), [Scheduling](scheduling.md), [Output](output.md), and [Engine protocol](engine.md).

## Design constraints

The transport is sample-clocked, so a context has one sample rate and channel count for its lifetime. Live playback renders a finite phrase, starts the sink, and keeps the process alive until the sink drains. For interactive applications, keep the context and instruments on their owning thread and trigger notes while the device is playing.
