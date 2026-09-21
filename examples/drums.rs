//! Sixteen-step drum machine on a looping bar.
//!
//! ```text
//! cargo run --example drums
//! ```

use std::collections::HashMap;

use drywet::{Context, ContextConfig, Drum, PipeWireSink, Sequence};

fn main() -> drywet::Result<()> {
    let ctx = Context::new(ContextConfig {
        sink: Some(Box::new(PipeWireSink::new()?)),
        ..Default::default()
    });
    let drum = Drum::new(&ctx);

    let pattern = HashMap::from([
        ("kick", [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0]),
        ("snare", [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1]),
        ("hat", [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0]),
    ]);

    let seq = Sequence::new(
        |time, i| {
            for (voice, hits) in &pattern {
                if hits[i as usize] == 1 {
                    drum.trigger_attack_release(*voice, "16n", Some(time))?;
                }
            }
            Ok(())
        },
        0..16,
        "16n",
    );
    seq.start(0)?;
    ctx.transport().set_bpm(108)?;
    ctx.transport().set_loop(true);
    ctx.transport().set_loop_points(0, "1m")?;
    ctx.transport().start()?;
    Ok(())
}
