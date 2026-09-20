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

/// Heritage test 1: Sequence + Synth `render("1m")` hits C4 E4 G4 B4,
/// max abs PCM > 0.01, sink frames ≥ 2.0 × sample_rate.
#[test]
fn render_sequence_synth_one_measure() {
    let mut ctx = Context::new();
    ctx.transport().set_bpm(120.0).unwrap();
    let mut synth = Synth::new(&ctx);

    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let collected = Rc::clone(&hits);
    let mut seq = Sequence::new(
        move |_time, value| {
            collected.borrow_mut().push(value.unwrap().to_string());
        },
        ["C4", "E4", "G4", "B4"],
        "4n",
    );
    seq.start(&mut ctx.transport(), 0).unwrap();

    // Callbacks run while `render` holds TransportRef, so they cannot
    // re-borrow Context to mix. Trigger the same notes at sequence times
    // (0, 0.5, 1.0, 1.5 at 120 BPM 4/4) onto the one sink.
    for (note, time) in [("C4", 0.0), ("E4", 0.5), ("G4", 1.0), ("B4", 1.5)] {
        synth
            .trigger_attack_release(&mut ctx, note, "8n", Some(time.into()))
            .unwrap();
    }

    let pcm = ctx.transport().render("1m").unwrap();
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
    let mut ctx = Context::new();
    let mut synth = Synth::new(&ctx);
    ctx.transport().start();
    synth
        .trigger_attack_release(&mut ctx, "A4", 0.05, None)
        .unwrap();
    assert_eq!(ctx.transport().state(), TransportState::Started);
    assert!(peak(ctx.sink().frames()) > 0.01);
}

/// Heritage test 3: loop + `schedule_repeat` through `render(1.5)` fires
/// at least three hits.
#[test]
fn render_loop_repeats_hits() {
    let mut ctx = Context::new();
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

    ctx.transport().render(1.5).unwrap();
    assert!(hits.borrow().len() >= 3);
}

/// `render` starts if needed, pads the buffer, and leaves the playhead at
/// the converted duration.
#[test]
fn render_starts_pads_and_sets_playhead() {
    let mut ctx = Context::new();
    assert_eq!(ctx.transport().state(), TransportState::Stopped);

    let pcm = ctx.transport().render(0.5).unwrap();
    assert_eq!(ctx.transport().state(), TransportState::Started);
    approx_eq(ctx.transport().seconds(), 0.5);
    assert_eq!(
        pcm.len(),
        (0.5 * f64::from(ctx.sample_rate())).round() as usize
    );
    assert_eq!(ctx.sink().frames().len(), pcm.len());
}
