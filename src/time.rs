use std::error::Error;
use std::fmt;

use crate::limits::{HZ_MAX, HZ_MIN};
use crate::pitch::{midi_to_hz, PitchError};

/// Failure converting a musical time or frequency value.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeError {
    /// String did not match a note value (`4n`, `8n.`, `8t`, `1m`) or BBS.
    InvalidTime(String),
    /// Numeric frequency was outside 20–20000 Hz.
    InvalidFrequency(f64),
    /// Note-name conversion failed.
    Pitch(PitchError),
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeError::InvalidTime(value) => write!(f, "invalid time: {value:?}"),
            TimeError::InvalidFrequency(hz) => write!(f, "Hz must be 20–20000, got {hz}"),
            TimeError::Pitch(err) => write!(f, "{err}"),
        }
    }
}

impl Error for TimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TimeError::Pitch(err) => Some(err),
            TimeError::InvalidTime(_) | TimeError::InvalidFrequency(_) => None,
        }
    }
}

impl From<PitchError> for TimeError {
    fn from(err: PitchError) -> Self {
        TimeError::Pitch(err)
    }
}

/// Numeric seconds/Hz or a musical time / note-name string.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeValue {
    Numeric(f64),
    Text(String),
}

/// Seconds, Hz, or a Tone-style time / scientific note string.
pub trait IntoTime {
    fn into_time(self) -> TimeValue;
}

impl IntoTime for TimeValue {
    fn into_time(self) -> TimeValue {
        self
    }
}

impl IntoTime for &str {
    fn into_time(self) -> TimeValue {
        TimeValue::Text(self.to_string())
    }
}

impl IntoTime for String {
    fn into_time(self) -> TimeValue {
        TimeValue::Text(self)
    }
}

impl IntoTime for &String {
    fn into_time(self) -> TimeValue {
        TimeValue::Text(self.clone())
    }
}

impl IntoTime for f64 {
    fn into_time(self) -> TimeValue {
        TimeValue::Numeric(self)
    }
}

impl IntoTime for f32 {
    fn into_time(self) -> TimeValue {
        TimeValue::Numeric(f64::from(self))
    }
}

impl IntoTime for i32 {
    fn into_time(self) -> TimeValue {
        TimeValue::Numeric(f64::from(self))
    }
}

impl IntoTime for u32 {
    fn into_time(self) -> TimeValue {
        TimeValue::Numeric(f64::from(self))
    }
}

impl IntoTime for i64 {
    fn into_time(self) -> TimeValue {
        TimeValue::Numeric(self as f64)
    }
}

/// Convert a note value, BBS string, or raw seconds to seconds.
///
/// Note values match `^(\\+)?(\\d+)([nmt])([.])?$`. BBS matches
/// `^(\\d+):(\\d+):(\\d+(?:\\.\\d+)?)$`. A leading `+` adds `now`.
/// Integers and floats pass through as seconds. `ppq` is accepted for
/// Transport parity and unused here.
///
/// Does not validate `bpm` or the time-signature denominator (same as
/// drywet-py); Transport will.
pub fn to_seconds(
    value: impl IntoTime,
    bpm: f64,
    time_signature: (u32, u32),
    now: f64,
    _ppq: u32,
) -> Result<f64, TimeError> {
    match value.into_time() {
        TimeValue::Numeric(seconds) => Ok(seconds),
        TimeValue::Text(text) => parse_seconds(&text, bpm, time_signature, now),
    }
}

/// Convert a time value to pulses at `ppq` ticks per quarter note.
///
/// `round(seconds * (bpm / 60) * ppq)`.
/// Does not validate `bpm` or the time-signature denominator (same as
/// drywet-py / `to_seconds`); Transport will.
pub fn to_ticks(
    value: impl IntoTime,
    bpm: f64,
    time_signature: (u32, u32),
    now: f64,
    ppq: u32,
) -> Result<i64, TimeError> {
    let seconds = to_seconds(value, bpm, time_signature, now, ppq)?;
    let ticks_per_second = (bpm / 60.0) * f64::from(ppq);
    Ok((seconds * ticks_per_second).round() as i64)
}

