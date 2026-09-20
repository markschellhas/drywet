use crate::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};
use crate::sink::{BufferSink, Sink};
use crate::transport::{Transport, TransportRef};

/// Runtime that owns sample rate, channel count, one [`Sink`], and one [`Transport`].
///
/// Defaults match drywet-py: 44100 Hz, 1 channel, an owned [`BufferSink`].
/// A later PipeWire sink can plug in as `Context<PipeWireSink>`.
#[derive(Debug)]
pub struct Context<S: Sink = BufferSink> {
    sample_rate: u32,
    channels: u16,
    sink: S,
    transport: Transport,
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
        Self {
            sample_rate,
            channels,
            sink,
            transport: Transport::new(),
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
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Mutable access to the owned PCM destination.
    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    /// Handle to the arrangement clock.
    ///
    /// Mutably borrows the sink so [`TransportRef::start`] can mark it
    /// accepted and [`TransportRef::stop`] can stop it without closing.
    pub fn transport(&mut self) -> TransportRef<'_, S> {
        TransportRef::new(&mut self.transport, &mut self.sink)
    }
}

impl Default for Context<BufferSink> {
    fn default() -> Self {
        Self::new()
    }
}
