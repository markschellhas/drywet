# Instruments

<!-- kicker: Sound sources -->

All instruments are created against a context and receive that context when scheduling a voice.

## Synth

```rust
let mut synth = drywet::Synth::new(&ctx);
synth.trigger_attack_release(&ctx, "C4", "8n", None)?;
synth.trigger_attack_release(&ctx, 60, "0.2s", Some(0.5.into()))?;
synth.trigger_release(&ctx, "C4")?;
synth.release_all(&ctx)?;
```

The synth accepts note names or MIDI note numbers. The optional attack time is in seconds when supplied by a sequence callback.

## Drum

```rust
let mut drum = drywet::Drum::new(&ctx);
drum.trigger_attack_release(&ctx, "kick", "16n", None)?;
drum.trigger_attack_release(&ctx, "snare", "16n", Some(0.25.into()))?;
```

Built-in drum voices include `kick`, `snare`, `hihat`, `tom`, and `clap`. Drum durations are accepted for a common scheduling interface; the one-shot voice itself determines its tail.

## Sampler

```rust
let mut piano = drywet::Sampler::from_directory(&ctx, "samples/piano")?;
piano.trigger_attack_release(&ctx, "C4", "4n", None)?;
```

`from_directory` loads audio files keyed by filename. The directory must exist and contain supported sample files. Keep the sampler and context alive for the full playback/render operation.

## Scheduling from callbacks

Instrument methods return `Result<&mut Instrument, InstrumentError>`. Sequence callbacks return unit, so handle or deliberately ignore the result inside the callback:

```rust
move |time, note| {
    if let Some(note) = note {
        let _ = synth.trigger_attack_release(&ctx, note, "8n", Some(time.into()));
    }
}
```
