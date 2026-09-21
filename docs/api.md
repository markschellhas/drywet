# API reference

<!-- kicker: Public surface -->

The crate root re-exports the main runtime types:

```rust
use drywet::{Context, DeviceSink, Drum, Loop, Part, Sampler, Sequence, Synth};
```

## Context and sinks

- `Context::new()` creates a `Context<BufferSink>` at 44.1 kHz mono.
- `Context::with(sample_rate, channels, sink)` creates a context for a supplied `Sink`.
- `DeviceSink::new()` opens the platform's default output format through CPAL.
- `BufferSink` stores rendered samples for tests and offline processing.
- `PipeWireSink::new(sample_rate, channels)` is a callback/testing sink.

## Transport

`ctx.transport()` returns a mutable transport reference. Use `set_bpm(f64)`, `set_time_signature`, `set_loop`, and `set_loop_points` for configuration. `start`, `pause`, and `stop` mutate transport state and return the reference directly. `ctx.render(duration)` renders into the configured sink.

## Instruments and events

`Synth::new(&ctx)`, `Drum::new(&ctx)`, and `Sampler::from_directory(&ctx, dir)` create instruments. Trigger methods take `&Context` first, followed by note/voice, duration, and an optional attack time. `Sequence`, `Part`, and `Loop` register callbacks with `start(&mut ctx.transport(), offset)`.

## Engine

`drywet-engine` reads newline-delimited JSON from stdin. It uses `DeviceSink` by default and `BufferSink` with `--buffer`. See [Engine protocol](engine.md) for commands and payloads.

## Modules

Lower-level types live in `drywet::time`, `drywet::event`, `drywet::transport`, `drywet::instrument`, and `drywet::sink`. The root intentionally does not expose the old `ContextConfig`, root `Event`, or a crate-level `Result` alias.
