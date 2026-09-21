# Context and transport

<!-- kicker: Runtime state -->

## Context construction

```rust
let mut offline = drywet::Context::new();

let sink = drywet::DeviceSink::new()?;
let mut live = drywet::Context::with(sink.sample_rate(), sink.channels(), sink);
```

`Context::new()` creates a 44.1 kHz mono `BufferSink`. `Context::with` accepts any `Sink`; use the sink's negotiated format for device output. A context owns one transport and one sink for its lifetime.

## Transport controls

```rust
live.transport().set_bpm(120.0)?;
live.transport().set_time_signature("4/4")?;
live.transport().set_loop(true);
live.transport().set_loop_points("0:0:0", "8:0:0")?;
live.transport().start();
live.transport().pause();
live.transport().stop();
```

`start`, `pause`, and `stop` return the transport reference for chaining; do not apply `?` to them. `set_bpm`, `set_time_signature`, and `set_loop_points` validate their inputs and return a `Result`.

## Rendering and playback

`ctx.render(duration)` advances the transport and renders into the configured sink. For a live phrase:

```rust
ctx.render("1m")?;
ctx.sink_mut().play()?;
ctx.sink().wait_until_end()?;
```

`DeviceSink::play` opens and starts the native output stream. Keep the process alive while the stream is draining. For a long-running application, call `play` once, start the transport, and keep the event loop alive while triggering instruments.

## Threading

Contexts and instruments are intentionally lightweight and stateful. Keep them together on their owning thread; use an application message queue when a GUI or network thread needs to request notes.
