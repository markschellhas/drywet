use crate::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};

/// PCM destination shared by [`BufferSink`] and later live backends.
///
/// Methods stay object-safe (`&[f32]`, no generics) so a `dyn Sink` can be
/// added later without changing the musician-facing mix/write path.
pub trait Sink {
    /// Add mono frames at `at_sample`, or at the write cursor when `None`.
    ///
    /// Stereo sinks duplicate each mono sample across channels. Does not
    /// advance the write cursor. Tails sum into existing samples.
    fn mix(&mut self, frames: &[f32], at_sample: Option<usize>);

    /// Mix `frames` at the write cursor and advance it by `frames.len()`.
    fn write(&mut self, frames: &[f32]);

    /// Stop playback. [`BufferSink`] is a no-op.
    fn stop(&mut self);

    /// Close the destination. [`BufferSink`] is a no-op.
    fn close(&mut self);

    /// Output latency in milliseconds (`0` for [`BufferSink`]).
    fn latency_ms(&self) -> u32;

    /// Playhead in sample frames (not interleaved samples).
    fn write_cursor(&self) -> usize;

    /// Whether any mix or write has been accepted.
    fn accepted(&self) -> bool;

    /// Mark the destination as accepted. Default is a no-op.
    ///
    /// [`crate::transport::TransportRef::start`] uses this so BufferSink
    /// reports accepted immediately when the clock starts.
    fn mark_accepted(&mut self) {}
}

/// In-memory expanding f32 buffer for tests and offline render.
///
/// `frames` is a mono sample list. When `channels > 1`, each sample is
/// duplicated across channels. `stop` / `close` do nothing; `latency_ms` is 0.
#[derive(Debug)]
pub struct BufferSink {
    sample_rate: u32,
    channels: u16,
    buf: Vec<f32>,
    write_cursor: usize,
    accepted: bool,
}

impl BufferSink {
    /// Create a sink with the given sample rate (Hz) and channel count.
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            buf: Vec::new(),
            write_cursor: 0,
            accepted: false,
        }
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channel count. Stereo (`2`) duplicates each mono frame.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Interleaved buffer contents (zeros fill gaps before a mix offset).
    pub fn frames(&self) -> &[f32] {
        &self.buf
    }

    /// Add mono frames at `at_sample`, or at the write cursor when `None`.
    pub fn mix(&mut self, frames: &[f32], at_sample: Option<usize>) {
        let at = at_sample.unwrap_or(self.write_cursor);
        self.mix_at(frames, at);
        self.accepted = true;
    }

    /// Mix `frames` at the write cursor and advance it by `frames.len()`.
    pub fn write(&mut self, frames: &[f32]) {
        let start = self.write_cursor;
        self.mix_at(frames, start);
        self.write_cursor = start + frames.len();
        self.accepted = true;
    }

    /// No-op. The buffer stays readable after stop.
    pub fn stop(&mut self) {}

    /// No-op. The buffer stays readable after close.
    pub fn close(&mut self) {}

    /// Always `0` — BufferSink has no device latency.
    pub fn latency_ms(&self) -> u32 {
        0
    }

    /// Playhead in sample frames (not interleaved samples).
    pub fn write_cursor(&self) -> usize {
        self.write_cursor
    }

    /// `true` after the first mix, write, or [`mark_accepted`](Self::mark_accepted).
    pub fn accepted(&self) -> bool {
        self.accepted
    }

    /// Set [`accepted`](Self::accepted) without mixing audio.
    pub fn mark_accepted(&mut self) {
        self.accepted = true;
    }

    /// Clip to `[-1, 1]`, then pack `round(clipped * 32767)` as little-endian i16.
    pub fn to_pcm_s16le(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.buf.len() * 2);
        for &sample in &self.buf {
            let clipped = sample.clamp(-1.0, 1.0);
            let pcm = (clipped * 32767.0).round() as i16;
            out.extend_from_slice(&pcm.to_le_bytes());
        }
        out
    }

    fn mix_at(&mut self, frames: &[f32], at_sample: usize) {
        let channels = usize::from(self.channels);
        let start = at_sample * channels;
        let needed = start + frames.len() * channels;
        if self.buf.len() < needed {
            self.buf.resize(needed, 0.0);
        }
        for (i, &sample) in frames.iter().enumerate() {
            let base = start + i * channels;
            for ch in 0..channels {
                self.buf[base + ch] += sample;
            }
        }
    }
}

impl Default for BufferSink {
    fn default() -> Self {
        Self::new(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS)
    }
}

impl Sink for BufferSink {
    fn mix(&mut self, frames: &[f32], at_sample: Option<usize>) {
        BufferSink::mix(self, frames, at_sample);
    }

    fn write(&mut self, frames: &[f32]) {
        BufferSink::write(self, frames);
    }

    fn stop(&mut self) {
        BufferSink::stop(self);
    }

    fn close(&mut self) {
        BufferSink::close(self);
    }

    fn latency_ms(&self) -> u32 {
        BufferSink::latency_ms(self)
    }

    fn write_cursor(&self) -> usize {
        BufferSink::write_cursor(self)
    }

    fn accepted(&self) -> bool {
        BufferSink::accepted(self)
    }

    fn mark_accepted(&mut self) {
        BufferSink::mark_accepted(self);
    }
}
