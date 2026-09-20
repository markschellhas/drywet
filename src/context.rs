use std::cell::{Ref, RefCell, RefMut};

use crate::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};
use crate::sink::{BufferSink, Sink};
use crate::time::{IntoTime, TimeError};
use crate::transport::{
    fire_until_releasing, Transport, TransportError, TransportRef, TransportState,
};

/// Runtime that owns sample rate, channel count, one [`Sink`], and one [`Transport`].
///
/// Defaults match drywet-py: 44100 Hz, 1 channel, an owned [`BufferSink`].
/// A later PipeWire sink can plug in as `Context<PipeWireSink>`.
///
/// The sink and transport sit in [`RefCell`]s so a scheduled callback can
/// mix onto the sink (and read the clock) while `render` / `fire_until`
/// is running.
#[derive(Debug)]
pub struct Context<S: Sink = BufferSink> {
    sample_rate: u32,
    channels: u16,
    sink: RefCell<S>,
    transport: RefCell<Transport>,
}

impl Context<BufferSink> {
    /// 44100 Hz, 1 channel, owned [`BufferSink`].
    pub fn new() -> Self {
        Self::with(
            DEFAULT_SAMPLE_RATE,
            DEFAULT_CHANNELS,
            BufferSink::new(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS),
        )
    }
}

impl<S: Sink> Context<S> {
    /// Take ownership of `sink` at the given rate and channel count.
    pub fn with(sample_rate: u32, channels: u16, sink: S) -> Self {
        let mut transport = Transport::new();
        transport.set_sample_rate(sample_rate);
        Self {
            sample_rate,
            channels,
            sink: RefCell::new(sink),
            transport: RefCell::new(transport),
        }
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channel count.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// The owned PCM destination.
    pub fn sink(&self) -> Ref<'_, S> {
        self.sink.borrow()
    }

    /// Mutable access to the owned PCM destination.
    pub fn sink_mut(&self) -> RefMut<'_, S> {
        self.sink.borrow_mut()
    }

    /// Handle to the arrangement clock.
    ///
    /// Borrows are short-lived [`RefCell`] guards so a callback can mix
    /// without needing `&mut Context`.
    pub fn transport(&self) -> TransportRef<'_, S> {
        TransportRef::new(&self.transport, &self.sink)
    }

    /// Convert a musical time using the current transport clock.
    ///
    /// Safe to call from a schedule callback during [`Self::render`].
    pub fn to_seconds(&self, value: impl IntoTime) -> Result<f64, TimeError> {
        self.transport.borrow().to_seconds(value)
    }

    /// Start if needed, fire scheduled events through `duration`, pad the
    /// sink, and return a copy of the sink frames.
    ///
    /// Port of drywet-py `Transport.render`. The sink is borrowed only for
    /// start/pad — not while callbacks run — so instruments can mix.
    pub fn render(&self, duration: impl IntoTime) -> Result<Vec<f32>, TransportError> {
        if self.transport.borrow().state() != TransportState::Started {
            if self.transport.borrow_mut().start() {
                self.sink.borrow_mut().mark_accepted();
            }
        }
        let until = self.transport.borrow().to_seconds(duration)?;
        fire_until_releasing(&self.transport, until)?;
        let needed = (until * f64::from(self.sample_rate)).round() as usize;
        let mut sink = self.sink.borrow_mut();
        let cursor = sink.write_cursor();
        if cursor < needed {
            sink.write(&vec![0.0; needed - cursor]);
        }
        Ok(sink.frames().to_vec())
    }
}

impl Default for Context<BufferSink> {
    fn default() -> Self {
        Self::new()
    }
}
