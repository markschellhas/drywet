use std::error::Error;
use std::fmt;

use crate::limits::{BPM_MAX, BPM_MIN};
use crate::sink::Sink;

/// Arrangement clock owned by [`crate::Context`].
///
/// Start, stop, pause, and tempo writes that also touch the sink go through
/// [`TransportRef`], so one borrow can mark the sink accepted and stop it
/// without closing.
#[derive(Debug)]
pub struct Transport {
    state: TransportState,
    bpm: f64,
    running_bpm: f64,
    time_signature: (u32, u32),
    seconds: f64,
}

/// Playhead lifecycle: stopped, started, or paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    /// Clock is idle; playhead is at zero.
    Stopped,
    /// Clock is running.
    Started,
    /// Clock is held; resume with [`TransportRef::start`] or [`TransportRef::toggle`].
    Paused,
}

/// Tempo or time-signature write that drywet-py would raise as `ValueError`.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportError {
    /// Tempo was outside 40–240 BPM.
    InvalidBpm(f64),
    /// Signature was not a positive `(n, d)` pair or a positive beat count.
    InvalidTimeSignature,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::InvalidBpm(bpm) => write!(f, "BPM must be 40–240, got {bpm}"),
            TransportError::InvalidTimeSignature => {
                write!(f, "time_signature must be (n, d) or a positive int")
            }
        }
    }
}

impl Error for TransportError {}

/// Time-signature input: beat count (`4` → `(4, 4)`) or an `(n, d)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSignature {
    /// Beats per bar; denominator is 4. `4` becomes `(4, 4)`.
    Beats(u32),
    /// Explicit numerator and denominator.
    Pair(u32, u32),
}

impl From<u32> for TimeSignature {
    fn from(beats: u32) -> Self {
        TimeSignature::Beats(beats)
    }
}

impl From<(u32, u32)> for TimeSignature {
    fn from(pair: (u32, u32)) -> Self {
        TimeSignature::Pair(pair.0, pair.1)
    }
}

fn normalize_signature(value: TimeSignature) -> Result<(u32, u32), TransportError> {
    let (numerator, denominator) = match value {
        TimeSignature::Beats(4) => (4, 4),
        TimeSignature::Beats(beats) => (beats, 4),
        TimeSignature::Pair(numerator, denominator) => (numerator, denominator),
    };
    if numerator == 0 || denominator == 0 {
        Err(TransportError::InvalidTimeSignature)
    } else {
        Ok((numerator, denominator))
    }
}

impl Transport {
    /// Stopped transport at 120 BPM, 4/4, playhead at zero.
    pub fn new() -> Self {
        Self {
            state: TransportState::Stopped,
            bpm: 120.0,
            running_bpm: 120.0,
            time_signature: (4, 4),
            seconds: 0.0,
        }
    }

    /// Current lifecycle state.
    pub fn state(&self) -> TransportState {
        self.state
    }

    /// Last-set tempo. Conversions use [`clock_bpm`](Self::clock_bpm).
    pub fn bpm(&self) -> f64 {
        self.bpm
    }

    /// Running tempo while started; otherwise the last-set value.
    pub fn clock_bpm(&self) -> f64 {
        if self.state == TransportState::Started {
            self.running_bpm
        } else {
            self.bpm
        }
    }

    /// Store `bpm`. Applies immediately unless the clock is started, in which
    /// case it takes effect on the next start from stopped.
    pub fn set_bpm(&mut self, bpm: f64) -> Result<(), TransportError> {
        if !(f64::from(BPM_MIN)..=f64::from(BPM_MAX)).contains(&bpm) {
            return Err(TransportError::InvalidBpm(bpm));
        }
        self.bpm = bpm;
        if self.state != TransportState::Started {
            self.running_bpm = bpm;
        }
        Ok(())
    }

    /// Current `(numerator, denominator)`.
    pub fn time_signature(&self) -> (u32, u32) {
        self.time_signature
    }

