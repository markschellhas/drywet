use std::rc::Rc;

use crate::sink::Sink;
use crate::time::{IntoTime, TimeError, TimeValue};
use crate::transport::{Transport, TransportError, TransportRef};

/// One slot in a [`Sequence`]: a rest, a note/value, or a nested group.
///
/// Nested groups subdivide the parent slot equally (Tone / drywet-py flatten).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceEvent {
    /// Skip this slot (`None` in drywet-py).
    Rest,
    /// Fire the sequence callback with this value.
    Value(String),
    /// Subdivide this slot across the inner events.
    Group(Vec<SequenceEvent>),
}

impl SequenceEvent {
    /// Nested group so tests can write `SequenceEvent::group(["E4", "G4"])`.
    pub fn group(events: impl IntoIterator<Item = impl Into<SequenceEvent>>) -> Self {
        SequenceEvent::Group(events.into_iter().map(Into::into).collect())
    }
}

impl From<&str> for SequenceEvent {
    fn from(value: &str) -> Self {
        SequenceEvent::Value(value.to_string())
    }
}

impl From<String> for SequenceEvent {
    fn from(value: String) -> Self {
        SequenceEvent::Value(value)
    }
}

impl From<&String> for SequenceEvent {
    fn from(value: &String) -> Self {
        SequenceEvent::Value(value.clone())
    }
}

impl From<Option<&str>> for SequenceEvent {
    fn from(value: Option<&str>) -> Self {
        match value {
            Some(value) => SequenceEvent::Value(value.to_string()),
            None => SequenceEvent::Rest,
        }
    }
}

impl From<Option<String>> for SequenceEvent {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(value) => SequenceEvent::Value(value),
            None => SequenceEvent::Rest,
        }
    }
}

impl<T: Into<SequenceEvent>> From<Vec<T>> for SequenceEvent {
    fn from(events: Vec<T>) -> Self {
        SequenceEvent::group(events)
    }
}

impl<T: Into<SequenceEvent>, const N: usize> From<[T; N]> for SequenceEvent {
    fn from(events: [T; N]) -> Self {
        SequenceEvent::group(events)
    }
}

/// Clock that [`Sequence`], [`Part`], and [`Loop`] can attach to:
/// [`Transport`] or [`TransportRef`].
pub trait SequenceClock {
    /// Schedule `callback` once at `time` seconds. Returns the event id.
    fn schedule<F>(&mut self, callback: F, time: f64) -> Result<u64, TransportError>
    where
        F: Fn(f64) + 'static;

    /// Schedule `callback` at `start_time`, then every `interval` seconds.
    fn schedule_repeat<F>(
        &mut self,
        callback: F,
        interval: f64,
        start_time: f64,
    ) -> Result<u64, TransportError>
    where
        F: Fn(f64) + 'static;

    /// Remove scheduled events whose ids are in `ids`.
    fn cancel_ids(&mut self, ids: &[u64]);

    /// Convert a musical time using the current clock.
    fn to_seconds(&self, value: TimeValue) -> Result<f64, TimeError>;
}

impl SequenceClock for Transport {
    fn schedule<F>(&mut self, callback: F, time: f64) -> Result<u64, TransportError>
    where
        F: Fn(f64) + 'static,
    {
        Transport::schedule(self, callback, time)
    }

    fn schedule_repeat<F>(
        &mut self,
        callback: F,
        interval: f64,
        start_time: f64,
    ) -> Result<u64, TransportError>
    where
        F: Fn(f64) + 'static,
    {
        Transport::schedule_repeat(self, callback, interval, start_time)
    }

    fn cancel_ids(&mut self, ids: &[u64]) {
        Transport::cancel_ids(self, ids);
    }

    fn to_seconds(&self, value: TimeValue) -> Result<f64, TimeError> {
        Transport::to_seconds(self, value)
    }
}

