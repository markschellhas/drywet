use std::error::Error;
use std::fmt;

use crate::limits::{BPM_MAX, BPM_MIN, DEFAULT_PPQ};
use crate::sink::Sink;
use crate::time::{to_frequency, to_seconds, to_ticks, IntoTime, TimeError};

type EventCallback = Box<dyn Fn(f64) + 'static>;

#[derive(Clone, Copy)]
enum TransportEvent {
    Start,
    Stop,
    Pause,
    Loop,
}

fn parse_event(name: &str) -> Result<TransportEvent, TransportError> {
    match name {
        "start" => Ok(TransportEvent::Start),
        "stop" => Ok(TransportEvent::Stop),
        "pause" => Ok(TransportEvent::Pause),
        "loop" => Ok(TransportEvent::Loop),
        _ => Err(TransportError::UnknownEvent(name.to_string())),
    }
}

/// Wrap `seconds` into `[start, end)` when the loop range is valid.
fn wrap_into_loop(seconds: f64, start: f64, end: f64) -> f64 {
    let length = end - start;
    if length > 0.0 {
        start + (seconds - start).rem_euclid(length)
    } else {
        seconds
    }
}

/// Arrangement clock owned by [`crate::Context`].
///
/// Start, stop, pause, and tempo writes that also touch the sink go through
/// [`TransportRef`], so one borrow can mark the sink accepted and stop it
/// without closing.
pub struct Transport {
    state: TransportState,
    bpm: f64,
    running_bpm: f64,
    time_signature: (u32, u32),
    seconds: f64,
    looping: bool,
    loop_start_s: f64,
    loop_end_s: f64,
    on_start: Vec<EventCallback>,
    on_stop: Vec<EventCallback>,
    on_pause: Vec<EventCallback>,
    on_loop: Vec<EventCallback>,
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

/// Tempo, loop, or event write that drywet-py would raise as `ValueError`.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportError {
    /// Tempo was outside 40–240 BPM.
    InvalidBpm(f64),
    /// Signature was not a positive `(n, d)` pair or a positive beat count.
    InvalidTimeSignature,
    /// `set_loop_points` end was not strictly after start.
    InvalidLoopPoints,
    /// `on` was given a name other than `start` / `stop` / `pause` / `loop`.
    UnknownEvent(String),
    /// Musical time conversion failed (for example in `set_loop_points`).
    Time(TimeError),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::InvalidBpm(bpm) => write!(f, "BPM must be 40–240, got {bpm}"),
            TransportError::InvalidTimeSignature => {
                write!(f, "time_signature must be (n, d) or a positive int")
            }
            TransportError::InvalidLoopPoints => {
                write!(f, "loop_end must be after loop_start")
            }
            TransportError::UnknownEvent(name) => write!(f, "unknown event: {name:?}"),
            TransportError::Time(err) => write!(f, "{err}"),
        }
    }
}

impl Error for TransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TransportError::Time(err) => Some(err),
            TransportError::InvalidBpm(_)
            | TransportError::InvalidTimeSignature
            | TransportError::InvalidLoopPoints
            | TransportError::UnknownEvent(_) => None,
        }
    }
}

impl From<TimeError> for TransportError {
    fn from(err: TimeError) -> Self {
        TransportError::Time(err)
    }
}

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

