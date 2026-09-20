use drywet::limits::DEFAULT_PPQ;
use drywet::pitch::PitchError;
use drywet::time::{to_frequency, to_seconds, to_ticks, TimeError};

const BPM_120: f64 = 120.0;
const FOUR_FOUR: (u32, u32) = (4, 4);
const SIX_EIGHT: (u32, u32) = (6, 8);

fn approx_eq(got: f64, expected: f64) {
    assert!(
        (got - expected).abs() < 1e-9,
        "expected {expected}, got {got}"
    );
}

#[test]
fn time_note_values_at_120_4_4() {
    let now = 0.0;
    let ppq = DEFAULT_PPQ;
    approx_eq(to_seconds("4n", BPM_120, FOUR_FOUR, now, ppq).unwrap(), 0.5);
    approx_eq(
        to_seconds("8n", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        0.25,
    );
    approx_eq(
        to_seconds("16n", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        0.125,
    );
    approx_eq(to_seconds("1n", BPM_120, FOUR_FOUR, now, ppq).unwrap(), 2.0);
    approx_eq(to_seconds("1m", BPM_120, FOUR_FOUR, now, ppq).unwrap(), 2.0);
    approx_eq(
        to_seconds("8n.", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        0.375,
    );
    approx_eq(
        to_seconds("8t", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        0.25 * 2.0 / 3.0,
    );
    approx_eq(
        to_seconds(1.25, BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        1.25,
    );
}

#[test]
fn time_relative_and_bbs() {
    let now = 1.0;
    let ppq = DEFAULT_PPQ;
    approx_eq(
        to_seconds("+4n", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        1.5,
    );
    approx_eq(
        to_seconds("0:1:0", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        0.5,
    );
    approx_eq(
        to_seconds("1:0:0", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        2.0,
    );
    approx_eq(
        to_seconds("0:0:4", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        0.5,
    );
}

#[test]
fn time_6_8_measure() {
    approx_eq(
        to_seconds("1m", BPM_120, SIX_EIGHT, 0.0, DEFAULT_PPQ).unwrap(),
        1.5,
    );
}

#[test]
fn time_ticks_and_frequency() {
    let now = 0.0;
    let ppq = DEFAULT_PPQ;
    assert_eq!(to_ticks("4n", BPM_120, FOUR_FOUR, now, ppq).unwrap(), 192);
    assert_eq!(to_ticks("8n", BPM_120, FOUR_FOUR, now, ppq).unwrap(), 96);
    approx_eq(
        to_frequency("A4", BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        440.0,
    );
    approx_eq(
        to_frequency(220, BPM_120, FOUR_FOUR, now, ppq).unwrap(),
        220.0,
    );
}

#[test]
fn time_invalid_time_raises() {
    let now = 0.0;
    let ppq = DEFAULT_PPQ;
    assert_eq!(
        to_seconds("nope", BPM_120, FOUR_FOUR, now, ppq).unwrap_err(),
        TimeError::InvalidTime("nope".into())
    );
    assert_eq!(
        to_seconds("3x", BPM_120, FOUR_FOUR, now, ppq).unwrap_err(),
        TimeError::InvalidTime("3x".into())
    );
    assert_eq!(
        to_seconds("0n", BPM_120, FOUR_FOUR, now, ppq).unwrap_err(),
        TimeError::InvalidTime("0n".into())
    );
    assert_eq!(
        to_seconds("0t", BPM_120, FOUR_FOUR, now, ppq).unwrap_err(),
        TimeError::InvalidTime("0t".into())
    );
}

#[test]
fn time_frequency_errors() {
    let now = 0.0;
    let ppq = DEFAULT_PPQ;
    assert_eq!(
        to_frequency(19.0, BPM_120, FOUR_FOUR, now, ppq).unwrap_err(),
        TimeError::InvalidFrequency(19.0)
    );
    assert_eq!(
        to_frequency(20001.0, BPM_120, FOUR_FOUR, now, ppq).unwrap_err(),
        TimeError::InvalidFrequency(20001.0)
    );
    assert_eq!(
        to_frequency("H4", BPM_120, FOUR_FOUR, now, ppq).unwrap_err(),
        TimeError::Pitch(PitchError::InvalidNote("H4".into()))
    );
}