/// Convert a scientific note name to Hz, or pass a numeric Hz through.
///
/// Numeric Hz must be in `HZ_MIN`–`HZ_MAX` (20–20000). Note names only
/// need valid MIDI — `C0` (~16.35 Hz) is OK. Clock arguments are unused.
pub fn to_frequency(
    value: impl IntoTime,
    _bpm: f64,
    _time_signature: (u32, u32),
    _now: f64,
    _ppq: u32,
) -> Result<f64, TimeError> {
    match value.into_time() {
        TimeValue::Numeric(hz) => {
            if !(HZ_MIN..=HZ_MAX).contains(&hz) {
                Err(TimeError::InvalidFrequency(hz))
            } else {
                Ok(hz)
            }
        }
        TimeValue::Text(text) => Ok(midi_to_hz(text.as_str())?),
    }
}

fn parse_seconds(
    value: &str,
    bpm: f64,
    time_signature: (u32, u32),
    now: f64,
) -> Result<f64, TimeError> {
    let text = value.trim();
    let (num, den) = time_signature;
    let quarter = 60.0 / bpm;
    let beat = quarter * (4.0 / f64::from(den));
    let bar = f64::from(num) * beat;
    let sixteenth = quarter / 4.0;

    if let Some((bars, beats, sixteenths)) = parse_bbs(text) {
        return Ok(f64::from(bars) * bar + f64::from(beats) * beat + sixteenths * sixteenth);
    }

    let (relative, count, unit, dotted) =
        parse_note(text).ok_or_else(|| TimeError::InvalidTime(value.to_string()))?;

    let seconds = match unit {
        b'm' => f64::from(count) * bar,
        b't' => {
            if count == 0 {
                return Err(TimeError::InvalidTime(value.to_string()));
            }
            (4.0 / f64::from(count)) * quarter * (2.0 / 3.0)
        }
        _ => {
            if count == 0 {
                return Err(TimeError::InvalidTime(value.to_string()));
            }
            let mut seconds = (4.0 / f64::from(count)) * quarter;
            if dotted {
                seconds *= 1.5;
            }
            seconds
        }
    };

    if relative {
        Ok(seconds + now)
    } else {
        Ok(seconds)
    }
}

fn parse_bbs(text: &str) -> Option<(u32, u32, f64)> {
    let mut parts = text.split(':');
    let bars = parse_uint(parts.next()?)?;
    let beats = parse_uint(parts.next()?)?;
    let sixteenths = parse_sixteenths(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some((bars, beats, sixteenths))
}

fn parse_note(text: &str) -> Option<(bool, u32, u8, bool)> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let mut idx = 0;
    let relative = bytes[0] == b'+';
    if relative {
        idx = 1;
    }

    let digits_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == digits_start {
        return None;
    }
    let count = parse_uint(std::str::from_utf8(&bytes[digits_start..idx]).ok()?)?;

    let unit = *bytes.get(idx)?;
    if unit != b'n' && unit != b'm' && unit != b't' {
        return None;
    }
    idx += 1;

    let dotted = match bytes.get(idx) {
        Some(&b'.') => {
            idx += 1;
            true
        }
        Some(_) => return None,
        None => false,
    };
    if idx != bytes.len() {
        return None;
    }
    Some((relative, count, unit, dotted))
}

fn parse_uint(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn parse_sixteenths(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    if let Some(dot) = bytes.iter().position(|&b| b == b'.') {
        let whole = &bytes[..dot];
        let frac = &bytes[dot + 1..];
        if whole.is_empty()
            || frac.is_empty()
            || !whole.iter().all(|b| b.is_ascii_digit())
            || !frac.iter().all(|b| b.is_ascii_digit())
        {
            return None;
        }
    } else if !bytes.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}
