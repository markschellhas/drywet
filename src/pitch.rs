use std::error::Error;
use std::fmt;

use crate::limits::{MIDI_MAX, MIDI_MIN};

/// Failure converting a note name or MIDI number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PitchError {
    /// String did not match scientific pitch notation (`C4`, `C#4`, `Db4`).
    InvalidNote(String),
    /// MIDI number was outside 0–127.
    OutOfRange(i32),
}

impl fmt::Display for PitchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PitchError::InvalidNote(note) => write!(f, "invalid note: {note:?}"),
            PitchError::OutOfRange(midi) => write!(f, "MIDI must be 0–127, got {midi}"),
        }
    }
}

impl Error for PitchError {}

/// Scientific note name or integer MIDI number.
pub trait IntoNote {
    fn into_midi(self) -> Result<u8, PitchError>;
}

impl IntoNote for &str {
    fn into_midi(self) -> Result<u8, PitchError> {
        parse_scientific(self)
    }
}

impl IntoNote for String {
    fn into_midi(self) -> Result<u8, PitchError> {
        parse_scientific(&self)
    }
}

impl IntoNote for &String {
    fn into_midi(self) -> Result<u8, PitchError> {
        parse_scientific(self)
    }
}

impl IntoNote for i32 {
    fn into_midi(self) -> Result<u8, PitchError> {
        clamp_midi(self)
    }
}

/// Convert a scientific note name or MIDI number to a MIDI note (0–127).
///
/// Names follow `^([A-Ga-g])([#b]?)(-?\\d+)$`. Pitch class is
/// C=0 D=2 E=4 F=5 G=7 A=9 B=11, `#` raises a semitone, `b` lowers one.
/// MIDI is `(octave + 1) * 12 + pc`. Integers pass through if in range.
pub fn note_to_midi(note: impl IntoNote) -> Result<u8, PitchError> {
    note.into_midi()
}

/// Equal-tempered frequency of a note: `440 * 2^((midi - 69) / 12)`.
pub fn midi_to_hz(midi: impl IntoNote) -> Result<f64, PitchError> {
    let midi = midi.into_midi()?;
    Ok(440.0 * 2.0_f64.powf((f64::from(midi) - 69.0) / 12.0))
}

fn clamp_midi(midi: i32) -> Result<u8, PitchError> {
    if midi < i32::from(MIDI_MIN) || midi > i32::from(MIDI_MAX) {
        Err(PitchError::OutOfRange(midi))
    } else {
        Ok(midi as u8)
    }
}

fn parse_scientific(note: &str) -> Result<u8, PitchError> {
    let trimmed = note.trim();
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() {
        return Err(PitchError::InvalidNote(note.to_string()));
    }

    let mut pc = match bytes[0] {
        b'A' | b'a' => 9,
        b'B' | b'b' => 11,
        b'C' | b'c' => 0,
        b'D' | b'd' => 2,
        b'E' | b'e' => 4,
        b'F' | b'f' => 5,
        b'G' | b'g' => 7,
        _ => return Err(PitchError::InvalidNote(note.to_string())),
    };

    let mut idx = 1;
    if let Some(&acc) = bytes.get(idx) {
        if acc == b'#' {
            pc += 1;
            idx += 1;
        } else if acc == b'b' {
            pc -= 1;
            idx += 1;
        }
    }

    let octave_bytes = &bytes[idx..];
    let octave = match parse_octave(octave_bytes) {
        Some(oct) => oct,
        None => return Err(PitchError::InvalidNote(note.to_string())),
    };

    let midi = match octave
        .checked_add(1)
        .and_then(|o| o.checked_mul(12))
        .and_then(|v| v.checked_add(pc))
    {
        Some(midi) => midi,
        None => return Err(PitchError::OutOfRange(i32::MAX)),
    };
    clamp_midi(midi)
}

fn parse_octave(bytes: &[u8]) -> Option<i32> {
    if bytes.is_empty() {
        return None;
    }
    let (digits, sign) = if bytes[0] == b'-' {
        (&bytes[1..], -1_i32)
    } else {
        (bytes, 1_i32)
    };
    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = std::str::from_utf8(digits).ok()?.parse::<i32>().ok()?;
    Some(value * sign)
}
