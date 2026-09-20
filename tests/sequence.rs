use std::cell::RefCell;
use std::rc::Rc;

use drywet::event::SequenceEvent;
use drywet::transport::Transport;
use drywet::{Context, Sequence};

fn approx_eq(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() < 1e-9,
        "expected {expected}, got {got}"
    );
}

/// Heritage test 1: [C4, E4, G4, B4] at 4n; start(0) is silent until
/// fire_until("1m"); notes C4 E4 G4 B4; second hit time ≈ 0.5.
#[test]
fn sequence_quarter_notes_fire_until_one_measure() {
    let ctx = Context::new();
    ctx.transport().set_bpm(120.0).unwrap();

    let hits = Rc::new(RefCell::new(Vec::<(f64, String)>::new()));
    let collected = Rc::clone(&hits);
    let mut seq = Sequence::new(
        move |time, value| {
            collected
                .borrow_mut()
                .push((time, value.unwrap().to_string()));
        },
        ["C4", "E4", "G4", "B4"],
        "4n",
    );

    seq.start(&mut ctx.transport(), 0).unwrap();
    assert!(hits.borrow().is_empty());

    ctx.transport().fire_until("1m").unwrap();

    let hits = hits.borrow();
    let notes: Vec<&str> = hits.iter().map(|(_, note)| note.as_str()).collect();
    assert_eq!(notes, ["C4", "E4", "G4", "B4"]);
    approx_eq(hits[1].0, 0.5);
}

/// Heritage test 2: nested [C4, [E4, G4], None, B4] → C4 E4 G4 B4 (None is rest).
#[test]
fn sequence_nested_list_subdivides_and_rests() {
    let ctx = Context::new();
    ctx.transport().set_bpm(120.0).unwrap();

    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let collected = Rc::clone(&hits);
    let mut seq = Sequence::new(
        move |_time, value| {
            collected.borrow_mut().push(value.unwrap().to_string());
        },
        [
            SequenceEvent::from("C4"),
            SequenceEvent::group(["E4", "G4"]),
            SequenceEvent::from(None::<&str>),
            SequenceEvent::from("B4"),
        ],
        "4n",
    );

    seq.start(&mut ctx.transport(), 0).unwrap();
    ctx.transport().fire_until("1m").unwrap();
    assert_eq!(*hits.borrow(), ["C4", "E4", "G4", "B4"]);
}

/// Heritage test 3: start then stop then fire_until → no hits.
#[test]
fn sequence_stop_cancels_scheduled_hits() {
    let ctx = Context::new();
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
    seq.stop(&mut ctx.transport());
    ctx.transport().fire_until("1m").unwrap();
    assert!(hits.borrow().is_empty());
}

/// `cancel_ids` removes only the listed events (Sequence.stop must not
/// wipe an unrelated schedule).
#[test]
fn sequence_stop_leaves_other_transport_events() {
    let ctx = Context::new();
    let seq_hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let other_hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));

    let collected = Rc::clone(&seq_hits);
    let mut seq = Sequence::new(
        move |_, _| {
            collected.borrow_mut().push("seq");
        },
        ["C4"],
        "4n",
    );
    seq.start(&mut ctx.transport(), 0).unwrap();

    let other = Rc::clone(&other_hits);
    ctx.transport()
        .schedule(move |_| other.borrow_mut().push("other"), 0.25)
        .unwrap();

    seq.stop(&mut ctx.transport());
    ctx.transport().fire_until(1.0).unwrap();
    assert!(seq_hits.borrow().is_empty());
    assert_eq!(*other_hits.borrow(), ["other"]);
}

/// Sequence.start also accepts a bare [`Transport`].
#[test]
fn sequence_start_on_bare_transport() {
    let mut transport = Transport::new();
    transport.set_bpm(120.0).unwrap();

    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let collected = Rc::clone(&hits);
    let mut seq = Sequence::new(
        move |_time, value| {
            collected.borrow_mut().push(value.unwrap().to_string());
        },
        ["C4", "E4"],
        "4n",
    );

    seq.start(&mut transport, 0).unwrap();
    assert!(hits.borrow().is_empty());
    transport.fire_until("2n").unwrap();
    assert_eq!(*hits.borrow(), ["C4", "E4"]);
}