/// Port of drywet-py `Transport.position`: `bars:beats:sixteenths` from seconds.
fn bars_beats_sixteenths(seconds: f64, bpm: f64, time_signature: (u32, u32)) -> String {
    let (num, den) = time_signature;
    let quarter = 60.0 / bpm;
    let beat = quarter * (4.0 / f64::from(den));
    let bar = f64::from(num) * beat;
    let sixteenth = quarter / 4.0;
    let mut remaining = seconds;
    let mut bars = if bar != 0.0 {
        (remaining / bar).floor() as i64
    } else {
        0
    };
    remaining -= bars as f64 * bar;
    let mut beats = if beat != 0.0 {
        (remaining / beat).floor() as i64
    } else {
        0
    };
    remaining -= beats as f64 * beat;
    let mut sixteenths = if sixteenth != 0.0 {
        (remaining / sixteenth).round() as i64
    } else {
        0
    };
    if sixteenths == 4 {
        sixteenths = 0;
        beats += 1;
    }
    if beats >= i64::from(num) {
        beats = 0;
        bars += 1;
    }
    format!("{bars}:{beats}:{sixteenths}")
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transport")
            .field("state", &self.state)
            .field("bpm", &self.bpm)
            .field("running_bpm", &self.running_bpm)
            .field("time_signature", &self.time_signature)
            .field("seconds", &self.seconds)
            .field("loop", &self.looping)
            .field("loop_start", &self.loop_start_s)
            .field("loop_end", &self.loop_end_s)
            .finish_non_exhaustive()
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
            looping: false,
            loop_start_s: 0.0,
            loop_end_s: 0.0,
            on_start: Vec::new(),
            on_stop: Vec::new(),
            on_pause: Vec::new(),
            on_loop: Vec::new(),
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

    /// Playhead in seconds. Render (later) advances this from frames / sample rate.
    pub fn seconds(&self) -> f64 {
        self.seconds
    }

    /// Set the playhead in seconds. Tests use this until render advances frames.
    ///
    /// When looping and the loop range is valid, wraps into `[loop_start, loop_end)`.
    pub fn set_seconds(&mut self, seconds: f64) {
        self.seconds = seconds;
        self.apply_loop_wrap();
    }

    /// Whether the playhead wraps between [`loop_start`](Self::loop_start) and
    /// [`loop_end`](Self::loop_end). Render (later) also mixes the last tail
    /// into the next cycle; cycle length stays nominal.
    pub fn r#loop(&self) -> bool {
        self.looping
    }

    /// Enable or disable looping. When enabled, wraps the current playhead
    /// into the loop range if that range is valid.
    pub fn set_loop(&mut self, enabled: bool) {
        self.looping = enabled;
        self.apply_loop_wrap();
    }

    /// Loop start in seconds.
    pub fn loop_start(&self) -> f64 {
        self.loop_start_s
    }

    /// Loop end in seconds.
    pub fn loop_end(&self) -> f64 {
        self.loop_end_s
    }

    /// Set loop start and end from musical times. `end` must be after `start`.
    pub fn set_loop_points(
        &mut self,
        start: impl IntoTime,
        end: impl IntoTime,
    ) -> Result<(), TransportError> {
        let start_s = self.to_seconds(start)?;
        let end_s = self.to_seconds(end)?;
        if end_s <= start_s {
            return Err(TransportError::InvalidLoopPoints);
        }
        self.loop_start_s = start_s;
        self.loop_end_s = end_s;
        self.apply_loop_wrap();
        Ok(())
    }

    /// Register a callback for `start`, `stop`, `pause`, or `loop`.
    ///
    /// Callbacks receive the event time in seconds. Hosts may ignore events
    /// and poll [`position`](Self::position) instead.
    pub fn on(
        &mut self,
        name: &str,
        callback: impl Fn(f64) + 'static,
    ) -> Result<(), TransportError> {
        let slot = match parse_event(name)? {
            TransportEvent::Start => &mut self.on_start,
            TransportEvent::Stop => &mut self.on_stop,
            TransportEvent::Pause => &mut self.on_pause,
            TransportEvent::Loop => &mut self.on_loop,
        };
        slot.push(Box::new(callback));
        Ok(())
    }

    fn apply_loop_wrap(&mut self) {
        if self.looping {
            self.seconds = wrap_into_loop(self.seconds, self.loop_start_s, self.loop_end_s);
        }
    }

    fn emit(&self, event: TransportEvent) {
        let time = self.seconds;
        let slot = match event {
            TransportEvent::Start => &self.on_start,
            TransportEvent::Stop => &self.on_stop,
            TransportEvent::Pause => &self.on_pause,
            TransportEvent::Loop => &self.on_loop,
        };
        for callback in slot {
            callback(time);
        }
    }

    /// Playhead in PPQ ticks: `to_ticks(seconds)` at [`DEFAULT_PPQ`].
    pub fn ticks(&self) -> i64 {
        self.to_ticks(self.seconds)
            .expect("numeric seconds always convert")
    }

    /// Bars:beats:sixteenths from the current playhead and [`clock_bpm`](Self::clock_bpm).
    pub fn position(&self) -> String {
        bars_beats_sixteenths(self.seconds, self.clock_bpm(), self.time_signature)
    }

    /// Convert a note value, BBS string, or raw seconds using the current clock.
    pub fn to_seconds(&self, value: impl IntoTime) -> Result<f64, TimeError> {
        to_seconds(
            value,
            self.clock_bpm(),
            self.time_signature,
            self.seconds,
            DEFAULT_PPQ,
        )
    }

    /// Convert a time value to pulses at [`DEFAULT_PPQ`] using the current clock.
    pub fn to_ticks(&self, value: impl IntoTime) -> Result<i64, TimeError> {
        to_ticks(
            value,
            self.clock_bpm(),
            self.time_signature,
            self.seconds,
            DEFAULT_PPQ,
        )
    }

    /// Convert a note name or numeric Hz using the current clock arguments.
    pub fn to_frequency(&self, value: impl IntoTime) -> Result<f64, TimeError> {
        to_frequency(
            value,
            self.clock_bpm(),
            self.time_signature,
            self.seconds,
            DEFAULT_PPQ,
        )
    }

    fn start(&mut self) -> bool {
        match self.state {
            TransportState::Started => false,
            TransportState::Paused => {
                self.state = TransportState::Started;
                self.emit(TransportEvent::Start);
                false
            }
            TransportState::Stopped => {
                self.running_bpm = self.bpm;
                self.state = TransportState::Started;
                self.seconds = 0.0;
                self.emit(TransportEvent::Start);
                true
            }
        }
    }

    fn pause(&mut self) {
        if self.state == TransportState::Started {
            self.state = TransportState::Paused;
            self.emit(TransportEvent::Pause);
        }
    }

    fn stop(&mut self) {
        self.state = TransportState::Stopped;
        self.seconds = 0.0;
        self.emit(TransportEvent::Stop);
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

    /// Set the playhead in seconds. Tests use this until render advances frames.
    ///
    /// When looping and the loop range is valid, wraps into `[loop_start, loop_end)`.
    pub fn set_seconds(&mut self, seconds: f64) {
        self.transport.set_seconds(seconds);
    }

    /// Whether the playhead wraps between [`loop_start`](Self::loop_start) and
    /// [`loop_end`](Self::loop_end).
    pub fn r#loop(&self) -> bool {
        self.transport.r#loop()
    }

    /// Enable or disable looping. When enabled, wraps the current playhead
    /// into the loop range if that range is valid.
    pub fn set_loop(&mut self, enabled: bool) {
        self.transport.set_loop(enabled);
    }

    /// Loop start in seconds.
    pub fn loop_start(&self) -> f64 {
        self.transport.loop_start()
    }

    /// Loop end in seconds.
    pub fn loop_end(&self) -> f64 {
        self.transport.loop_end()
    }

    /// Set loop start and end from musical times. `end` must be after `start`.
    pub fn set_loop_points(
        &mut self,
        start: impl IntoTime,
        end: impl IntoTime,
    ) -> Result<(), TransportError> {
        self.transport.set_loop_points(start, end)
    }

    /// Register a callback for `start`, `stop`, `pause`, or `loop`.
    ///
    /// Callbacks receive the event time in seconds.
    pub fn on(
        &mut self,
        name: &str,
        callback: impl Fn(f64) + 'static,
    ) -> Result<(), TransportError> {
        self.transport.on(name, callback)
    }

    /// Playhead in PPQ ticks: `to_ticks(seconds)` at [`DEFAULT_PPQ`].
    pub fn ticks(&self) -> i64 {
        self.transport.ticks()
    }

    /// Bars:beats:sixteenths from the current playhead and clock tempo.
    pub fn position(&self) -> String {
        self.transport.position()
    }

    /// Convert a note value, BBS string, or raw seconds using the current clock.
    pub fn to_seconds(&self, value: impl IntoTime) -> Result<f64, TimeError> {
        self.transport.to_seconds(value)
    }

    /// Convert a time value to pulses at [`DEFAULT_PPQ`] using the current clock.
    pub fn to_ticks(&self, value: impl IntoTime) -> Result<i64, TimeError> {
        self.transport.to_ticks(value)
    }

    /// Convert a note name or numeric Hz using the current clock arguments.
    pub fn to_frequency(&self, value: impl IntoTime) -> Result<f64, TimeError> {
        self.transport.to_frequency(value)
    }
}
