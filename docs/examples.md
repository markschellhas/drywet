# Examples

<!-- kicker: Examples -->

The runnable examples use the system's real default output device through
`DeviceSink` and CPAL. On macOS this is CoreAudio. Tests and the explicitly
offline examples continue to use `BufferSink`; no runnable music example uses
`MockStream`.

```text
cargo run --example metronome
cargo run --example chords
cargo run --example drums
cargo run --example sixeight
cargo run --example bassline
cargo run --example piano      # requires samples/piano/*.wav
cargo run --example jam
```

Each CLI example renders a finite sample-accurate phrase, starts the native
device stream, and waits until the device callback has consumed the phrase.
This wait only keeps the process alive; musical events are timed by their
sample offsets, not by `thread::sleep`.

The complete, compiling sources are:

- [metronome.rs](../examples/metronome.rs) — quarter-note kick/hat click
- [chords.rs](../examples/chords.rs) — a synthesized C-major pad
- [drums.rs](../examples/drums.rs) — looping sixteen-step kit pattern
- [sixeight.rs](../examples/sixeight.rs) — twelve-step 6/8 grid
- [bassline.rs](../examples/bassline.rs) — rests and nested subdivision
- [piano.rs](../examples/piano.rs) — `C4.wav`-style sampler directory
- [jam.rs](../examples/jam.rs) — a live synth note over a drum loop

They share the small native-device setup in
[support/mod.rs](../examples/support/mod.rs):

```rust
let sink = DeviceSink::new()?;
let ctx = Context::with(sink.sample_rate(), sink.channels(), sink);

// Attach Sequence / Part / Loop events and trigger instruments here.
ctx.render("2m")?;
ctx.sink_mut().play()?;
ctx.sink().wait_until_end()?;
```

`DeviceSink` uses the output device's negotiated sample rate and channel count,
so `Context` and its instruments render in the exact format consumed by the
device callback.

## Offline render

The offline example uses `Context::new()` and therefore `BufferSink`. It writes
raw signed 16-bit little-endian PCM to `phrase.pcm` without opening a device:

```text
cargo run --example render
```

See [render.rs](../examples/render.rs) for the compiling source.

## Engine session

The normal engine binary now opens `DeviceSink`. Pass `--buffer` for an
explicitly silent/offline process:

```text
cargo run --bin drywet-engine
cargo run --bin drywet-engine -- --buffer
cargo run --example engine_session
```

`engine_session` deliberately uses `--buffer` because it demonstrates the
NDJSON protocol rather than device playback. More protocol detail:
[Stdio engine](engine.md).
