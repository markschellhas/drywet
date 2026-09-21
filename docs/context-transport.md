# Context & Transport

<!-- kicker: Context and clock -->

`Context` owns sample rate, channel count, the sink, and one Transport. The Transport is the arrangement clock: start / stop / pause, loop points, BPM, and a queryable playhead.

## Create a Context

Defaults are 44100 Hz, mono, and `BufferSink` so tests never open a sound server. Stereo is opt-in. Live Linux output takes a `PipeWireSink`.

```rust
use drywet::{BufferSink, Context, ContextConfig, PipeWireSink};

let ctx = Context::new(ContextConfig::default());
// same as:
let ctx = Context::new(ContextConfig {
    sample_rate: 44100,
    channels: 1,
    sink: None, // BufferSink
});

let stereo = Context::new(ContextConfig {
    channels: 2,
    ..Default::default()
});
let live = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let offline = Context::new(ContextConfig {
    sink: Some(Box::new(BufferSink::new())),
    ..Default::default()
});
```

| Property | Default | Role |
| --- | --- | --- |
| `sample_rate` | `44100` | Integer sample clock |
| `channels` | `1` | Mono default; stereo allowed |
| `sink` | `BufferSink` | Tone Destination analogue — instruments do not `connect()` |
| `transport` | one per Context | Arrangement playhead |

## Start, stop, pause, toggle

`state` is one of `started`, `stopped`, `paused`.

```rust
let t = ctx.transport();
t.start()?;                 // returns after the sink accepts first audio
assert_eq!(t.state(), TransportState::Started);
t.pause()?;
t.start()?;                 // resume from pause
t.toggle()?;                // start ↔ pause/stop as in Tone
t.stop()?;                  // flush tails, then silence; keep the Context
```

Stop flushes sounding audio according to release/tail, then silence, without tearing down the Context. Sample cache lives across bars. Shutdown (engine) or dropping the Context tears the sink down.

## BPM and time signature

Set BPM before start so scheduled events stay sample-accurate. A BPM write while the Transport is started is stored and applied on the next `start`. There is no `bpm.rampTo` in v1.

```rust
let t = ctx.transport();
t.set_bpm(128)?;
t.set_time_signature(4, 4)?;
t.set_time_signature(4, 4)?; // int 4 is accepted as 4/4 at the engine protocol layer
t.set_time_signature(6, 8)?;
```

Unlike Tone’s “numerator over 4” reduction, drywet always stores `(numerator, denominator)` so 6/8 widgets stay 6/8. An int `4` on the engine protocol is accepted as 4/4.

## Looping

```rust
let t = ctx.transport();
t.set_loop(true);
t.set_loop_start("0:0:0")?;
t.set_loop_end("2:0:0")?;
// or:
t.set_loop_points("0:0:0", "2:0:0")?;
```

Loop wraps the last tail so each cycle stays the nominal length. Drum tails mix forward into the next bar; the wrap keeps the cycle from growing.

## Playhead and latency

Query the playhead from the host process. A UI needle should not be driven from the audio thread or from QTimer guesses.

| Property | Meaning |
| --- | --- |
| `position` | Bars:beats:sixteenths, e.g. `"1:2:0"` |
| `seconds` | Playhead in seconds |
| `ticks` | Integer ticks at PPQ (default 192, Tone’s `Transport.PPQ`) |
| `state` | `started` / `stopped` / `paused` |
| `latency_ms` | Stream latency after start (negotiated for PipeWire; `0` for BufferSink) |

```rust
use std::time::Instant;

ctx.transport().start()?;
let started_at = Instant::now();
let latency = ctx.transport().latency_ms() as f64 / 1000.0;

let visual_seconds = || {
    if ctx.transport().state() != TransportState::Started {
        ctx.transport().seconds()
    } else {
        (started_at.elapsed().as_secs_f64() - latency).max(0.0)
    }
};
```

> [!WARNING]
> **Use the callback time for notes.** Scheduled instrument triggers should use the `time` argument from Sequence / Part / Loop / `schedule`. Wall-clock alignment is for drawing a playhead, not for placing notes.

## Low-level schedule

Most apps use [Sequence, Part, and Loop](scheduling.md). The Transport also exposes Tone’s timeline primitives.

```rust
let t = ctx.transport();
let id1 = t.schedule(|time| synth.trigger_attack_release("C4", "4n", Some(time)), "1m")?;
let id2 = t.schedule_once(|time| println!("one shot {time}"), "+4n")?;
let id3 = t.schedule_repeat(on_beat, "4n", 0)?;

t.cancel_after("2:0:0")?;   // drop events after this time
t.clear();                  // remove scheduled events
```

Callbacks run on the scheduler thread, not the audio write thread. They must not block and they must not write PCM. Call instrument triggers at `time`. Schedule length over 600 seconds is an error.

## Transport events

The Transport emits `start`, `stop`, `pause`, and `loop` with the event time, as Tone does. Hosts may ignore these and poll `position`.

```rust
ctx.transport().on("loop", |time| println!("cycle {time}"));
ctx.transport().on("start", |time| println!("started at {time}"));
```

## Limits

Exceeding a v1 cap returns `Err` instead of hanging the sink.

| Limit | Range |
| --- | --- |
| BPM | 40–240 |
| MIDI notes | 0–127 |
| Frequency | 20–20000 Hz |
| PPQ | default 192 |
| Schedule length | 600 seconds |

```rust
match ctx.transport().set_bpm(12) {
    Err(err) => eprintln!("tempo out of range: {err}"),
    Ok(()) => {}
}
```

> [!NOTE]
> **Thread safety.** The audio callback must not allocate, take blocking locks, parse NDJSON, or do file I/O. Note and transport changes arrive through a lock-free queue from another thread. UI hosts should call Transport methods from one control thread.
