# GUI integration

The GUI owns controls and visualization; drywet owns the sample clock, scheduling, and audio output.

## In-process Rust GUI

For egui, iced, or another Rust GUI, create a native device sink once and keep the context and instruments on their owning thread:

```rust
let sink = drywet::DeviceSink::new()?;
let mut ctx = drywet::Context::with(sink.sample_rate(), sink.channels(), sink);
let mut synth = drywet::Synth::new(&ctx);
ctx.sink_mut().play()?;

// In a UI callback:
synth.trigger_attack_release(&ctx, "C4", "8n", None)?;
```

For scheduled callbacks, use `Rc<RefCell<_>>` around the instrument and capture the context as shown in [Scheduling](scheduling.md). Avoid moving `Context`, instruments, or schedulers between GUI threads.

## External GUI

For QML, web, or another process, use `drywet-engine`. The GUI writes one JSON command per line and reads one JSON response per line:

```text
GUI -> stdin:  {"cmd":"note-on", "note":"C4"}
GUI <- stdout: {"ok":true}
```

The engine uses `DeviceSink` by default. Pass `--buffer` in tests or when deterministic in-memory rendering is required. See [Engine protocol](engine.md) for the complete command set.

## Rendering for visualization

Use `Context::new()` and `ctx.render(duration)` when the UI needs a waveform or level meter without opening an audio device. `BufferSink` is intentionally offline; use `DeviceSink` for audible playback.
