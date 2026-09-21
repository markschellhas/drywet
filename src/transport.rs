use std::cell::RefCell;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::limits::{BPM_MAX, BPM_MIN, DEFAULT_PPQ, DEFAULT_SAMPLE_RATE, MAX_SCHEDULE_SECONDS};
use crate::sink::Sink;
use crate::time::{to_frequency, to_seconds, to_ticks, IntoTime, TimeError};

type ListenerCallback = Box<dyn Fn(f64) + 'static>;
type ScheduleCallback = Rc<dyn Fn(f64) + 'static>;

/// One-shot or repeating callback registered with [`Transport::schedule`].
struct ScheduledEvent {
    id: u64,
    time: f64,
    callback: ScheduleCallback,
    interval: Option<f64>,
}

/// `(event_id, round(when, 9) as nanos)` — same uniqueness as drywet-py `_fired`.
type FiredKey = (u64, i64);

const OCCURRENCE_EPS: f64 = 1e-12;

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
    sample_rate: u32,
    on_start: Vec<ListenerCallback>,
    on_stop: Vec<ListenerCallback>,
    on_pause: Vec<ListenerCallback>,
    on_loop: Vec<ListenerCallback>,
    events: Vec<ScheduledEvent>,
    next_event_id: u64,
    fired: HashSet<FiredKey>,
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
    /// Schedule time was negative or greater than [`MAX_SCHEDULE_SECONDS`].
    ScheduleTimeOutOfRange(f64),
    /// `schedule_repeat` interval converted to a non-positive duration.
    InvalidRepeatInterval,
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
            TransportError::ScheduleTimeOutOfRange(_) => {
                write!(f, "schedule time out of range")
            }
            TransportError::InvalidRepeatInterval => {
                write!(f, "repeat interval must be > 0")
            }
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
            | TransportError::UnknownEvent(_)
            | TransportError::ScheduleTimeOutOfRange(_)
            | TransportError::InvalidRepeatInterval => None,
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
            .field("sample_rate", &self.sample_rate)
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
            sample_rate: DEFAULT_SAMPLE_RATE,
            on_start: Vec::new(),
            on_stop: Vec::new(),
            on_pause: Vec::new(),
            on_loop: Vec::new(),
            events: Vec::new(),
            next_event_id: 1,
            fired: HashSet::new(),
        }
    }

    /// Current lifecycle state.
    pub fn state(&self) -> TransportState {
        self.state
    }

    /// Sample rate copied from [`crate::Context`] at construction.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Set the sample rate used by [`TransportRef::render`] to pad frames.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
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

    pub(crate) fn start(&mut self) -> bool {
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

    fn event_time(&self, value: impl IntoTime) -> Result<f64, TransportError> {
        let seconds = self.to_seconds(value)?;
        if seconds < 0.0 || seconds > MAX_SCHEDULE_SECONDS {
            return Err(TransportError::ScheduleTimeOutOfRange(seconds));
        }
        Ok(seconds)
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_event_id;
        self.next_event_id += 1;
        id
    }

    /// Schedule `callback` once at `time`. Returns the event id.
    ///
    /// `time` is converted with [`to_seconds`](Self::to_seconds). Times below 0
    /// or above [`MAX_SCHEDULE_SECONDS`] are rejected. Callbacks receive the
    /// event time in seconds and must not write PCM.
    pub fn schedule(
        &mut self,
        callback: impl Fn(f64) + 'static,
        time: impl IntoTime,
    ) -> Result<u64, TransportError> {
        let time = self.event_time(time)?;
        let id = self.next_id();
        self.events.push(ScheduledEvent {
            id,
            time,
            callback: Rc::new(callback),
            interval: None,
        });
        Ok(id)
    }

    /// Alias of [`schedule`](Self::schedule).
    pub fn schedule_once(
        &mut self,
        callback: impl Fn(f64) + 'static,
        time: impl IntoTime,
    ) -> Result<u64, TransportError> {
        self.schedule(callback, time)
    }

    /// Schedule `callback` at `start_time`, then every `interval` seconds.
    ///
    /// `interval` is converted with [`to_seconds`](Self::to_seconds). A
    /// non-positive interval is rejected when occurrences are generated
    /// ([`fire_until`](Self::fire_until)). `start_time` uses the same range
    /// check as [`schedule`](Self::schedule).
    pub fn schedule_repeat(
        &mut self,
        callback: impl Fn(f64) + 'static,
        interval: impl IntoTime,
        start_time: impl IntoTime,
    ) -> Result<u64, TransportError> {
        let start = self.event_time(start_time)?;
        let interval = self.to_seconds(interval)?;
        let id = self.next_id();
        self.events.push(ScheduledEvent {
            id,
            time: start,
            callback: Rc::new(callback),
            interval: Some(interval),
        });
        Ok(id)
    }

    /// Drop events whose start time is `>= after` (converted to seconds).
    pub fn cancel(&mut self, after: impl IntoTime) -> Result<(), TransportError> {
        let after_s = self.to_seconds(after)?;
        self.events.retain(|event| event.time < after_s);
        Ok(())
    }

    /// Remove scheduled events whose ids are in `ids`.
    ///
    /// Port of drywet-py filtering `_events` by id. [`crate::Sequence::stop`]
    /// uses this so a sequence can detach without clearing the rest of the
    /// transport schedule.
    pub fn cancel_ids(&mut self, ids: &[u64]) {
        if ids.is_empty() {
            return;
        }
        let to_drop: HashSet<u64> = ids.iter().copied().collect();
        self.events.retain(|event| !to_drop.contains(&event.id));
    }

    /// Remove every scheduled event and forget which occurrences have fired.
    pub fn clear(&mut self) {
        self.events.clear();
        self.fired.clear();
    }

    /// Occurrences of `event` at or before `until` (drywet-py `_occurrences`).
    ///
    /// When looping is on and a one-shot sits in `[loop_start, loop_end)`,
    /// also yield `start + n * length` while `<= until`. Virtual — the
    /// event list is not mutated, so a second fire/render is idempotent.
    fn occurrences(
        start: f64,
        interval: Option<f64>,
        until: f64,
        looping: bool,
        loop_start: f64,
        loop_end: f64,
    ) -> Result<Vec<f64>, TransportError> {
        match interval {
            None => {
                let mut times = Vec::new();
                if start <= until + OCCURRENCE_EPS {
                    times.push(start);
                }
                if looping && loop_end > loop_start && loop_start <= start && start < loop_end {
                    let length = loop_end - loop_start;
                    let mut cursor = start + length;
                    while cursor <= until + OCCURRENCE_EPS {
                        times.push(cursor);
                        cursor += length;
                    }
                }
                Ok(times)
            }
            Some(interval) => {
                if interval <= 0.0 {
                    return Err(TransportError::InvalidRepeatInterval);
                }
                let mut times = Vec::new();
                let mut cursor = start;
                while cursor <= until + OCCURRENCE_EPS {
                    times.push(cursor);
                    cursor += interval;
                }
                Ok(times)
            }
        }
    }

    /// Unfired `(when, callback, key)` pairs at or before `until_s`, sorted.
    pub(crate) fn collect_due(
        &mut self,
        until_s: f64,
    ) -> Result<Vec<(f64, ScheduleCallback, FiredKey)>, TransportError> {
        let mut pending: Vec<(f64, u64, ScheduleCallback, FiredKey)> = Vec::new();
        for event in &self.events {
            for when in Self::occurrences(
                event.time,
                event.interval,
                until_s,
                self.looping,
                self.loop_start_s,
                self.loop_end_s,
            )? {
                let key = fired_key(event.id, when);
                if self.fired.contains(&key) {
                    continue;
                }
                pending.push((when, event.id, Rc::clone(&event.callback), key));
            }
        }
        pending.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Ok(pending
            .into_iter()
            .map(|(when, _id, callback, key)| (when, callback, key))
            .collect())
    }

    pub(crate) fn set_seconds_raw(&mut self, seconds: f64) {
        self.seconds = seconds;
    }

    pub(crate) fn mark_fired(&mut self, key: FiredKey) {
        self.fired.insert(key);
    }

    /// Fire unfired occurrences at or before `until`, in `(when, id)` order.
    ///
    /// Port of drywet-py `_fire_until`. Sets the playhead to each occurrence
    /// time before calling the callback, then to `until`. Exposed so tests
    /// and later Sequence / render can drive the clock without mixing PCM.
    pub fn fire_until(&mut self, until: impl IntoTime) -> Result<(), TransportError> {
        let until_s = self.to_seconds(until)?;
        let pending = self.collect_due(until_s)?;
        for (when, callback, key) in pending {
            self.seconds = when;
            callback(when);
            self.fired.insert(key);
        }
        self.seconds = until_s;
        Ok(())
    }
}

