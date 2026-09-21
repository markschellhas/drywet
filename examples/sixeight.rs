//! 6/8 grid: twelve sixteenth steps (`numerator * 16 / denominator`).
//!
//! ```text
//! cargo run --example sixeight
//! ```

use drywet::{Context, ContextConfig, Drum, PipeWireSink, Sequence};

fn main() -> drywet::Result<()> {
    let ctx = Context::new(ContextConfig {
        sink: Some(Box::new(PipeWireSink::new()?)),
        ..Default::default()
    });
    let drum = Drum::new(&ctx);

    ctx.transport().set_time_signature(6, 8)?;
    let (num, den) = ctx.transport().time_signature();
    let steps = num * 16 / den;
    let kick = [1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0];

    let seq = Sequence::new(
        |time, i| {
            if kick[i as usize] == 1 {
                drum.trigger_attack_release("kick", "16n", Some(time))?;
            }
            Ok(())
        },
        0..steps,
        "16n",
    );
    seq.start(0)?;
    ctx.transport().set_bpm(96)?;
    ctx.transport().set_loop(true);
    ctx.transport().set_loop_points(0, "1m")?;
    ctx.transport().start()?;
    Ok(())
}
