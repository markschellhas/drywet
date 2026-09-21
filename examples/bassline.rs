//! Bassline Sequence with rests and nested subdivision.
//!
//! ```text
//! cargo run --example bassline
//! ```

mod support;

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use drywet::event::SequenceEvent::{Group, Rest, Value};
use drywet::{Sequence, Synth};

fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Rc::new(support::device_context()?);
    let synth = Rc::new(RefCell::new(Synth::new(ctx.as_ref())));

    ctx.transport().set_bpm(124.0)?;
    let ctx_cb = Rc::clone(&ctx);
    let synth_cb = Rc::clone(&synth);
    let mut seq = Sequence::new(
        move |time, note| {
            synth_cb
                .borrow_mut()
                .trigger_attack_release(
                    ctx_cb.as_ref(),
                    note.expect("a Sequence callback only runs for notes"),
                    "16n",
                    Some(time.into()),
                )
                .expect("bassline note");
        },
        [
            Value("C2".into()),
            Rest,
            Group(vec![Value("C2".into()), Value("G2".into())]),
            Value("A#1".into()),
            Value("C2".into()),
            Rest,
            Value("D2".into()),
            Value("G2".into()),
        ],
        "8n",
    );
    seq.start(&mut ctx.transport(), 0)?;
    ctx.transport().set_loop(true);
    ctx.transport().set_loop_points("0:0:0", "1:0:0")?;
    support::play(ctx.as_ref(), "4m")?;
    Ok(())
}
