use std::cell::RefCell;
use std::rc::Rc;

use drywet::transport::TransportState;
use drywet::{Context, Sequence, Synth};

fn approx_eq(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() < 1e-9,
        "expected {expected}, got {got}"
    );
}

fn peak(frames: &[f32]) -> f32 {
    frames.iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()))
}

/// Heritage test 1: Sequence + Synth `render("1m")` mixes inside the
/// callback — C4 E4 G4 B4, max abs PCM > 0.01, sink frames ≥ 2.0 × sr.
#[test]
fn render_sequence_synth_one_measure() {
    let ctx = Rc::new(Context::new());
    ctx.transport().set_bpm(120.0).unwrap();
    let synth = Rc::new(RefCell::new(Synth::new(&ctx)));

    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let collected = Rc::clone(&hits);
    let synth_cb = Rc::clone(&synth);
    let ctx_cb = Rc::clone(&ctx);
    let mut seq = Sequence::new(
        move |time, value| {
            let note = value.unwrap();
            collected.borrow_mut().push(note.to_string());
            synth_cb
                .borrow_mut()
                .trigger_attack_release(&*ctx_cb, note, "8n", Some(time.into()))
                .unwrap();
        },
        ["C4", "E4", "G4", "B4"],
        "4n",
    );
    seq.start(&mut ctx.transport(), 0).unwrap();

    let pcm = ctx.render("1m").unwrap();
    assert_eq!(*hits.borrow(), ["C4", "E4", "G4", "B4"]);
    assert!(peak(&pcm) > 0.01);
    let min_frames = (2.0 * f64::from(ctx.sample_rate())) as usize;
    assert!(ctx.sink().frames().len() >= min_frames);
    assert!(pcm.len() >= min_frames);
}

/// Heritage test 2: live `trigger_attack_release(..., time=None)` mixes at
/// the write cursor without stopping the clock.
#[test]
fn render_live_note_mixes_without_stopping() {
    let ctx = Context::new();
    let mut synth = Synth::new(&ctx);
    ctx.transport().start();
    synth
        .trigger_attack_release(&ctx, "A4", 0.05, None)
        .unwrap();
    assert_eq!(ctx.transport().state(), TransportState::Started);
    assert!(peak(ctx.sink().frames()) > 0.01);
}

/// Heritage test 3: loop + `schedule_repeat` through `render(1.5)` fires
/// at least three hits.
#[test]
fn render_loop_repeats_hits() {
    let ctx = Context::new();
    {
        let mut t = ctx.transport();
        t.set_loop(true);
        t.set_loop_points(0, 0.5).unwrap();
    }

    let hits = Rc::new(RefCell::new(Vec::<f64>::new()));
    let collected = Rc::clone(&hits);
    ctx.transport()
        .schedule_repeat(move |time| collected.borrow_mut().push(time), 0.5, 0)
        .unwrap();

    ctx.render(1.5).unwrap();
    assert!(hits.borrow().len() >= 3);
}

/// `render` starts if needed, pads the buffer, and leaves the playhead at
/// the converted duration.
#[test]
fn render_starts_pads_and_sets_playhead() {
    let ctx = Context::new();
    assert_eq!(ctx.transport().state(), TransportState::Stopped);

    let pcm = ctx.render(0.5).unwrap();
    assert_eq!(ctx.transport().state(), TransportState::Started);
    approx_eq(ctx.transport().seconds(), 0.5);
    assert_eq!(
        pcm.len(),
        (0.5 * f64::from(ctx.sample_rate())).round() as usize
    );
    assert_eq!(ctx.sink().frames().len(), pcm.len());
}