/// Fire due events without holding a `RefCell` borrow across callbacks.
///
/// Used by [`crate::Context::render`] and [`TransportRef::fire_until`] so a
/// callback can `to_seconds` / mix onto the sink.
pub(crate) fn fire_until_releasing(
    transport: &RefCell<Transport>,
    until: impl IntoTime,
) -> Result<(), TransportError> {
    let until_s = transport.borrow().to_seconds(until)?;
    let pending = transport.borrow_mut().collect_due(until_s)?;
    for (when, callback, key) in pending {
        transport.borrow_mut().set_seconds_raw(when);
        callback(when);
        transport.borrow_mut().mark_fired(key);
    }
    transport.borrow_mut().set_seconds_raw(until_s);
    Ok(())
}

fn fired_key(id: u64, when: f64) -> FiredKey {
    (id, (when * 1e9).round() as i64)
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}

/// View of [`Transport`] plus the context sink (`RefCell` handles).
///
/// Each method takes a short [`RefCell`] borrow so a schedule callback can
/// mix or read the clock without a second exclusive [`crate::Context`] borrow.
pub struct TransportRef<'a, S: Sink> {
    transport: &'a RefCell<Transport>,
    sink: &'a RefCell<S>,
}

impl<'a, S: Sink> TransportRef<'a, S> {
    pub(crate) fn new(transport: &'a RefCell<Transport>, sink: &'a RefCell<S>) -> Self {
        Self { transport, sink }
    }

