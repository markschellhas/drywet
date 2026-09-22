# drywet compiling Rust API

Matches `src/` and `tests/` on this repo. Crate version `0.1.0`.

## Crate root re-exports

```rust
drywet::{VERSION, Context, run, Loop, Part, Sequence, Drum, Sampler, Synth, BufferSink, PipeWireSink, Sink, Insert, InsertChain, InsertError, Bus, BusError, BusId, MixDest}
```

Everything else is namespaced: `drywet::transport`, `drywet::event`, `drywet::time`, `drywet::pitch`, `drywet::limits`, `drywet::instrument`.

There is **no** `ContextConfig`, `drywet::Result`, `drywet::Event`, or `drywet::Transport` at the crate root.

## Context

```rust
Context::new()                                      // 44100, 1 ch, BufferSink
Context::with(sample_rate, channels, sink)          // generic Context<S: Sink>
ctx.sample_rate() -> u32
ctx.channels() -> u16
ctx.sink() / ctx.sink_mut()                         // Ref / RefMut
ctx.transport() -> TransportRef<'_, S>
ctx.to_seconds(impl IntoTime) -> Result<f64, TimeError>
ctx.set_inserts(impl IntoIterator<Item = Box<dyn Insert>>) -> Result<(), InsertError>
ctx.clear_inserts() -> Result<(), InsertError>
ctx.bus("drums") -> Result<Bus, BusError>           // create-or-get; "master" is InvalidName
ctx.render(impl IntoTime) -> Result<Vec<f32>, TransportError>  // buses folded + wet copy; sink.frames() stay dry master
ctx.tick(impl IntoTime) -> Result<(), TransportError>           // live driver; no-op unless Started
ctx.live_origin() -> f64                                       // write_cursor / sample_rate
ctx.live_mix_time(origin_s, event_s) -> Option<TimeValue>      // None if behind playhead
```

Default `Context` is `Context<BufferSink>`. Live Linux:

```rust
use drywet::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};
use drywet::{Context, PipeWireSink};

let ctx = Context::with(
    DEFAULT_SAMPLE_RATE,
    DEFAULT_CHANNELS,
    PipeWireSink::new(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS),
);
ctx.sink_mut().start_clock();
```

`PipeWireSink::new(sr, ch)` always injects `MockStream` (no libpipewire, no speakers). `drywet-engine` without `--buffer` still uses that constructor today. A hardware backend is `PipeWireSink::with_backend(sr, ch, backend)` where `backend: StreamBackend`. Call `start_clock()` to open the injected stream. The process callback is `sink.process(&mut output)` — it must not allocate. `frames()` is empty on this sink; use `BufferSink` to inspect PCM.

## Transport / TransportRef

`Transport` is constructable (`Transport::new()`). Hosts normally go through `ctx.transport()`.

| Method | Role |
| --- | --- |
| `start` / `stop` / `pause` / `toggle` | Lifecycle. `start` from stopped resets seconds to 0 and copies pending BPM |
| `state` | `TransportState::{Stopped, Started, Paused}` |
| `set_bpm` / `bpm` / `clock_bpm` | 40–240. While started, write is pending until next start from stopped |
| `set_time_signature` | `4` → `(4, 4)` or `(n, d)`. Zero n/d is `Err` |
| `seconds` / `set_seconds` / `ticks` / `position` | Playhead |
| `set_loop` / `loop_start` / `loop_end` / `set_loop_points` | Wrap playhead into `[start, end)` |
| `on("start"\|"stop"\|"pause"\|"loop", cb)` | Listeners get time in seconds |
| `latency_ms` | From the sink (0 for BufferSink) |
| `to_seconds` / `to_ticks` / `to_frequency` | Clock-aware convert |
| `schedule` / `schedule_once` / `schedule_repeat` | Callbacks get `f64` seconds; must not write PCM |
| `cancel(after)` / `cancel_ids` / `clear` | Drop events |
| `dispose` | `clear` + `stop` + `sink.close` (TransportRef only) |
| `fire_until` | Drive scheduled callbacks without mixing |
| `tick(lookahead)` | Live driver: `fire_until(seconds+lookahead)` while Started; no-op if Stopped/Paused |

