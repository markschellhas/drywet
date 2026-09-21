//! Sampler piano from a folder of `C4.wav`-style files.
//!
//! Missing pitches are pitch-shifted from the nearest WAV.
//! Place samples in `samples/piano/` relative to the process cwd.
//!
//! ```text
//! cargo run --example piano
//! ```

mod support;

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use drywet::{Part, Sampler};

fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Rc::new(support::device_context()?);
    let piano = Rc::new(RefCell::new(Sampler::from_directory(
        ctx.as_ref(),
        "samples/piano",
    )?));

    ctx.transport().set_bpm(90.0)?;
    let ctx_cb = Rc::clone(&ctx);
    let piano_cb = Rc::clone(&piano);
    let mut part = Part::new(
        move |time, note| {
            piano_cb
                .borrow_mut()
                .trigger_attack_release(ctx_cb.as_ref(), note, "4n", Some(time.into()))
                .expect("piano note");
        },
        [
            ("0:0:0", "C4"),
            ("0:1:0", "E4"),
            ("0:2:0", "G4"),
            ("0:3:0", "B4"),
            ("1:0:0", "C5"),
        ],
    );
    part.start(&mut ctx.transport(), 0)?;
    support::play(ctx.as_ref(), "2m")?;
    Ok(())
}