    /// Start or resume the clock.
    ///
    /// Already started is a no-op. Paused resumes without copying the pending
    /// tempo. Stopped copies the written bpm onto the running clock, resets
    /// seconds to `0`, and marks the sink accepted.
    pub fn start(&mut self) -> &mut Self {
        if self.transport.borrow_mut().start() {
            self.sink.borrow_mut().mark_accepted();
        }
        self
    }

    /// Pause if started; otherwise a no-op.
    pub fn pause(&mut self) -> &mut Self {
        self.transport.borrow_mut().pause();
        self
    }

    /// Stop the arrangement clock and the sink. Does not close the sink.
    pub fn stop(&mut self) -> &mut Self {
        self.transport.borrow_mut().stop();
        self.sink.borrow_mut().stop();
        self
    }

    /// Pause when started; otherwise start.
    pub fn toggle(&mut self) -> &mut Self {
        if self.state() == TransportState::Started {
            self.pause()
        } else {
            self.start()
        }
    }

    /// Current lifecycle state.
    pub fn state(&self) -> TransportState {
        self.transport.borrow().state()
    }

    /// Sample rate copied from [`crate::Context`] onto the transport.
    pub fn sample_rate(&self) -> u32 {
        self.transport.borrow().sample_rate()
    }

    /// Output latency from the sink.
    pub fn latency_ms(&self) -> u32 {
        self.sink.borrow().latency_ms()
    }

    /// Last-set tempo (written `_bpm`).
    pub fn bpm(&self) -> f64 {
        self.transport.borrow().bpm()
    }

    /// Running tempo while started; otherwise the last-set value.
    pub fn clock_bpm(&self) -> f64 {
        self.transport.borrow().clock_bpm()
    }

    /// Store `bpm`. Applies immediately unless started (then next start).
    pub fn set_bpm(&mut self, bpm: f64) -> Result<(), TransportError> {
        self.transport.borrow_mut().set_bpm(bpm)
    }

    /// Current `(numerator, denominator)`.
    pub fn time_signature(&self) -> (u32, u32) {
        self.transport.borrow().time_signature()
    }

    /// Set `(n, d)`, or an int beat count (`4` → `(4, 4)`).
    pub fn set_time_signature(
        &mut self,
        value: impl Into<TimeSignature>,
    ) -> Result<(), TransportError> {
        self.transport.borrow_mut().set_time_signature(value)
    }

    /// Playhead in seconds. `0.0` at rest and after stop.
    pub fn seconds(&self) -> f64 {
        self.transport.borrow().seconds()
    }

    /// Set the playhead in seconds. Tests use this until render advances frames.
    ///
    /// When looping and the loop range is valid, wraps into `[loop_start, loop_end)`.
    pub fn set_seconds(&mut self, seconds: f64) {
        self.transport.borrow_mut().set_seconds(seconds);
    }

