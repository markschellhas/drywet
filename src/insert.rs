use std::error::Error;
use std::fmt;

use crate::limits::MAX_INSERTS;

/// Real-time playback insert. `process` is a mono sample stream.
///
/// Implementations must be [`Send`] + `'static`. `process` must not allocate,
/// format, perform I/O, or take a blocking lock.
pub trait Insert: Send + 'static {
    /// Process a contiguous block of mono samples in place.
    fn process(&mut self, frames: &mut [f32]);
}

/// Ordered playback inserts applied first-to-last.
pub struct InsertChain {
    inserts: Vec<Box<dyn Insert>>,
}

impl InsertChain {
    /// Empty chain (identity).
    pub fn new() -> Self {
        Self {
            inserts: Vec::new(),
        }
    }

    /// Replace the chain. More than [`MAX_INSERTS`] is `Err`; the previous chain stays.
    pub fn set(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        if inserts.len() > MAX_INSERTS {
            return Err(InsertError::ChainFull {
                max: MAX_INSERTS,
                got: inserts.len(),
            });
        }
        self.inserts = inserts;
        Ok(())
    }

    /// Apply inserts in playback order (first insert first).
    pub fn process(&mut self, frames: &mut [f32]) {
        for insert in &mut self.inserts {
            insert.process(frames);
        }
    }
}

impl Default for InsertChain {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for InsertChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InsertChain")
            .field("len", &self.inserts.len())
            .finish()
    }
}

/// Failure replacing a playback insert chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertError {
    /// Replacement had more than [`MAX_INSERTS`] inserts.
    ChainFull {
        /// Maximum inserts allowed on one sink.
        max: usize,
        /// Number of inserts in the rejected replacement.
        got: usize,
    },
}

impl fmt::Display for InsertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InsertError::ChainFull { max, got } => {
                write!(f, "insert chain cannot exceed {max} inserts, got {got}")
            }
        }
    }
}

impl Error for InsertError {}

/// Apply `chain` to interleaved PCM.
///
/// Mono (`channels == 1`) is processed in place. More channels: take the first
/// sample of each frame, process a mono stream in stack chunks of 64, then
/// duplicate the result across the frame.
pub fn apply_interleaved(chain: &mut InsertChain, interleaved: &mut [f32], channels: u16) {
    if channels <= 1 {
        chain.process(interleaved);
        return;
    }
    let ch = usize::from(channels);
    let mut offset = 0;
    while offset + ch <= interleaved.len() {
        let remaining_frames = (interleaved.len() - offset) / ch;
        let n = remaining_frames.min(64);
        let mut mono = [0.0f32; 64];
        for frame in 0..n {
            mono[frame] = interleaved[offset + frame * ch];
        }
        chain.process(&mut mono[..n]);
        for frame in 0..n {
            let sample = mono[frame];
            let base = offset + frame * ch;
            interleaved[base..base + ch].fill(sample);
        }
        offset += n * ch;
    }
}
