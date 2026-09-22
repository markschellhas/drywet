# Scheduling

<!-- kicker: Musical events -->

## Sequence

`Sequence` maps values to evenly spaced events. Its callback receives `(time_seconds, Option<&str>)`:

```rust
let mut seq = drywet::Sequence::new(
    move |time, note| {
        if let Some(note) = note {
            let _ = synth.trigger_attack_release(&ctx, note, "16n", Some(time.into()));
        }
    },
    vec!["C4", "E4", "G4", "B4"],
    "8n",
);
seq.start(&mut ctx.transport(), 0)?;
```

The first argument to `start` is a mutable transport reference; the second is the start offset. Registering only attaches ids. Device output is `tick` + play, or `render` + play + drain — `transport.start()` alone does not mix.

## Part

`Part` schedules explicit `(time, value)` pairs:

```rust
let mut part = drywet::Part::new(
    move |time, note| {
        let _ = synth.trigger_attack_release(&ctx, note, "8n", Some(time.into()));
    },
    vec![("0:0:0", "C4"), ("2:0:0", "G4")],
);
part.start(&mut ctx.transport(), 0)?;
```

## Loop

`Loop` repeats a callback at an interval:

```rust
let mut looped = drywet::Loop::new(
    move |_time| { let _ = drum.trigger_attack_release(&ctx, "kick", "16n", None); },
    "1m",
);
looped.start(&mut ctx.transport(), 0)?;
```

## Groups, rests, and finite playback

Nested sequence values use `drywet::event::SequenceEvent::{Value, Rest, Group}` when a phrase needs chords or silence. For offline output, schedule events and call `ctx.render("4m")?`. For a finite device clip, render, call `ctx.sink_mut().play()?`, and wait for the sink to drain. For a live arrangement, `transport.start()` then `tick` from the host frame loop (or let `drywet-engine` pump). A callback's return type is unit, so handle instrument errors inside it.
