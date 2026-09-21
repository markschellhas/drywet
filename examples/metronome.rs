//! Practice metronome: kick on the downbeat, hat on the other quarters.
//!
//! ```text
//! cargo run --example metronome
//! ```

mod support;

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use drywet::{Drum, Loop};

fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Rc::new(support::device_context()?);
    let drum = Rc::new(RefCell::new(Drum::new(ctx.as_ref())));

    ctx.transport().set_bpm(72.0)?;
    let ctx_cb = Rc::clone(&ctx);
    let drum_cb = Rc::clone(&drum);
    let mut click = Loop::new(
        move |time| {
            let voice = if ctx_cb.transport().position().ends_with(":0:0") {
                "kick"
            } else {
                "hat"
            };
            drum_cb
                .borrow_mut()
                .trigger_attack_release(ctx_cb.as_ref(), voice, "32n", Some(time.into()))
                .expect("metronome hit");
        },
        "4n",
    );
    click.start(&mut ctx.transport(), 0)?;
    support::play(ctx.as_ref(), "2m")?;
    Ok(())
}