impl<S: Sink> SequenceClock for TransportRef<'_, S> {
    fn schedule<F>(&mut self, callback: F, time: f64) -> Result<u64, TransportError>
    where
        F: Fn(f64) + 'static,
    {
        TransportRef::schedule(self, callback, time)
    }

    fn schedule_repeat<F>(
        &mut self,
        callback: F,
        interval: f64,
        start_time: f64,
    ) -> Result<u64, TransportError>
    where
        F: Fn(f64) + 'static,
    {
        TransportRef::schedule_repeat(self, callback, interval, start_time)
    }

    fn cancel_ids(&mut self, ids: &[u64]) {
        TransportRef::cancel_ids(self, ids);
    }

    fn to_seconds(&self, value: TimeValue) -> Result<f64, TimeError> {
        TransportRef::to_seconds(self, value)
    }
}

/// Tone-style event list that schedules callbacks on a [`Transport`].
///
/// Nested lists subdivide the parent slot. `None` / [`SequenceEvent::Rest`]
/// is a rest. `.start(offset)` only registers ids; callbacks stay silent
/// until `transport.start()` / [`Transport::fire_until`] / later render.
pub struct Sequence {
    callback: Rc<dyn Fn(f64, Option<&str>)>,
    events: Vec<SequenceEvent>,
    subdivision: TimeValue,
    ids: Vec<u64>,
}

impl Sequence {
    /// Build a sequence. `events` can be `["C4", "E4", …]` or nested
    /// [`SequenceEvent`] values.
    pub fn new(
        callback: impl Fn(f64, Option<&str>) + 'static,
        events: impl IntoIterator<Item = impl Into<SequenceEvent>>,
        subdivision: impl IntoTime,
    ) -> Self {
        Self {
            callback: Rc::new(callback),
            events: events.into_iter().map(Into::into).collect(),
            subdivision: subdivision.into_time(),
            ids: Vec::new(),
        }
    }

    /// Flatten events from `offset` and schedule them on `transport`.
    ///
    /// Replaces any previous attachment (stops first). Returns `self` for
    /// chaining. Accepts [`Transport`] or [`TransportRef`].
    pub fn start(
        &mut self,
        transport: &mut impl SequenceClock,
        offset: impl IntoTime,
    ) -> Result<&mut Self, TransportError> {
        self.stop(transport);
        let start = transport.to_seconds(offset.into_time())?;
        let subdivision = transport.to_seconds(self.subdivision.clone())?;
        // width = to_seconds(subdivision) * len(events); slot = width / len.
        let times = flatten(&self.events, start, subdivision * self.events.len() as f64);
        let mut ids = Vec::with_capacity(times.len());
        for (when, value) in times {
            let callback = Rc::clone(&self.callback);
            match transport.schedule(move |time| callback(time, Some(value.as_str())), when) {
                Ok(id) => ids.push(id),
                Err(err) => {
                    transport.cancel_ids(&ids);
                    return Err(err);
                }
            }
        }
        self.ids = ids;
        Ok(self)
    }

    /// Detach from `transport` by cancelling this sequence's scheduled ids.
    pub fn stop(&mut self, transport: &mut impl SequenceClock) -> &mut Self {
        transport.cancel_ids(&self.ids);
        self.ids.clear();
        self
    }
}

/// Port of drywet-py Sequence flatten.
///
/// `width` is the span of `events` at this nesting level (`slot = width / len`).
fn flatten(events: &[SequenceEvent], start: f64, width: f64) -> Vec<(f64, String)> {
    if events.is_empty() {
        return Vec::new();
    }
    let slot = width / events.len() as f64;
    let mut out = Vec::new();
    for (i, event) in events.iter().enumerate() {
        let when = start + i as f64 * slot;
        match event {
            SequenceEvent::Rest => {}
            SequenceEvent::Value(value) => out.push((when, value.clone())),
            SequenceEvent::Group(inner) => out.extend(flatten(inner, when, slot)),
        }
    }
    out
}

