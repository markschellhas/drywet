//! Bassline Sequence with rests and nested subdivision.
//!
//! ```text
//! cargo run --example bassline
//! ```

use drywet::{Context, ContextConfig, Event::*, PipeWireSink, Sequence, Synth};

fn main() -> drywet::Result<()> {
    let ctx = Context::new(ContextConfig {
        sink: Some(Box::new(PipeWireSink::new()?)),
        ..Default::default()
    });
    let synth = Synth::new(&ctx);

    let seq = Sequence::new(
        |time, note| synth.trigger_attack_release(note, "16n", Some(time)),
        [
            Note("C2"),
            Rest,
            Group(vec![Note("C2"), Note("G2")]),
            Note("A#1"),
            Note("C2"),
            Rest,
            Note("D2"),
            Note("G2"),
        ],
        "8n",
    );
    seq.start(0)?;
    ctx.transport().set_bpm(124)?;
    ctx.transport().set_loop(true);
    ctx.transport().set_loop_points("0:0:0", "1:0:0")?;
    ctx.transport().start()?;
    Ok(())
}
