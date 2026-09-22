# Audio output

<!-- kicker: Sinks -->

## Native device output

`DeviceSink` uses CPAL's default output device (CoreAudio on macOS and the platform's native CPAL host elsewhere):

```rust
let sink = drywet::DeviceSink::new()?;
let mut ctx = drywet::Context::with(sink.sample_rate(), sink.channels(), sink);
// schedule events...
ctx.render("1m")?;
ctx.sink_mut().play()?;
ctx.sink().wait_until_end()?;
```

The device chooses the sample rate and channel count. Query those values before constructing the context so scheduling and rendering use the same format.

## Buffer output

`Context::new()` creates a 44.1 kHz mono `BufferSink`:

```rust
let mut ctx = drywet::Context::new();
let pcm = ctx.render("1m")?;
```

Use this sink for tests, waveform inspection, and file/export pipelines. It never opens an audio device.

## Playback insert

`mix` and `write` stay dry. Attach a chain on the Context; output runs it:

```rust
ctx.set_inserts(vec![Box::new(my_insert) as Box<dyn drywet::Insert>])?;
let pcm = ctx.render("1m")?;
```

`render` returns a processed copy. `sink.frames()` has the same length and stays dry. `PipeWireSink::process` and the `DeviceSink` callback apply the same chain after summing queued PCM. An empty chain is identity. There is no engine NDJSON command for inserts.

## Output buses

Named extra mix destinations share the insert contract. Instruments mix dry onto a bus or onto master. At output, each bus is processed, folded into the master stream, then optional master inserts run. Zero extra buses is today's behavior.

```rust
let drums = ctx.bus("drums")?;
drums.set_inserts(vec![Box::new(my_insert) as Box<dyn drywet::Insert>])?;
sampler.trigger_attack_on(&ctx, &drums, "C4", Some(time.into()))?;
synth.trigger_attack_release(&ctx, "E4", "8n", Some(time.into()))?;
```

`ctx.bus("master")` is reserved. Same name returns the same bus for the Context lifetime. Dropping the handle does not destroy the bus. `MAX_BUSES` extra buses (not counting master); creating more is `Err`. There is no engine NDJSON command for buses.

## PipeWire sink

`PipeWireSink::new(sample_rate, channels)` is a callback sink for integration and testing. It is not the default live path for examples and does not select or open a system output device. Use `DeviceSink` for audible playback.

## Sink lifecycle

The `Sink` trait exposes `write`, `play`, `stop`, `is_playing`, and `wait_until_end`. Rendering fills the sink; `play` starts playback. Keep the process alive until `wait_until_end` returns, or keep a live transport and stream running in your application's event loop.
