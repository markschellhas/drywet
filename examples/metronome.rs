//! Practice metronome: kick on the downbeat, hat on the other quarters.
//!
//! ```text
//! cargo run --example metronome
//! ```

use drywet::{Context, ContextConfig, Drum, Loop, PipeWireSink};

fn main() -> drywet::Result<()> {
    let ctx = Context::new(ContextConfig {
        sink: Some(Box::new(PipeWireSink::new()?)),
        ..Default::default()
    });
    let drum = Drum::new(&ctx);

    let click = Loop::new(
        |time, _| {
            let voice = if ctx.transport().position().ends_with(":0:0") {
                "kick"
            } else {
                "hat"
            };
            drum.trigger_attack_release(voice, "32n", Some(time))
        },
        "4n",
    );
    click.start(0)?;
    ctx.transport().set_bpm(72)?;
    ctx.transport().start()?;
    Ok(())
}
