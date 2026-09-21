//! Sixteen-step drum machine on a looping bar.
//!
//! ```text
//! cargo run --example drums
//! ```

mod support;

use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

use drywet::{Drum, Sequence};

fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Rc::new(support::device_context()?);
    let drum = Rc::new(RefCell::new(Drum::new(ctx.as_ref())));

    let pattern = HashMap::from([
        ("kick", [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0]),
        ("snare", [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1]),
        ("hat", [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0]),
    ]);

    ctx.transport().set_bpm(108.0)?;
    let ctx_cb = Rc::clone(&ctx);
    let drum_cb = Rc::clone(&drum);
    let mut seq = Sequence::new(
        move |time, step| {
            let step: usize = step
                .expect("a Sequence callback only runs for values")
                .parse()
                .expect("step index");
            for (voice, hits) in &pattern {
                if hits[step] == 1 {
                    drum_cb
                        .borrow_mut()
                        .trigger_attack_release(ctx_cb.as_ref(), voice, "16n", Some(time.into()))
                        .expect("drum hit");
                }
            }
        },
        (0..16).map(|step| step.to_string()),
        "16n",
    );
    seq.start(&mut ctx.transport(), 0)?;
    ctx.transport().set_loop(true);
    ctx.transport().set_loop_points(0, "1m")?;
    support::play(ctx.as_ref(), "2m")?;
    Ok(())
}