    /// Whether the playhead wraps between [`loop_start`](Self::loop_start) and
    /// [`loop_end`](Self::loop_end).
    pub fn r#loop(&self) -> bool {
        self.transport.borrow().r#loop()
    }

    /// Enable or disable looping. When enabled, wraps the current playhead
    /// into the loop range if that range is valid.
    pub fn set_loop(&mut self, enabled: bool) {
        self.transport.borrow_mut().set_loop(enabled);
    }

    /// Loop start in seconds.
    pub fn loop_start(&self) -> f64 {
        self.transport.borrow().loop_start()
    }

    /// Loop end in seconds.
    pub fn loop_end(&self) -> f64 {
        self.transport.borrow().loop_end()
    }

    /// Set loop start and end from musical times. `end` must be after `start`.
    pub fn set_loop_points(
        &mut self,
        start: impl IntoTime,
        end: impl IntoTime,
    ) -> Result<(), TransportError> {
        self.transport.borrow_mut().set_loop_points(start, end)
    }

    /// Register a callback for `start`, `stop`, `pause`, or `loop`.
    ///
    /// Callbacks receive the event time in seconds.
    pub fn on(
        &mut self,
        name: &str,
        callback: impl Fn(f64) + 'static,
    ) -> Result<(), TransportError> {
        self.transport.borrow_mut().on(name, callback)
    }

    /// Playhead in PPQ ticks: `to_ticks(seconds)` at [`DEFAULT_PPQ`].
    pub fn ticks(&self) -> i64 {
        self.transport.borrow().ticks()
    }

    /// Bars:beats:sixteenths from the current playhead and clock tempo.
    pub fn position(&self) -> String {
        self.transport.borrow().position()
    }

    /// Convert a note value, BBS string, or raw seconds using the current clock.
    pub fn to_seconds(&self, value: impl IntoTime) -> Result<f64, TimeError> {
        self.transport.borrow().to_seconds(value)
    }

    /// Convert a time value to pulses at [`DEFAULT_PPQ`] using the current clock.
    pub fn to_ticks(&self, value: impl IntoTime) -> Result<i64, TimeError> {
        self.transport.borrow().to_ticks(value)
    }

    /// Convert a note name or numeric Hz using the current clock arguments.
    pub fn to_frequency(&self, value: impl IntoTime) -> Result<f64, TimeError> {
        self.transport.borrow().to_frequency(value)
    }

    /// Schedule `callback` once at `time`. Returns the event id.
    ///
    /// Callbacks receive the event time in seconds and must not write PCM.
    pub fn schedule(
        &mut self,
        callback: impl Fn(f64) + 'static,
        time: impl IntoTime,
    ) -> Result<u64, TransportError> {
        self.transport.borrow_mut().schedule(callback, time)
    }

    /// Alias of [`schedule`](Self::schedule).
    pub fn schedule_once(
        &mut self,
        callback: impl Fn(f64) + 'static,
        time: impl IntoTime,
    ) -> Result<u64, TransportError> {
        self.transport.borrow_mut().schedule_once(callback, time)
    }

    /// Schedule `callback` at `start_time`, then every `interval` seconds.
    pub fn schedule_repeat(
        &mut self,
        callback: impl Fn(f64) + 'static,
        interval: impl IntoTime,
        start_time: impl IntoTime,
    ) -> Result<u64, TransportError> {
        self.transport
            .borrow_mut()
            .schedule_repeat(callback, interval, start_time)
    }

    /// Drop events whose start time is `>= after` (converted to seconds).
    pub fn cancel(&mut self, after: impl IntoTime) -> Result<(), TransportError> {
        self.transport.borrow_mut().cancel(after)
    }

    /// Remove scheduled events whose ids are in `ids`.
    pub fn cancel_ids(&mut self, ids: &[u64]) {
        self.transport.borrow_mut().cancel_ids(ids);
    }

    /// Remove every scheduled event and forget which occurrences have fired.
    pub fn clear(&mut self) {
        self.transport.borrow_mut().clear();
    }

    /// Clear the schedule, stop the clock and sink, and close the sink.
    pub fn dispose(&mut self) {
        self.transport.borrow_mut().clear();
        self.stop();
        self.sink.borrow_mut().close();
    }

    /// Fire unfired occurrences at or before `until`, in `(when, id)` order.
    ///
    /// Drops the transport `RefCell` borrow before each callback so
    /// instruments can mix and convert time.
    pub fn fire_until(&mut self, until: impl IntoTime) -> Result<(), TransportError> {
        fire_until_releasing(self.transport, until)
    }
}
