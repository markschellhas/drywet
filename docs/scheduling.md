# Sequence, Part, Loop

<!-- kicker: Scheduling -->

These objects attach to the Transport and make no sound of their own. Callbacks receive a sample-accurate `time` plus a value, and should pass that time into instrument triggers.

## The time rule

```rust
// correct
let seq = Sequence::new(
    |time, note| synth.trigger_attack_release(note, "8n", Some(time)),
    ["C4", "G3", "A3", "F3"],
    "4n",
);
```

```rust
// incorrect — missing time argument
let seq = Sequence::new(
    |time, note| synth.trigger_attack_release(note, "8n", None),
    ["C4", "E4"],
    "4n",
);
```

- Signature is `(time, value)` as in Tone.
- Callbacks must not block and must not write PCM.
- They run on the scheduler thread, not the audio write thread.
- Nothing sounds until `transport.start()`.

## Sequence

`Sequence::new(callback, events, subdivision)` spaces events evenly on the Transport. Nested lists subdivide, matching Tone Sequence.

```rust
let seq = Sequence::new(on_note, ["C4", "D4", "E4", "G4"], "8n");
seq.start(0)?;
```

```rust
// each bar is a quarter; the second event is two eighths
use drywet::Event::{Group, Note, Rest};

let seq = Sequence::new(
    on_note,
    [Note("C4"), Group(vec![Note("E4"), Note("G4")]), Note("B3"), Rest],
    "4n",
);
seq.start(0)?;
```

`Rest` is a rest. Nested groups split that step equally. Subdivision is the duration of each top-level event.

```rust
let bass = Sequence::new(
    |time, note| bass_synth.trigger_attack_release(note, "8n", Some(time)),
    [36, 0 /* rest */, 36, 43, 36, 0, 38, 43],
    "8n",
);
bass.start("1m")?; // wait one bar, then run
```

MIDI integers are notes. `0` as a rest only applies when you use the `Event` enum — prefer `Rest` so MIDI note 0 (C-1) stays a pitch.

## Part

`Part::new(callback, events)` is a list of `(time, value)` pairs: a scored fragment, not a regular grid.

```rust
let part = Part::new(
    |time, note| piano.trigger_attack_release(note, "4n", Some(time)),
    [
        (0.into(), "C4"),
        ("0:1:0".into(), "E4"),
        ("0:2:0".into(), "G4"),
        ("1:0:0".into(), "C5"),
    ],
);
part.start(0)?;
```

When a value is a chord, invoke the callback once per item or trigger each note in the callback — keep it non-blocking either way.

```rust
fn on_event(time: f64, notes: &[&str]) -> drywet::Result<()> {
    for note in notes {
        piano.trigger_attack_release(*note, "2n", Some(time))?;
    }
    Ok(())
}
```

## Loop

`Loop::new(callback, interval)` repeats a callback forever (or until stopped) at a fixed musical interval. The value argument may be unused.

```rust
let click = Loop::new(
    |time, _| drum.trigger_attack_release("hat", "32n", Some(time)),
    "4n",
);
click.start(0)?;
ctx.transport().set_bpm(80)?;
ctx.transport().start()?;
```

## Start, stop, dispose

| Method | Effect |
| --- | --- |
| `.start(offset)` | Attach to Transport at `offset` (number, note value, or bars:beats:sixteenths) |
| `.stop()` | Detach; Transport may keep running |
| `.dispose()` | Remove events and drop references |

```rust
seq.start(0)?;
part.start("1m")?;
ctx.transport().start()?;
// later:
seq.stop()?;
part.dispose();
ctx.transport().clear();
```

Low-level `transport.schedule` / `schedule_once` / `schedule_repeat` are documented on [Context & Transport](context-transport.md#low-level-schedule). Sequence / Part / Loop are the usual app API.

v1 has no Sequence humanize, probability, or Transport swing.
