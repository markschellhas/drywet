use std::cell::RefCell;
use std::rc::Rc;

use drywet::transport::Transport;
use drywet::{Context, Loop, Part};

fn approx_eq(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() < 1e-9,
        "expected {expected}, got {got}"
    );
}

/// Heritage test 1: Part [("0:0:0", "C4"), ("4n", "E4")]; start();
/// fire_until(1.0); notes C4 E4; second time ≈ 0.5.
#[test]
fn part_loop_part_time_value_pairs() {
    let ctx = Context::new();
    let hits = Rc::new(RefCell::new(Vec::<(f64, String)>::new()));
    let collected = Rc::clone(&hits);
    let mut part = Part::new(
        move |time, note| {
            collected.borrow_mut().push((time, note.to_string()));
        },
        [("0:0:0", "C4"), ("4n", "E4")],
    );

    part.start(&mut ctx.transport(), 0).unwrap();
    assert!(hits.borrow().is_empty());

    ctx.transport().fire_until(1.0).unwrap();

    let hits = hits.borrow();
    let notes: Vec<&str> = hits.iter().map(|(_, note)| note.as_str()).collect();
    assert_eq!(notes, ["C4", "E4"]);
    approx_eq(hits[1].0, 0.5);
}

/// Heritage test 2: Loop interval "4n"; start(0); fire_until(1.0); 3 hits;
/// stop(); fire_until(2.0); hit count unchanged.
#[test]
fn part_loop_loop_repeats_until_stop() {
    let ctx = Context::new();
    let hits = Rc::new(RefCell::new(Vec::<f64>::new()));
    let collected = Rc::clone(&hits);
    let mut looper = Loop::new(
        move |time| {
            collected.borrow_mut().push(time);
        },
        "4n",
    );

    looper.start(&mut ctx.transport(), 0).unwrap();
    ctx.transport().fire_until(1.0).unwrap();
    assert_eq!(hits.borrow().len(), 3);

    looper.stop(&mut ctx.transport());
    let n = hits.borrow().len();
    ctx.transport().fire_until(2.0).unwrap();
    assert_eq!(hits.borrow().len(), n);
}

/// Part.stop cancels scheduled hits (same attach rule as Sequence).
#[test]
fn part_loop_part_stop_cancels_scheduled_hits() {
    let ctx = Context::new();
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let collected = Rc::clone(&hits);
    let mut part = Part::new(
        move |_time, note| {
            collected.borrow_mut().push(note.to_string());
        },
        [("0:0:0", "C4"), ("4n", "E4")],
    );

    part.start(&mut ctx.transport(), 0).unwrap();
    part.stop(&mut ctx.transport());
    ctx.transport().fire_until(1.0).unwrap();
    assert!(hits.borrow().is_empty());
}

/// `cancel_ids` removes only this Part / Loop (must not wipe an unrelated
/// schedule).
#[test]
fn part_loop_stop_leaves_other_transport_events() {
    let ctx = Context::new();
    let part_hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let loop_hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let other_hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));

    let collected = Rc::clone(&part_hits);
    let mut part = Part::new(
        move |_, _| {
            collected.borrow_mut().push("part");
        },
        [("0:0:0", "C4")],
    );
    part.start(&mut ctx.transport(), 0).unwrap();

    let collected = Rc::clone(&loop_hits);
    let mut looper = Loop::new(
        move |_| {
            collected.borrow_mut().push("loop");
        },
        "4n",
    );
    looper.start(&mut ctx.transport(), 0).unwrap();

    let other = Rc::clone(&other_hits);
    ctx.transport()
        .schedule(move |_| other.borrow_mut().push("other"), 0.25)
        .unwrap();

    part.stop(&mut ctx.transport());
    looper.stop(&mut ctx.transport());
    ctx.transport().fire_until(1.0).unwrap();
    assert!(part_hits.borrow().is_empty());
    assert!(loop_hits.borrow().is_empty());
    assert_eq!(*other_hits.borrow(), ["other"]);
}

/// Part.start and Loop.start also accept a bare [`Transport`].
#[test]
fn part_loop_start_on_bare_transport() {
    let mut transport = Transport::new();
    transport.set_bpm(120.0).unwrap();

    let part_hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let collected = Rc::clone(&part_hits);
    let mut part = Part::new(
        move |_time, note| {
            collected.borrow_mut().push(note.to_string());
        },
        [("0:0:0", "C4"), ("4n", "E4")],
    );
    part.start(&mut transport, 0).unwrap();

    let loop_hits = Rc::new(RefCell::new(Vec::<f64>::new()));
    let collected = Rc::clone(&loop_hits);
    let mut looper = Loop::new(
        move |time| {
            collected.borrow_mut().push(time);
        },
        "4n",
    );
    looper.start(&mut transport, 0).unwrap();

    assert!(part_hits.borrow().is_empty());
    assert!(loop_hits.borrow().is_empty());
    transport.fire_until(1.0).unwrap();
    assert_eq!(*part_hits.borrow(), ["C4", "E4"]);
    assert_eq!(loop_hits.borrow().len(), 3);
}
