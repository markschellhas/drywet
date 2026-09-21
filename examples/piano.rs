//! Sampler piano from a folder of `C4.wav`-style files.
//!
//! Missing pitches are pitch-shifted from the nearest WAV.
//! Place samples in `samples/piano/` relative to the process cwd.
//!
//! ```text
//! cargo run --example piano
//! ```

use drywet::{Context, ContextConfig, Part, PipeWireSink, Sampler};

fn main() -> drywet::Result<()> {
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
    Ok(())
}