`schedule` rejects times `< 0` or `> 600`. `schedule_repeat` rejects non-positive intervals when occurrences are generated.

`Sequence` / `Part` / `Loop` accept either `Transport` or `TransportRef` via `SequenceClock`.

## Time and pitch

```rust
drywet::time::{to_seconds, to_ticks, to_frequency, IntoTime, TimeValue, TimeError}
drywet::pitch::{note_to_midi, midi_to_hz, IntoNote, PitchError}
```

`IntoTime` accepts `&str`, `String`, `f64`/`f32`/`i32`/`u32`/`i64`, `TimeValue`.
`IntoNote` accepts `&str`, `String`, `i32`.

`TimeValue::from(0.0)` or `0.0.into()` is the usual `Option<TimeValue>` for `time = 0`. Callback seconds: `Some(time.into())`.

Note-value regex (after trim): optional `+`, digits, `n`/`m`/`t`, optional `.`.
BBS: `bars:beats:sixteenths` (sixteenths may be fractional).

`note_to_midi("C4") == 60`, `"A4" == 69`, `"C#4"` / `"Db4" == 61`. `midi_to_hz(69) ≈ 440`.

## Instruments

All mix onto `ctx.sink` (master) unless `_on` is used. Shared pattern:

```rust
inst.trigger_attack(&ctx, note, time)
inst.trigger_attack_release(&ctx, note, duration, time)
inst.trigger_attack_on(&ctx, &bus, note, time)
inst.trigger_attack_release_on(&ctx, &bus, note, duration, time)
inst.trigger_release(note, time)   // voice count only; PCM already mixed
inst.release_all(time)
```

`time: Option<TimeValue>` — `None` = write cursor.

### Synth

Additive 4 harmonics, 10 ms attack / 50 ms release, gain 0.18. `trigger_attack` mixes 1.0 s. Default 32 voices (`with_max_voices`). No velocity.

### Drum

`trigger(&ctx, name, time)` — `kick` (~0.22 s), `snare` (~0.16 s), `hat`/`hi-hat`/`hihat` (~0.05 s). `trigger_attack_release` ignores duration.

### Sampler

```rust
Sampler::new(&ctx)
Sampler::with_max_voices(&ctx, n)
Sampler::with_map(&ctx, [(note, path), ...], max_voices)
Sampler::from_directory(&ctx, dir)   // ^([A-Ga-g][#b]?\d+)\.wav$
sampler.add(note, path)
sampler.samples() -> &HashMap<u8, Vec<f32>>
sampler.loop_flag()                  // stored; held-loop mixer not implemented
```

WAV: RIFF/WAVE, PCM format 1, 16-bit. Stereo frames averaged to mono. Rate mismatch linear-resampled to context rate.

`InstrumentError`: `VoiceLimitExceeded`, `UnknownDrum`, `Pitch`, `Time`, `InvalidWav`, `EmptySampler`, `Bus`.

Drum also has `trigger_on(&ctx, &bus, name, time)`.

## Sequence / Part / Loop

```rust
use drywet::event::SequenceEvent;

Sequence::new(Fn(f64, Option<&str>), events, subdivision)
Part::new(Fn(f64, &str), [(time, event), ...])
Loop::new(Fn(f64), interval)

obj.start(&mut transport, offset) -> Result<&mut Self, TransportError>
obj.stop(&mut transport)
```

`SequenceEvent`: `Rest`, `Value(String)`, `Group(Vec<...>)`. `From<&str>`, `From<Option<&str>>` (`None` → Rest), `From<Vec<_>>` / arrays → Group. Helper: `SequenceEvent::group(["E4", "G4"])`.

Flatten: parent slot width = `to_seconds(subdivision) * len(events)`; nested groups split that slot equally. Rests occupy a slot but do not fire.

`.start` replaces a previous attachment (stops first). `.stop` cancels only this object's ids.

## Sinks

