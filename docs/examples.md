# Examples

<!-- kicker: Examples -->

Sketches you can copy for common widgets: metronome, chords, drums, sampler piano, live notes over a loop, and an offline CI render.

Runnable copies live in [`examples/`](../examples). After the crate builds:

```text
cargo run --example metronome
cargo run --example chords
cargo run --example drums
cargo run --example sixeight
cargo run --example bassline
cargo run --example piano
cargo run --example jam
cargo run --example render
```

Live examples open PipeWire. Offline `render` uses `BufferSink` and writes `phrase.wav`. Engine protocol:

```text
cargo run --example engine_session
# or pipe NDJSON yourself:
printf '%s\n' '{"cmd":"warmup","instrument":"drum"}' ... \
  | cargo run --bin drywet-engine
```

## Practice metronome

```rust
use drywet::{Context, ContextConfig, Drum, Loop, PipeWireSink};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let drum = Drum::new(&ctx);

let click = Loop::new(
    |time, _| {
        let voice = if ctx.transport().position().ends_with(":0:0") {
            "kick"
        } else {
            "hat"
        };
        drum.trigger_attack_release(voice, "32n", Some(time))
    },
    "4n",
);
click.start(0)?;
ctx.transport().set_bpm(72)?;
ctx.transport().start()?;
```

Run: `cargo run --example metronome`

## Chord widget

This example uses the built-in Synth. Each button mixes a triad onto the current stream.

```rust
use drywet::{Context, ContextConfig, PipeWireSink, Synth};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let synth = Synth::new(&ctx);
ctx.transport().start()?;

fn on_pad(synth: &Synth, name: &str) -> drywet::Result<()> {
    let notes: &[&str] = match name {
        "C" => &["C4", "E4", "G4"],
        "Am" => &["A3", "C4", "E4"],
        "F" => &["F3", "A3", "C4"],
        "G" => &["G3", "B3", "D4"],
        _ => return Ok(()),
    };
    for note in notes {
        synth.trigger_attack_release(*note, "2n", None)?;
    }
    Ok(())
}

on_pad(&synth, "C")?;
```

Run: `cargo run --example chords`

## Drum machine

```rust
use drywet::{Context, ContextConfig, Drum, PipeWireSink, Sequence};
use std::collections::HashMap;

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let drum = Drum::new(&ctx);

let pattern = HashMap::from([
    ("kick",  [1,0,0,0, 1,0,0,0, 1,0,1,0, 1,0,0,0]),
    ("snare", [0,0,0,0, 1,0,0,0, 0,0,0,0, 1,0,0,1]),
    ("hat",   [1,0,1,0, 1,0,1,0, 1,0,1,0, 1,0,1,0]),
]);

let seq = Sequence::new(
    |time, i| {
        for (voice, hits) in &pattern {
            if hits[i as usize] == 1 {
                drum.trigger_attack_release(*voice, "16n", Some(time))?;
            }
        }
        Ok(())
    },
    0..16,
    "16n",
);
seq.start(0)?;
ctx.transport().set_bpm(108)?;
ctx.transport().set_loop(true);
ctx.transport().set_loop_points(0, "1m")?;
ctx.transport().start()?;
```

Run: `cargo run --example drums`

## 6/8 grid

Twelve sixteenth steps: `numerator * 16 / denominator`.

```rust
ctx.transport().set_time_signature(6, 8)?;
let (num, den) = ctx.transport().time_signature();
let steps = num * 16 / den; // 12
let kick = [1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0];

let seq = Sequence::new(
    |time, i| {
        if kick[i as usize] == 1 {
            drum.trigger_attack_release("kick", "16n", Some(time))?;
        }
        Ok(())
    },
    0..steps,
    "16n",
);
seq.start(0)?;
```

Run: `cargo run --example sixeight`

## Bassline Sequence

```rust
use drywet::{Context, ContextConfig, Event::*, PipeWireSink, Sequence, Synth};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let synth = Synth::new(&ctx);

let seq = Sequence::new(
    |time, note| synth.trigger_attack_release(note, "16n", Some(time)),
    [
        Note("C2"),
        Rest,
        Group(vec![Note("C2"), Note("G2")]),
        Note("A#1"),
        Note("C2"),
        Rest,
        Note("D2"),
        Note("G2"),
    ],
    "8n",
);
seq.start(0)?;
ctx.transport().set_bpm(124)?;
ctx.transport().set_loop(true);
ctx.transport().set_loop_points("0:0:0", "1:0:0")?;
ctx.transport().start()?;
```

Run: `cargo run --example bassline`

## Sampler piano

```rust
use drywet::{Context, ContextConfig, Part, PipeWireSink, Sampler};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let piano = Sampler::from_directory("samples/piano", &ctx)?;

let part = Part::new(
    |time, note| piano.trigger_attack_release(note, "4n", Some(time)),
    [
        ("0:0:0", "C4"),
        ("0:1:0", "E4"),
        ("0:2:0", "G4"),
        ("0:3:0", "B4"),
        ("1:0:0", "C5"),
    ],
);
part.start(0)?;
ctx.transport().set_bpm(90)?;
ctx.transport().start()?;
// missing pitches (D4, F4, ...) are pitch-shifted from the nearest WAV
```

Run: `cargo run --example piano` (needs `samples/piano/*.wav` next to the process)

## Live notes over a loop

The Sequence keeps running. UI keys mix at the write cursor while playback continues.

```rust
use drywet::{Context, ContextConfig, Drum, Event::*, PipeWireSink, Sequence, Synth};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let synth = Synth::new(&ctx);
let drum = Drum::new(&ctx);

let groove = Sequence::new(
    |time, voice| drum.trigger_attack_release(voice, "16n", Some(time)),
    [
        Note("kick"),
        Rest,
        Note("hat"),
        Rest,
        Note("snare"),
        Rest,
        Note("hat"),
        Rest,
    ],
    "8n",
);
groove.start(0)?;
ctx.transport().set_loop(true);
ctx.transport().set_loop_points(0, "1m")?;
ctx.transport().start()?;

fn on_key(synth: &Synth, note: &str) -> drywet::Result<()> {
    synth.trigger_attack_release(note, "8n", None) // no time → now
}

on_key(&synth, "A4")?;
```

Run: `cargo run --example jam`

## Offline CI test

```rust
use drywet::{Context, ContextConfig, Sequence, Synth};

#[test]
fn quarter_notes_fill_a_bar() {
    let ctx = Context::new(ContextConfig::default());
    let synth = Synth::new(&ctx);
    let seq = Sequence::new(
        |time, note| synth.trigger_attack_release(note, "8n", Some(time)),
        ["C4", "E4", "G4", "C5"],
        "4n",
    );
    seq.start(0).unwrap();
    ctx.transport().set_bpm(120).unwrap();
    let pcm = ctx.transport().render("1m").unwrap();
    let expected = (ctx.to_seconds("1m").unwrap() * ctx.sample_rate() as f64) as usize;
    assert_eq!(pcm.len(), expected);
    assert!(pcm.iter().any(|s| s.abs() > 0.0));
}
```

Run the sketch: `cargo run --example render`  
Run the suite: `cargo test`

## Engine from a host

```text
{"cmd": "warmup", "instrument": "drum"}
{"cmd": "start", "bpm": 100, "loop": true, "schedule": {"type": "loop", "interval": "4n", "note": "kick", "duration": "16n"}}
{"cmd": "play-midi", "note": "hat", "duration": "32n"}
{"cmd": "stop"}
{"cmd": "shutdown"}
```

```text
cargo run --example engine_session
```

More protocol detail: [Stdio engine](engine.md).
