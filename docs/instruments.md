# Instruments

<!-- kicker: Voices -->

v1 includes a Sampler that maps WAV files to notes, a small additive Synth, and kick / snare / hi-hat transients. Velocity is ignored. There is no SoundFont dependency.

## Shared trigger API

Tone names in snake_case. `trigger_attack` starts a live voice at the write cursor (or `time`) and keeps sounding until `trigger_release` / `release_all`. Release is a short envelope, not a hard cut. `trigger_attack_release` is the scheduled one-shot. Duration of a held note is the hold, not a prerendered length.

| Method | Tone analogue |
| --- | --- |
| `trigger_attack(note, time)` | `triggerAttack` |
| `trigger_release(note, time)` | `triggerRelease` |
| `trigger_attack_release(note, duration, time)` | `triggerAttackRelease` |

`time` is `Option<impl Into<Time>>`. `None` means now (the write cursor).

```rust
synth.trigger_attack("C4", None)?;                       // now
synth.trigger_release("C4", Some("+4n"))?;
synth.trigger_attack_release("E4", "8n", Some(time))?;   // scheduled
```

Instruments do not `connect` anywhere. The Context sink is the destination.

## Synth

A small additive voice (harmonics plus attack/release) for cases where no sample bank is loaded. It covers the Tone Synth role, not OmniOscillator internals.

```rust
use drywet::{Context, ContextConfig, PipeWireSink, Synth};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let synth = Synth::new(&ctx);
ctx.transport().start()?;

fn play_chord(synth: &Synth, notes: &[&str], duration: &str) -> drywet::Result<()> {
    for note in notes {
        synth.trigger_attack_release(*note, duration, None)?;
    }
    Ok(())
}

play_chord(&synth, &["C4", "E4", "G4", "B4"], "2n")?;
```

## Sampler

Follows Tone Sampler: note or MIDI → WAV map, pitch-shift missing pitches, polyphonic `trigger_attack_release`. WAV only in v1. Files are resampled on load if their rate differs from the Context.

### Map files by note name

```rust
let piano = Sampler::new(
    [
        ("C4".into(), "samples/C4.wav".into()),
        ("G4".into(), "samples/G4.wav".into()),
        ("72".into(), "samples/C5.wav".into()), // MIDI integer keys are fine
    ],
    &ctx,
)?;

piano.trigger_attack_release("D4", "4n", Some(time))?;
// D4 has no file → nearest sample is pitch-shifted (Tone urls + automatic repitch)
```

### Load a folder of `C4.wav`-style files

```rust
let piano = Sampler::from_directory("samples/", &ctx)?;
piano.add("F#3", "samples/extra/Fs3.wav")?; // Tone Sampler.add
```

### Sustain loops and release

```rust
let organ = Sampler::from_directory("organ/", &ctx)?.with_loop(true);
organ.trigger_attack("C3", None)?;
// ... later, all keys up:
organ.release_all(None)?;
organ.release_all(Some(time))?; // scheduled, Tone releaseAll
```

Polyphony is capped and documented. Exceeding the cap returns `Err` rather than hanging the sink. Optional looping is for organ-like sustains, not clip warping.

> [!NOTE]
> **Sample banks stay with the caller.** Keep WAVs in the consumer repo. A widget that vendors `drywet-engine` should also vendor its own `samples/`.

## Drum

Kick, snare, and hi-hat transients for beat widgets (the Membrane/Noise role, not those Tone classes). Patterns are on/off steps; v1 has no velocity. Trigger by voice name so a fourth lane does not require a new class. Unknown names return `Err`.

```rust
use drywet::{Context, ContextConfig, Drum, PipeWireSink, Sequence};

let ctx = Context::new(ContextConfig {
    sink: Some(Box::new(PipeWireSink::new()?)),
    ..Default::default()
});
let drum = Drum::new(&ctx);

let kick  = [1,0,0,0, 1,0,0,0, 1,0,0,0, 1,0,0,0];
let snare = [0,0,0,0, 1,0,0,0, 0,0,0,0, 1,0,0,0];
let hat   = [1,0,1,0, 1,0,1,0, 1,0,1,0, 1,0,1,0];

let seq = Sequence::new(
    |time, index| {
        let i = index as usize;
        if kick[i]  == 1 { drum.trigger_attack_release("kick", "16n", Some(time))?; }
        if snare[i] == 1 { drum.trigger_attack_release("snare", "16n", Some(time))?; }
        if hat[i]   == 1 { drum.trigger_attack_release("hat", "16n", Some(time))?; }
        Ok(())
    },
    0..16,
    "16n",
);
seq.start(0)?;
ctx.transport().set_bpm(96)?;
ctx.transport().set_loop(true);
ctx.transport().set_loop_points(0, "1m")?;
ctx.transport().start()?;
```

`Drum::trigger("kick", time)` is the short form. At 3/4 or 6/8, use 12 steps. See [beat grids](time.md#beat-grids-from-time-signature).

## Live vs scheduled

| Call | When it sounds |
| --- | --- |
| `trigger_attack("C4", None)` then `trigger_release("C4", None)` | Key down / hold / key up on the live voice mixer |
| `trigger_attack_release("C4", "8n", None)` | Immediately, mixed at the write cursor (one-shot) |
| `trigger_attack_release("C4", "8n", Some(time))` | At the sample-accurate event time from a callback |

Both mix onto the same playing stream. There is no `Instrument.sync()` / `unsync()` in v1: Sequence / Part / Loop are already on the Transport, and live notes mix immediately.
