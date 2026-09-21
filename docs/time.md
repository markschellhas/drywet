# Time and musical values

<!-- kicker: Timing -->

Musical values are accepted anywhere the API uses `IntoTime`: bars/beats/ticks such as `"2:0:0"`, note lengths such as `"4n"` and `"1m"`, seconds such as `"0.5s"`, or numeric seconds.

```rust
use drywet::time::{to_seconds, to_ticks};

let seconds = to_seconds("4n", 120.0)?;
let ticks = to_ticks("2:0:0", 120.0)?;
```

The transport's PPQ and time signature determine the conversion from musical positions to ticks. BPM affects note-length conversion; changing BPM does not rewrite already-rendered PCM.

## Event timestamps

Sequence callbacks receive their timestamp as seconds:

```rust
move |time, note| {
    if let Some(note) = note {
        let _ = synth.trigger_attack_release(
            &ctx, note, "16n", Some(time.into()),
        );
    }
}
```

The optional timestamp is the event's attack time. Pass `None` to trigger immediately relative to the current transport position. Durations are still parsed independently, so `"16n"`, `"0.1s"`, and numeric values are all valid.

## Practical limits

Use finite durations for offline rendering. A live transport can run indefinitely, but the owning process must remain alive and the output sink must be started. Invalid note names, malformed positions, and non-positive BPM values return errors instead of being silently clamped.
