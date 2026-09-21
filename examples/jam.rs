//! Live notes mixed onto a running drum loop.
//!
//! ```text
//! cargo run --example jam
//! ```

use drywet::{Context, ContextConfig, Drum, Event::*, PipeWireSink, Sequence, Synth};

fn on_key(synth: &Synth, note: &str) -> drywet::Result<()> {
    synth.trigger_attack_release(note, "8n", None)
}

fn main() -> drywet::Result<()> {
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
    on_key(&synth, "A4")?;
    Ok(())
}
