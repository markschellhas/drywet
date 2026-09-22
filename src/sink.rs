use std::cell::RefCell;
use std::rc::Rc;

use crate::bus::{mix_mono, validate_name, Bus, BusControl, BusError, BusId, BusTable, MixDest};
use crate::insert::{apply_interleaved, Insert, InsertChain, InsertError};
use crate::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};

pub use crate::sink_device::{DeviceSink, DeviceSinkError};
pub use crate::sink_pipewire::{
    latency_ms_from_quantum, MockStream, PipeWireSink, StreamBackend, DEFAULT_QUANTUM_FRAMES,
};

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

    /// Replace the playback insert chain. Default ignores `inserts`.
    ///
    /// Mix and write stay dry. Object-safe (`Vec<Box<dyn Insert>>`, no generics).
    fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        let _ = inserts;
        Ok(())
    }

    /// Apply the insert chain to a caller-owned buffer. Default is a no-op.
    fn apply_inserts(&mut self, frames: &mut [f32]) {
        let _ = frames;
    }

    /// Create or get an extra named bus. Default is [`BusError::UnknownBus`].
    fn ensure_bus(&mut self, name: &str) -> Result<Bus, BusError> {
        let _ = name;
        Err(BusError::UnknownBus)
    }

    /// Mix dry PCM onto master or a named bus.
    fn mix_on(
        &mut self,
        dest: MixDest,
        frames: &[f32],
        at_sample: Option<usize>,
    ) -> Result<(), BusError> {
        match dest {
            MixDest::Master => {
                self.mix(frames, at_sample);
                Ok(())
            }
            MixDest::Bus(_) => Err(BusError::UnknownBus),
        }
    }

    /// Fold extra buses into a master-dry copy, then run master inserts.
    ///
    /// Default is [`Self::apply_inserts`]. May grow `frames` to cover bus tails.
    fn fold_into(&mut self, frames: &mut Vec<f32>) {
        self.apply_inserts(frames);
    }

    /// Mark the destination as accepted. Default is a no-op.
    ///
    /// [`crate::transport::TransportRef::start`] uses this so BufferSink
    /// reports accepted immediately when the clock starts.
    fn mark_accepted(&mut self) {}

    /// Open the destination clock if the sink has one. Default is a no-op.
    ///
    /// [`crate::sink::PipeWireSink`] opens the persistent stream.
    /// [`BufferSink`] stays silent so the engine can always call this.
    fn start_clock(&mut self) {}

    /// Interleaved buffer contents. Empty for sinks that do not buffer.
    ///
    /// [`crate::transport::TransportRef::render`] copies this after padding.
    fn frames(&self) -> &[f32] {
        &[]
    }
}

#[derive(Debug, Default)]
struct BufferBusData {
    buf: Vec<f32>,
    inserts: InsertChain,
}

struct BufferBusControl {
    buses: Rc<RefCell<BusTable<BufferBusData>>>,
}

impl BusControl for BufferBusControl {
    fn set_inserts(&self, id: BusId, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        if let Some(slot) = self.buses.borrow_mut().get_mut(id) {
            slot.data.inserts.set(inserts)
        } else {
            Ok(())
        }
    }
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
    inserts: InsertChain,
    buses: Rc<RefCell<BusTable<BufferBusData>>>,
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
            inserts: InsertChain::new(),
            buses: Rc::new(RefCell::new(BusTable::new())),
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

    /// Replace the playback insert chain. Mix, write, and [`frames`](Self::frames) stay dry.
    pub fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        self.inserts.set(inserts)
    }

    /// Apply the insert chain to `frames`. Does not rewrite the dry buffer.
    pub fn apply_inserts(&mut self, frames: &mut [f32]) {
        apply_interleaved(&mut self.inserts, frames, self.channels);
    }

    /// Create or get an extra named bus. Same name returns the same bus.
    pub fn ensure_bus(&mut self, name: &str) -> Result<Bus, BusError> {
        validate_name(name)?;
        let id = self.buses.borrow_mut().ensure(name)?;
        Ok(Bus::new(
            id,
            name,
            Rc::new(BufferBusControl {
                buses: Rc::clone(&self.buses),
            }),
        ))
    }

    /// Mix dry PCM onto master or a named bus.
    pub fn mix_on(
        &mut self,
        dest: MixDest,
        frames: &[f32],
        at_sample: Option<usize>,
    ) -> Result<(), BusError> {
        match dest {
            MixDest::Master => {
                BufferSink::mix(self, frames, at_sample);
                Ok(())
            }
            MixDest::Bus(id) => {
                let at = at_sample.unwrap_or(self.write_cursor);
                let mut buses = self.buses.borrow_mut();
                let slot = buses.get_mut(id).ok_or(BusError::UnknownBus)?;
                mix_mono(&mut slot.data.buf, frames, at);
                self.accepted = true;
                Ok(())
            }
        }
    }

    /// Fold extra buses into `frames`, then run master inserts.
    pub fn fold_into(&mut self, frames: &mut Vec<f32>) {
        let channels = usize::from(self.channels.max(1));
        let mut buses = self.buses.borrow_mut();
        let mut max_frames = frames.len() / channels;
        for (_, slot) in buses.iter() {
            max_frames = max_frames.max(slot.data.buf.len());
        }
        let needed = max_frames.saturating_mul(channels);
        if frames.len() < needed {
            frames.resize(needed, 0.0);
        }
        let n_frames = frames.len() / channels;
        for (_, slot) in buses.iter_mut() {
            let mut wet = slot.data.buf.clone();
            if wet.len() < n_frames {
                wet.resize(n_frames, 0.0);
            } else {
                wet.truncate(n_frames);
            }
            slot.data.inserts.process(&mut wet);
            for frame in 0..n_frames {
                let sample = wet[frame];
                let base = frame * channels;
                for ch in 0..channels {
                    frames[base + ch] += sample;
                }
            }
        }
        apply_interleaved(&mut self.inserts, frames, self.channels);
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

    fn frames(&self) -> &[f32] {
        BufferSink::frames(self)
    }

    fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        BufferSink::set_inserts(self, inserts)
    }

    fn apply_inserts(&mut self, frames: &mut [f32]) {
        BufferSink::apply_inserts(self, frames)
    }

    fn ensure_bus(&mut self, name: &str) -> Result<Bus, BusError> {
        BufferSink::ensure_bus(self, name)
    }

    fn mix_on(
        &mut self,
        dest: MixDest,
        frames: &[f32],
        at_sample: Option<usize>,
    ) -> Result<(), BusError> {
        BufferSink::mix_on(self, dest, frames, at_sample)
    }

    fn fold_into(&mut self, frames: &mut Vec<f32>) {
        BufferSink::fold_into(self, frames)
    }
}
