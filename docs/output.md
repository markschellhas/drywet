# Output sinks

<!-- kicker: PCM output -->

drywet writes raw PCM. The Linux live path is a persistent PipeWire callback. Tests and offline bounce use an in-memory `BufferSink`. A custom sink implements the `Sink` trait: `write`, `mix`, `stop`, `close`, plus `latency_ms`, `write_cursor`, and `accepted`.

## PipeWireSink

Live default on Linux when you pass one in (the engine binary does this). One persistent stream. No `pw-play` / `pw-cat` child per hit.

```rust
use drywet::{Context, ContextConfig, PipeWireSink};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let stereo = Context::new(ContextConfig {
    channels: 2,
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
```

- PipeWire is output. The Transport is the arrangement clock; `pw-play` of discrete files is not.
- `start()` opens one long-lived callback stream. `latency_ms` is the measured or negotiated stream latency, not a hardcoded 80 ms.
- The callback path does not allocate, parse NDJSON, or do file I/O. Notes arrive from a preallocated queue filled on another thread.
- `Transport::stop()` keeps the stream open and writes silence. Only `close()` / dropping the Context tears the device down.
- Live and one-shot audio mix at the write cursor. Hosts should not keep a second PipeWire clock.
- Use one process and one sink. A process per note is not supported.

The `Sink` trait is the extension point. A later `cpal` backend can cover other machines without changing the musician API.

> [!WARNING]
> **Tests use BufferSink.** Hardware devices, JACK, and PipeWire are out of scope for automated tests. CI runs `cargo test` with BufferSink only.

## BufferSink and render

In-memory PCM for tests and `transport.render(duration)` — Tone Offline *role*, not `OfflineAudioContext`. `Context` defaults to this sink.

```rust
use drywet::{BufferSink, Context, ContextConfig, Sequence, Synth};

let sink = BufferSink::new();
let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(sink)),
    sample_rate: 44100,
    channels: 1,
});
let synth = Synth::new(&ctx);

let seq = Sequence::new(
    |time, note| synth.trigger_attack_release(note, "8n", Some(time)),
    ["C4", "E4", "G4", "C5"],
    "8n",
);
seq.start(0)?;
ctx.transport().set_bpm(120)?;

let pcm = ctx.transport().render("1m")?;
assert_eq!(
    pcm.len(),
    (ctx.to_seconds("1m")? * ctx.sample_rate() as f64) as usize
);
assert!(pcm.iter().any(|sample| *sample != 0.0));
```

Scheduled arrangement is rendered onto the sample clock (concatenation with tails mixed forward). Bar 1, step 0 is sample 0 of that bar.

`mix(frames, at_sample)` adds into an expanding `f32` buffer. Stereo duplicates a mono frame across channels. `write(frames)` mixes at `write_cursor` and advances it. `to_pcm_s16le` clips to [-1, 1] and packs little-endian i16. `stop` / `close` are no-ops. `latency_ms` is 0. `accepted` becomes true after the first mix/write.

```rust
let pcm = ctx.transport().render("2m")?;
let bytes = sink.to_pcm_s16le();
std::fs::write("phrase.wav", wav_header_and_bytes(&ctx, &bytes))?;
```

## Custom sinks

Apps may pass any type that implements `Sink`:

```rust
struct NullSink;

impl drywet::Sink for NullSink {
    fn write(&mut self, frames: &[f32]) { let _ = frames; }
    fn mix(&mut self, frames: &[f32], at_sample: usize) { let _ = (frames, at_sample); }
    fn stop(&mut self) {}
    fn close(&mut self) {}
    fn latency_ms(&self) -> u32 { 0 }
    fn write_cursor(&self) -> usize { 0 }
    fn accepted(&self) -> bool { true }
}

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(NullSink)),
    ..Default::default()
});
```

`write` consumes the arranged stream. `mix` overlays one-shots at a sample index. `stop` stops the arrangement and keeps the destination open; `close` releases the device or buffer.

## Live mix

Live notes (keys, clicks, MIDI) mix onto the write cursor of the playing stream. Held `trigger_attack` voices keep sounding on that mixer until release. A practice metronome and a chord widget can share one engine this way. After `Transport::stop()`, a click still uses the same open stream — no device handshake.

```rust
ctx.transport().start()?;
// arrangement already rendering into the sink...
synth.trigger_attack_release("A4", "16n", None)?; // mixed now
```

v1 renders onto integer sample frames in Rust. Per-sample DSP is not a public contract for hosts; they only trigger instruments at `time`.
