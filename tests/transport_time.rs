use drywet::transport::TransportError;
use drywet::Context;

fn approx_eq(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() < 1e-9,
        "expected {expected}, got {got}"
    );
}

#[test]
fn transport_time_bpm_signature_and_conversions() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.set_bpm(120.0).unwrap();
    t.set_time_signature(4).unwrap();
    assert_eq!(t.time_signature(), (4, 4));
    t.set_time_signature((6, 8)).unwrap();
    assert_eq!(t.time_signature(), (6, 8));
    t.set_time_signature((4, 4)).unwrap();
    approx_eq(t.to_seconds("4n").unwrap(), 0.5);
    assert_eq!(t.to_ticks("4n").unwrap(), 192);
    approx_eq(t.to_frequency("A4").unwrap(), 440.0);
}

#[test]
fn transport_time_bpm_limits_and_next_start_lock() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.set_bpm(100.0).unwrap();
    assert_eq!(t.set_bpm(39.0), Err(TransportError::InvalidBpm(39.0)));
    assert_eq!(t.set_bpm(241.0), Err(TransportError::InvalidBpm(241.0)));
    t.start();
    t.set_bpm(140.0).unwrap();
    approx_eq(t.to_seconds("4n").unwrap(), 0.6);
    t.stop();
    approx_eq(t.to_seconds("4n").unwrap(), 60.0 / 140.0);
}

#[test]
fn transport_time_position_after_manual_seconds() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.set_bpm(120.0).unwrap();
    t.set_time_signature((4, 4)).unwrap();
    t.set_seconds(2.5);
    assert_eq!(t.position(), "1:1:0");
    assert_eq!(t.ticks(), 192 * 5);
}
