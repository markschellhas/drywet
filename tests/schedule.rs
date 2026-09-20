use std::cell::RefCell;
use std::rc::Rc;

use drywet::limits::MAX_SCHEDULE_SECONDS;
use drywet::sink::Sink;
use drywet::time::TimeError;
use drywet::transport::{TransportError, TransportState};
use drywet::Context;

fn approx_eq(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() < 1e-9,
        "expected {expected}, got {got}"
    );
}

#[derive(Default)]
struct ProbeSink {
    stop_calls: usize,
    close_calls: usize,
}

impl Sink for ProbeSink {
    fn mix(&mut self, _frames: &[f32], _at_sample: Option<usize>) {}

    fn write(&mut self, _frames: &[f32]) {}

    fn stop(&mut self) {
        self.stop_calls += 1;
    }

    fn close(&mut self) {
        self.close_calls += 1;
    }

    fn latency_ms(&self) -> u32 {
        0
    }

    fn write_cursor(&self) -> usize {
        0
    }

    fn accepted(&self) -> bool {
        false
    }
}

/// Heritage test 1: schedule / once / repeat, then fire_until(1.0).
#[test]
fn schedule_once_repeat_fires_until() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.set_bpm(120.0).unwrap();

    let hits = Rc::new(RefCell::new(Vec::<(&'static str, f64)>::new()));
    let a = Rc::clone(&hits);
    t.schedule(move |time| a.borrow_mut().push(("a", time)), "4n")
        .unwrap();
    let b = Rc::clone(&hits);
    t.schedule_once(move |time| b.borrow_mut().push(("b", time)), 1.0)
        .unwrap();
    let r = Rc::clone(&hits);
    t.schedule_repeat(move |time| r.borrow_mut().push(("r", time)), 0.5, 0)
        .unwrap();

    t.fire_until(1.0).unwrap();

    let hits = hits.borrow();
    assert!(hits
        .iter()
        .any(|&(name, time)| name == "a" && (time - 0.5).abs() < 1e-9));
    assert!(hits
        .iter()
        .any(|&(name, time)| name == "b" && (time - 1.0).abs() < 1e-9));
    assert_eq!(hits.iter().filter(|(name, _)| *name == "r").count(), 3);
    approx_eq(t.seconds(), 1.0);
}

/// Heritage test 2: cancel keeps events whose time is strictly before `after`.
#[test]
fn schedule_cancel_keeps_earlier_events() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.clear();

    let hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let late = Rc::clone(&hits);
    t.schedule(move |_| late.borrow_mut().push("late"), 0.9)
        .unwrap();
    let early = Rc::clone(&hits);
    t.schedule(move |_| early.borrow_mut().push("early"), 0.1)
        .unwrap();
    t.cancel(0.5).unwrap();
    t.fire_until(1.0).unwrap();
    assert_eq!(*hits.borrow(), ["early"]);
}

/// Heritage test 3: clear drops pending events.
#[test]
fn schedule_clear_drops_pending() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    let hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let x = Rc::clone(&hits);
    t.schedule(move |_| x.borrow_mut().push("x"), 0.1).unwrap();
    t.clear();
    t.fire_until(1.0).unwrap();
    assert!(hits.borrow().is_empty());
}

/// Heritage test 4: times above MAX_SCHEDULE_SECONDS are Err.
#[test]
fn schedule_rejects_too_long() {
    let mut ctx = Context::new();
    assert_eq!(
        ctx.transport().schedule(|_| {}, MAX_SCHEDULE_SECONDS + 1.0),
        Err(TransportError::ScheduleTimeOutOfRange(
            MAX_SCHEDULE_SECONDS + 1.0
        ))
    );
}

#[test]
fn schedule_rejects_negative_time() {
    let mut ctx = Context::new();
    assert_eq!(
        ctx.transport().schedule(|_| {}, -0.1),
        Err(TransportError::ScheduleTimeOutOfRange(-0.1))
    );
}

#[test]
fn schedule_accepts_max_seconds() {
    let mut ctx = Context::new();
    let id = ctx
        .transport()
        .schedule(|_| {}, MAX_SCHEDULE_SECONDS)
        .unwrap();
    assert_eq!(id, 1);
}

#[test]
fn schedule_invalid_time_is_time_error() {
    let mut ctx = Context::new();
    assert_eq!(
        ctx.transport().schedule(|_| {}, "not-a-time"),
        Err(TransportError::Time(TimeError::InvalidTime(
            "not-a-time".into()
        )))
    );
}

#[test]
fn schedule_repeat_zero_interval_is_err_on_fire() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.schedule_repeat(|_| {}, 0.0, 0).unwrap();
    assert_eq!(
        t.fire_until(1.0),
        Err(TransportError::InvalidRepeatInterval)
    );
}

#[test]
fn schedule_dispose_clears_stops_and_closes() {
    let mut ctx = Context::with(44100, 1, ProbeSink::default());
    {
        let mut t = ctx.transport();
        t.start();
        let hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));
        let x = Rc::clone(&hits);
        t.schedule(move |_| x.borrow_mut().push("x"), 0.1).unwrap();
        t.dispose();
        assert_eq!(t.state(), TransportState::Stopped);
        assert_eq!(t.seconds(), 0.0);
        t.fire_until(1.0).unwrap();
        assert!(hits.borrow().is_empty());
    }
    assert_eq!(ctx.sink().stop_calls, 1);
    assert_eq!(ctx.sink().close_calls, 1);
}

#[test]
fn schedule_ids_increment_and_once_matches_schedule() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    let a = t.schedule(|_| {}, 0.1).unwrap();
    let b = t.schedule_once(|_| {}, 0.2).unwrap();
    let c = t.schedule_repeat(|_| {}, 0.5, 0).unwrap();
    assert_eq!((a, b, c), (1, 2, 3));
}

#[test]
fn schedule_does_not_refire_after_fire_until() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    let hits = Rc::new(RefCell::new(0usize));
    let count = Rc::clone(&hits);
    t.schedule(move |_| *count.borrow_mut() += 1, 0.25).unwrap();
    t.fire_until(1.0).unwrap();
    t.fire_until(1.0).unwrap();
    assert_eq!(*hits.borrow(), 1);
}
