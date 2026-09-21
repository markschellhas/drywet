//! Offline render through BufferSink. Writes `phrase.pcm` in the cwd.
//!
//! ```text
//! cargo run --example render
//! ```

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use drywet::{Context, Sequence, Synth};

fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Rc::new(Context::new());
    let synth = Rc::new(RefCell::new(Synth::new(ctx.as_ref())));

    ctx.transport().set_bpm(120.0)?;
    let ctx_cb = Rc::clone(&ctx);
    let synth_cb = Rc::clone(&synth);
    let mut seq = Sequence::new(
        move |time, note| {
            synth_cb
                .borrow_mut()
                .trigger_attack_release(
                    ctx_cb.as_ref(),
                    note.expect("a Sequence callback only runs for notes"),
                    "8n",
                    Some(time.into()),
                )
                .expect("rendered note");
        },
        ["C4", "E4", "G4", "C5"],
        "4n",
    );
    seq.start(&mut ctx.transport(), 0)?;
    let pcm = ctx.render("1m")?;
    let expected = (ctx.to_seconds("1m")? * f64::from(ctx.sample_rate())) as usize;
    assert_eq!(pcm.len(), expected);
    assert!(pcm.iter().any(|sample| sample.abs() > 0.0));

    std::fs::write("phrase.pcm", ctx.sink().to_pcm_s16le())?;
    println!(
        "wrote phrase.pcm ({} frames, {} Hz)",
        pcm.len(),
        ctx.sample_rate()
    );
    Ok(())
}
