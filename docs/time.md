# Musical time

<!-- kicker: Time strings -->

drywet uses Tone’s time strings against the Context Transport: note values, dotted and triplet suffixes, measures, Transport-relative offsets, and bars:beats:sixteenths. Invalid strings return `Err`.

## Convert time

All schedule times convert to sample frames on the integer clock. Use these helpers when a host needs seconds, ticks, or Hz.

```rust
ctx.transport().set_bpm(120)?;
ctx.to_seconds("4n")?;      // 0.5 at 120 BPM
ctx.to_ticks("4n")?;        // PPQ / 4  →  48 at PPQ 192
ctx.to_seconds(1.0)?;       // floats are already seconds
ctx.to_frequency("C4")?;    // ~261.63 Hz
```

`Transport` also delegates `to_seconds`, `to_ticks`, and `to_frequency` with the current bpm, signature, now, and PPQ.

## Note values

A trailing `n` is a note value. `1m` is one measure at the current time signature. `1n` through `32n` are supported.

| String | At 4/4, 120 BPM | Meaning |
| --- | --- | --- |
| `"1m"` | 2.0 s | One bar (four quarter notes) |
| `"1n"` | 2.0 s | Whole note |
| `"2n"` | 1.0 s | Half note |
| `"4n"` | 0.5 s | Quarter note |
| `"8n"` | 0.25 s | Eighth note |
| `"16n"` | 0.125 s | Sixteenth note |
| `"32n"` | 0.0625 s | Thirty-second note |

```rust
synth.trigger_attack_release("G4", "8n", Some(time))?;
let click = Loop::new(on_click, "4n");
```

At 6/8, `"1m"` is 1.5 s (six eighth notes at 120 BPM).

## Dotted and triplets

| String | Relation |
| --- | --- |
| `"8n."` | Dotted eighth = `"8n"` × 1.5 |
| `"4n."` | Dotted quarter |
| `"8t"` | Eighth-note triplet = `"4n"` / 3 |
| `"16t"` | Sixteenth-note triplet |

```rust
let seq = Sequence::new(hit, ["C4", "E4", "G4"], "8t");
```

At 120 BPM 4/4, `"8n."` is 0.375 s and `"8t"` is `0.25 * 2/3`.

## Relative time

A leading `+` is Transport-relative: this far after the current playhead. It is useful for `schedule_once` and live pickups.

```rust
ctx.transport().schedule_once(
    |time| synth.trigger_attack_release("C5", "16n", Some(time)),
    "+4n",
)?;
```

`"+4n"` with `now = 1.0` is 1.5 s.

## Bars : beats : sixteenths

Absolute arrangement time is `"bars:beats:sixteenths"`. Bar and beat counts are 0-based in the Tone 14.7 convention used here (`"0:0:0"` is the start of the arrangement). Sixteenths run 0–3 inside a beat at 4/4.

```rust
let part = Part::new(on_note, [
    ("0:0:0", "C4"),
    ("0:1:0", "E4"),
    ("0:2:2", "G4"),
    ("1:0:0", "C5"),
]);
println!("{}", ctx.transport().position()); // e.g. "0:3:2"
```

At 120 BPM 4/4: `"0:1:0"` is 0.5 s, `"1:0:0"` is 2.0 s, `"0:0:4"` is 0.5 s.

## Frequency

Scientific pitch and MIDI integers convert to Hz for the Synth. Out-of-range values return `Err` (Hz 20–20000, MIDI 0–127). `"H4"` is invalid.

```rust
ctx.to_frequency("A4")?;    // 440.0
ctx.to_frequency(69)?;      // MIDI 69 → 440.0
synth.trigger_attack_release(60, "4n", Some(time))?; // MIDI C4
```

Note helpers: `note_to_midi("C4")` is 60, `"A4"` is 69, `"C#4"` / `"Db4"` is 61. Integers pass through.

## Beat grids from time signature

Drum machine steps are sixteenths derived from the time signature:

`steps = numerator * 16 / denominator`

| Signature | Steps per bar |
| --- | --- |
| 4/4 | 16 |
| 3/4 | 12 |
| 6/8 | 12 |

```rust
let (num, den) = ctx.transport().time_signature();
let steps = num * 16 / den;
let grid: Vec<u8> = [1, 0, 0, 0]
    .into_iter()
    .cycle()
    .take(steps as usize)
    .collect(); // on/off sixteenths; no velocity in v1
```

> [!CAUTION]
> **Language timers are not the musical clock.** `thread::sleep`, `QTimer`, and `setInterval` are not sample-accurate. Pass the callback `time` into every scheduled trigger.