/// Timed event list: each `(when, value)` is scheduled relative to start offset.
///
/// `.start(offset)` only registers ids; callbacks stay silent until
/// `transport.start()` / [`Transport::fire_until`] / later render.
pub struct Part {
    callback: Rc<dyn Fn(f64, &str)>,
    events: Vec<(TimeValue, String)>,
    ids: Vec<u64>,
}

impl Part {
    /// Build a part from `(time, event)` pairs such as `("0:0:0", "C4")`.
    pub fn new<C, I, T, V>(callback: C, events: I) -> Self
    where
        C: Fn(f64, &str) + 'static,
        I: IntoIterator<Item = (T, V)>,
        T: IntoTime,
        V: Into<String>,
    {
        Self {
            callback: Rc::new(callback),
            events: events
                .into_iter()
                .map(|(when, value)| (when.into_time(), value.into()))
                .collect(),
            ids: Vec::new(),
        }
    }

    /// Schedule each event at `offset + to_seconds(when)` on `transport`.
    ///
    /// Replaces any previous attachment (stops first). Returns `self` for
    /// chaining. Accepts [`Transport`] or [`TransportRef`].
    pub fn start(
        &mut self,
        transport: &mut impl SequenceClock,
        offset: impl IntoTime,
    ) -> Result<&mut Self, TransportError> {
        self.stop(transport);
        let base = transport.to_seconds(offset.into_time())?;
        let mut ids = Vec::with_capacity(self.events.len());
        for (when, value) in &self.events {
            let at = match transport.to_seconds(when.clone()) {
                Ok(seconds) => base + seconds,
                Err(err) => {
                    transport.cancel_ids(&ids);
                    return Err(err.into());
                }
            };
            let callback = Rc::clone(&self.callback);
            let value = value.clone();
            match transport.schedule(move |time| callback(time, value.as_str()), at) {
                Ok(id) => ids.push(id),
                Err(err) => {
                    transport.cancel_ids(&ids);
                    return Err(err);
                }
            }
        }
        self.ids = ids;
        Ok(self)
    }

    /// Detach from `transport` by cancelling this part's scheduled ids.
    pub fn stop(&mut self, transport: &mut impl SequenceClock) -> &mut Self {
        transport.cancel_ids(&self.ids);
        self.ids.clear();
        self
    }
}

/// Repeating callback attached to a [`SequenceClock`].
///
/// `.start(offset)` registers one `schedule_repeat`; callbacks stay silent
/// until `transport.start()` / [`Transport::fire_until`] / later render.
pub struct Loop {
    callback: Rc<dyn Fn(f64)>,
    interval: TimeValue,
    ids: Vec<u64>,
}

impl Loop {
    /// Build a loop that fires `callback` every `interval`.
    pub fn new(callback: impl Fn(f64) + 'static, interval: impl IntoTime) -> Self {
        Self {
            callback: Rc::new(callback),
            interval: interval.into_time(),
            ids: Vec::new(),
        }
    }

    /// `schedule_repeat(callback, interval, start_time=offset)` on `transport`.
    ///
    /// Replaces any previous attachment (stops first). Returns `self` for
    /// chaining. Accepts [`Transport`] or [`TransportRef`].
    pub fn start(
        &mut self,
        transport: &mut impl SequenceClock,
        offset: impl IntoTime,
    ) -> Result<&mut Self, TransportError> {
        self.stop(transport);
        let start = transport.to_seconds(offset.into_time())?;
        let interval = transport.to_seconds(self.interval.clone())?;
        let callback = Rc::clone(&self.callback);
        let id = transport.schedule_repeat(move |time| callback(time), interval, start)?;
        self.ids = vec![id];
        Ok(self)
    }

    /// Detach from `transport` by cancelling this loop's scheduled id.
    pub fn stop(&mut self, transport: &mut impl SequenceClock) -> &mut Self {
        transport.cancel_ids(&self.ids);
        self.ids.clear();
        self
    }
}