    /// Set `(n, d)`, or an int beat count (`4` → `(4, 4)`).
    pub fn set_time_signature(
        &mut self,
        value: impl Into<TimeSignature>,
    ) -> Result<(), TransportError> {
        self.time_signature = normalize_signature(value.into())?;
        Ok(())
    }

    /// Playhead in seconds. Always `0.0` until a later task advances frames.
    pub fn seconds(&self) -> f64 {
        self.seconds
    }

    /// Bars:beats:sixteenths. `"0:0:0"` at rest and after stop.
    pub fn position(&self) -> String {
        "0:0:0".to_string()
    }

    fn start(&mut self) -> bool {
        match self.state {
            TransportState::Started => false,
            TransportState::Paused => {
                self.state = TransportState::Started;
                false
            }
            TransportState::Stopped => {
                self.running_bpm = self.bpm;
                self.state = TransportState::Started;
                self.seconds = 0.0;
                true
            }
        }
    }

    fn pause(&mut self) {
        if self.state == TransportState::Started {
            self.state = TransportState::Paused;
        }
    }

    fn stop(&mut self) {
        self.state = TransportState::Stopped;
        self.seconds = 0.0;
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}

/// Mutable view of [`Transport`] plus the context sink.
///
/// Needed so [`start`](Self::start) can mark the sink accepted and
/// [`stop`](Self::stop) can call [`Sink::stop`] without a second borrow.
pub struct TransportRef<'a, S: Sink> {
    transport: &'a mut Transport,
    sink: &'a mut S,
}

impl<'a, S: Sink> TransportRef<'a, S> {
    pub(crate) fn new(transport: &'a mut Transport, sink: &'a mut S) -> Self {
        Self { transport, sink }
    }

    /// Start or resume the clock.
    ///
    /// Already started is a no-op. Paused resumes without copying the pending
    /// tempo. Stopped copies the written bpm onto the running clock, resets
    /// seconds to `0`, and marks the sink accepted.
    pub fn start(&mut self) -> &mut Self {
        if self.transport.start() {
            self.sink.mark_accepted();
        }
        self
    }

    /// Pause if started; otherwise a no-op.
    pub fn pause(&mut self) -> &mut Self {
        self.transport.pause();
        self
    }

    /// Stop the arrangement clock and the sink. Does not close the sink.
    pub fn stop(&mut self) -> &mut Self {
        self.transport.stop();
        self.sink.stop();
        self
    }

    /// Pause when started; otherwise start.
    pub fn toggle(&mut self) -> &mut Self {
        if self.transport.state() == TransportState::Started {
            self.pause()
        } else {
            self.start()
        }
    }

    /// Current lifecycle state.
    pub fn state(&self) -> TransportState {
        self.transport.state()
    }

    /// Output latency from the sink.
    pub fn latency_ms(&self) -> u32 {
        self.sink.latency_ms()
    }

    /// Last-set tempo (written `_bpm`).
    pub fn bpm(&self) -> f64 {
        self.transport.bpm()
    }

    /// Running tempo while started; otherwise the last-set value.
    pub fn clock_bpm(&self) -> f64 {
        self.transport.clock_bpm()
    }

    /// Store `bpm`. Applies immediately unless started (then next start).
    pub fn set_bpm(&mut self, bpm: f64) -> Result<(), TransportError> {
        self.transport.set_bpm(bpm)
    }

    /// Current `(numerator, denominator)`.
    pub fn time_signature(&self) -> (u32, u32) {
        self.transport.time_signature()
    }

    /// Set `(n, d)`, or an int beat count (`4` → `(4, 4)`).
    pub fn set_time_signature(
        &mut self,
        value: impl Into<TimeSignature>,
    ) -> Result<(), TransportError> {
        self.transport.set_time_signature(value)
    }

    /// Playhead in seconds. `0.0` at rest and after stop.
    pub fn seconds(&self) -> f64 {
        self.transport.seconds()
    }

    /// Bars:beats:sixteenths. `"0:0:0"` at rest and after stop.
    pub fn position(&self) -> String {
        self.transport.position()
    }
}
