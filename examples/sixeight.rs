//! 6/8 grid: twelve sixteenth steps (`numerator * 16 / denominator`).
//!
//! ```text
//! cargo run --example sixeight
//! ```

mod support;

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use drywet::{Drum, Sequence};

fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Rc::new(support::device_context()?);
    let drum = Rc::new(RefCell::new(Drum::new(ctx.as_ref())));

    ctx.transport().set_time_signature((6, 8))?;
    ctx.transport().set_bpm(96.0)?;
    let (num, den) = ctx.transport().time_signature();
    let steps = num * 16 / den;
    let kick = [1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0];

    let ctx_cb = Rc::clone(&ctx);
    let drum_cb = Rc::clone(&drum);
    let mut seq = Sequence::new(
        move |time, step| {
            let step: usize = step
                .expect("a Sequence callback only runs for values")
                .parse()
                .expect("step index");
            if kick[step] == 1 {
                drum_cb
                    .borrow_mut()
                    .trigger_attack_release(ctx_cb.as_ref(), "kick", "16n", Some(time.into()))
                    .expect("kick hit");
            }
        },
        (0..steps).map(|step| step.to_string()),
        "16n",
    );
    seq.start(&mut ctx.transport(), 0)?;
    ctx.transport().set_loop(true);
    ctx.transport().set_loop_points(0, "1m")?;
    support::play(ctx.as_ref(), "2m")?;
    Ok(())
}
