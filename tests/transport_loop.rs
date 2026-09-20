use std::cell::RefCell;
use std::rc::Rc;

use drywet::time::TimeError;
use drywet::transport::TransportError;
use drywet::Context;

fn approx_eq(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() < 1e-9,
        "expected {expected}, got {got}"
    );
}

fn record(
    hits: &Rc<RefCell<Vec<(&'static str, f64)>>>,
    name: &'static str,
) -> impl Fn(f64) + 'static {
    let hits = Rc::clone(hits);
    move |time| hits.borrow_mut().push((name, time))
}

#[test]
fn transport_loop_points_and_events() {
    let ctx = Context::new();
    let mut t = ctx.transport();
    t.set_bpm(120.0).unwrap();

    let hits = Rc::new(RefCell::new(Vec::new()));
    t.on("start", record(&hits, "start")).unwrap();
    t.on("stop", record(&hits, "stop")).unwrap();
    t.on("pause", record(&hits, "pause")).unwrap();

    t.set_loop(true);
    t.set_loop_points("0:0:0", "1:0:0").unwrap();
    assert!(t.r#loop());
    approx_eq(t.loop_start(), 0.0);
    approx_eq(t.loop_end(), 2.0);

    t.start();
    t.pause();
    t.stop();

    let names: Vec<&str> = hits.borrow().iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["start", "pause", "stop"]);
}

#[test]
fn transport_loop_set_loop_points_validates() {
    let ctx = Context::new();
    assert_eq!(
        ctx.transport().set_loop_points("1m", "0:0:0"),
        Err(TransportError::InvalidLoopPoints)
    );
}

#[test]
fn transport_loop_unknown_event_is_err() {
    let ctx = Context::new();
    assert_eq!(
        ctx.transport().on("nope", |_| {}),
        Err(TransportError::UnknownEvent("nope".into()))
    );
}

#[test]
fn transport_loop_set_loop_points_invalid_time() {
    let ctx = Context::new();
    assert_eq!(
        ctx.transport().set_loop_points("not-a-time", "1m"),
        Err(TransportError::Time(TimeError::InvalidTime(
            "not-a-time".into()
        )))
    );
}

#[test]
fn transport_loop_set_seconds_wraps_when_enabled() {
    let ctx = Context::new();
    let mut t = ctx.transport();
    t.set_bpm(120.0).unwrap();
    t.set_loop_points("0:0:0", "1:0:0").unwrap();
    t.set_seconds(2.5);
    approx_eq(t.seconds(), 2.5);

    t.set_loop(true);
    approx_eq(t.seconds(), 0.5);
    t.set_seconds(2.0);
    approx_eq(t.seconds(), 0.0);
    t.set_seconds(-0.5);
    approx_eq(t.seconds(), 1.5);

    t.set_loop(false);
    t.set_seconds(3.0);
    approx_eq(t.seconds(), 3.0);
}
