//! Live notes mixed onto a running drum loop.
//!
//! ```text
//! cargo run --example jam
//! ```

mod support;

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use drywet::event::SequenceEvent::{Rest, Value};
use drywet::instrument::InstrumentError;
use drywet::{Context, Drum, Sequence, Sink, Synth};

fn on_key<S: Sink>(ctx: &Context<S>, synth: &mut Synth, note: &str) -> Result<(), InstrumentError> {
    synth.trigger_attack_release(ctx, note, "8n", None)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Rc::new(support::device_context()?);
    let synth = Rc::new(RefCell::new(Synth::new(ctx.as_ref())));
    let drum = Rc::new(RefCell::new(Drum::new(ctx.as_ref())));

    let ctx_cb = Rc::clone(&ctx);
    let drum_cb = Rc::clone(&drum);
    let mut groove = Sequence::new(
        move |time, voice| {
            drum_cb
                .borrow_mut()
                .trigger_attack_release(
                    ctx_cb.as_ref(),
                    voice.expect("a Sequence callback only runs for notes"),
                    "16n",
                    Some(time.into()),
                )
                .expect("groove hit");
        },
        [
            Value("kick".into()),
            Rest,
            Value("hat".into()),
            Rest,
            Value("snare".into()),
            Rest,
            Value("hat".into()),
            Rest,
        ],
        "8n",
    );
    groove.start(&mut ctx.transport(), 0)?;
    ctx.transport().set_loop(true);
    ctx.transport().set_loop_points(0, "1m")?;
    on_key(ctx.as_ref(), &mut synth.borrow_mut(), "A4")?;
    support::play(ctx.as_ref(), "2m")?;
    Ok(())
}
