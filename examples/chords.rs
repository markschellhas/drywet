//! Chord widget: each pad mixes a triad onto the current stream.
//!
//! ```text
//! cargo run --example chords
//! ```

mod support;

use std::error::Error;

use drywet::instrument::InstrumentError;
use drywet::{Context, Sink, Synth};

fn on_pad<S: Sink>(ctx: &Context<S>, synth: &mut Synth, name: &str) -> Result<(), InstrumentError> {
    let notes: &[&str] = match name {
        "C" => &["C4", "E4", "G4"],
        "Am" => &["A3", "C4", "E4"],
        "F" => &["F3", "A3", "C4"],
        "G" => &["G3", "B3", "D4"],
        _ => return Ok(()),
    };
    for note in notes {
        synth.trigger_attack_release(ctx, *note, "2n", None)?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let ctx = support::device_context()?;
    let mut synth = Synth::new(&ctx);
    on_pad(&ctx, &mut synth, "C")?;
    support::play(&ctx, "1m")?;
    Ok(())
}