```rust
trait Sink {
    fn mix(&mut self, frames: &[f32], at_sample: Option<usize>);
    fn write(&mut self, frames: &[f32]);
    fn stop(&mut self);
    fn close(&mut self);
    fn latency_ms(&self) -> u32;
    fn write_cursor(&self) -> usize;
    fn accepted(&self) -> bool;
    fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError>;
    fn apply_inserts(&mut self, frames: &mut [f32]);
    fn ensure_bus(&mut self, name: &str) -> Result<Bus, BusError>;
    fn mix_on(&mut self, dest: MixDest, frames: &[f32], at_sample: Option<usize>) -> Result<(), BusError>;
    fn fold_into(&mut self, frames: &mut Vec<f32>);
    fn mark_accepted(&mut self);
    fn start_clock(&mut self);
    fn frames(&self) -> &[f32];
}
```

`BufferSink::new(sample_rate, channels)` — expanding f32 buffer. Stereo duplicates each mono frame. `to_pcm_s16le()` clips to [-1, 1] and packs LE i16. `stop`/`close` are no-ops.

`mix` at `Some(at)` does not advance the write cursor. `write` mixes at the cursor and advances. Tails sum. Mix and write never run inserts. Defaults for `set_inserts` / `apply_inserts` are no-ops; BufferSink, PipeWireSink, and DeviceSink override. `mix_on` routes dry PCM to master or a named bus. `fold_into` sums processed buses into a master-dry copy, then runs master inserts.

## Inserts

```rust
trait Insert: Send + 'static {
    fn process(&mut self, frames: &mut [f32]); // mono sample stream
}

InsertChain::new()
chain.set(inserts) -> Result<(), InsertError>  // ChainFull { max, got } if > MAX_INSERTS
chain.process(&mut frames)                     // first insert first
ctx.set_inserts(vec![Box::new(my_insert) as Box<dyn drywet::Insert>])?
```

Chain order is playback order. Mix stays dry. `render` / `PipeWireSink::process` / the DeviceSink callback run the chain. Empty chain is identity.

## Buses

```rust
let drums = ctx.bus("drums")?;                      // create or get; stable for the Context
drums.set_inserts(vec![Box::new(my_insert) as Box<dyn drywet::Insert>])?;
drums.clear_inserts()?;
sampler.trigger_attack_on(&ctx, &drums, "C4", Some(time.into()))?;
```

`BusError`: `BusFull { max, got }`, `InvalidName` (`""` or `"master"`), `UnknownBus`. Extra buses cap at `MAX_BUSES` (not counting master). Dropping the handle does not destroy the bus. Default `trigger_*` still mix to master.

## Limits (`drywet::limits`)

| Constant | Value |
| --- | --- |
| `MIDI_MIN` / `MIDI_MAX` | 0 / 127 |
| `HZ_MIN` / `HZ_MAX` | 20 / 20000 |
| `BPM_MIN` / `BPM_MAX` | 40 / 240 |
| `DEFAULT_PPQ` | 192 |
| `DEFAULT_SAMPLE_RATE` | 44100 |
| `DEFAULT_CHANNELS` | 1 |
| `DEFAULT_MAX_VOICES` | 32 |
| `MAX_SCHEDULE_SECONDS` | 600 |
| `DEFAULT_LOOKAHEAD_S` | 0.04 |
| `MAX_INSERTS` | 8 |
| `MAX_BUSES` | 8 extra named buses (not counting master) |

## Engine helper

`drywet::run(stdin, stdout, sink) -> io::Result<Rc<Context<S>>>` — in-process NDJSON handler used by `drywet-engine` and `tests/engine.rs`. `R: BufRead + Send + 'static`. While Started, ticks on a 40 ms stdin timeout. `shutdown` disposes and returns; EOF returns without dispose.

## Sharing from callbacks

```rust
let ctx = Rc::new(Context::new());
let synth = Rc::new(RefCell::new(Synth::new(&ctx)));
```

Do not put `Context` on a worker thread. Fire notes on the owning thread, or send note names over a channel to that thread.
