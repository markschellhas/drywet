# API cheat sheet

<!-- kicker: Reference -->

Public modules, constructors, and the Tone.js mapping. Rust names are snake_case. Type names stay Transport, Sampler, Sequence, Part, Loop.

## Modules

| Module | Exports |
| --- | --- |
| `drywet` | `Context`, `ContextConfig`, `Transport`, time helpers, `VERSION` |
| `drywet::instrument` | `Sampler`, `Synth`, `Drum` |
| `drywet::event` | `Sequence`, `Part`, `Loop` |
| `drywet::sink` | `Sink`, `PipeWireSink`, `BufferSink` |
| `drywet-engine` | stdio host binary |

## Context

```rust
Context::new(ContextConfig { sample_rate: 44100, channels: 1, sink: None })
ctx.transport()
ctx.sample_rate()
ctx.channels()
ctx.to_seconds(time)
ctx.to_ticks(time)
ctx.to_frequency(note)
```

`sink: None` constructs a `BufferSink`. Pass `Some(Box::new(PipeWireSink::new()?))` for live Linux output.

## Transport

| drywet v1 | Tone.js 14.7 |
| --- | --- |
| `start` / `stop` / `pause` / `toggle` | same |
| `set_bpm` | `Transport.bpm` (no `rampTo`; applied on next `start` if already running) |
| `set_loop`, `set_loop_start`, `set_loop_end`, `set_loop_points` | `loop`, `loopStart`, `loopEnd`, `setLoopPoints` |
| `position`, `seconds`, `ticks`, `PPQ`, `state` | same roles |
| `time_signature` as `(n, d)` | `timeSignature` (Tone reduces to numerator/4) |
| `schedule` / `schedule_once` / `schedule_repeat` | `schedule` / `scheduleOnce` / `scheduleRepeat` |
| `cancel` / `clear` / `dispose` | same |
| `render(duration)` | Offline role |
| `latency_ms` | desktop playhead offset (not in Tone) |

Events: `start`, `stop`, `pause`, `loop`.

## Instruments

```rust
Synth::new(&ctx)
Drum::new(&ctx)
Sampler::new(urls, &ctx)
Sampler::from_directory(path, &ctx)
sampler.add(note, path)
sampler.release_all(time)

// shared
inst.trigger_attack(note, time)
inst.trigger_release(note, time)
inst.trigger_attack_release(note, duration, time)
```

`time` is `None` for now, or `Some(...)` for a scheduled time. Drum voices: `"kick"`, `"snare"`, `"hat"`.

## Events

```rust
Sequence::new(callback, events, subdivision)
Part::new(callback, events)    // [(time, value), ...]
Loop::new(callback, interval)
obj.start(offset)
obj.stop()
obj.dispose()
```

`Event::{Note, Rest, Group}` covers rests and nested subdivision.

## Sinks

```rust
PipeWireSink::new()
BufferSink::new()
// trait: write, mix, stop, close, latency_ms, write_cursor, accepted
```

## Engine commands

`warmup`, `start`, `stop`, `pause`, `resume`, `play-midi`, `note-on`, `note-off`, `bpm`, `shutdown`.

Events out: `started` (`latencyMs`, `frames` or position), `ok`, `error`.

Default sink PipeWire; `--buffer` uses BufferSink.

## Limits

| Thing | v1 cap |
| --- | --- |
| BPM | 40–240 |
| MIDI | 0–127 |
| Hz | 20–20000 |
| Samples | WAV only; resample on load |
| PPQ | 192 default |
| Default I/O | s16le, 44100, mono |
| Schedule length | 600 seconds |
| Dependencies | Rust crate + host PipeWire for live output |

Exceeding a cap returns `Err`.

## Not in v1

- Web Audio: `AudioContext`, nodes, `toDestination()`, Signals, `Tone.Draw`
- `Instrument.sync` / `unsync`
- Effects, velocity layers, swing, humanize, probability, per-note automation
- Mixer UI, recording, clip warping, Ableton Link
- Required cargo / compiler for vendored widgets
- VST3 / JUCE / CLAP
- A Python runtime (see [drywet-py](https://github.com/markschellhas/drywet-py) for the prototype)
