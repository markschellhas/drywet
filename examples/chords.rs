//! Chord widget: each pad mixes a triad onto the current stream.
//!
//! ```text
//! cargo run --example chords
//! ```

use drywet::{Context, ContextConfig, PipeWireSink, Synth};

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

fn main() -> drywet::Result<()> {
    let ctx = Context::new(ContextConfig {
        sink: Some(Box::new(PipeWireSink::new()?)),
        ..Default::default()
    });
    let synth = Synth::new(&ctx);
    ctx.transport().start()?;
    on_pad(&synth, "C")?;
    Ok(())
}
