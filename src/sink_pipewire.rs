//! Persistent callback sink with a PipeWire-style process pull.
//!
//! Default construction uses an in-process [`MockStream`] so `cargo test` and
//! `Context` can own a sink without libpipewire or a sound server. Notes land
//! in a preallocated slot + command table on the control thread; the process
//! callback only copies and sums into a caller-provided output buffer.

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::bus::{validate_name, Bus, BusControl, BusError, BusId, BusTable, MixDest};
use crate::insert::{apply_interleaved, Insert, InsertChain, InsertError, CHUNK_FRAMES};
use crate::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE, MAX_BUSES};

/// Default period size in sample frames when no backend quantum is injected.
pub const DEFAULT_QUANTUM_FRAMES: usize = 256;

/// Mono frames stored in one preallocated mix slot.
const SLOT_FRAMES: usize = 4096;

/// Command / slot table size. Control-thread enqueue never grows this.
const SLOT_COUNT: usize = 64;

/// Negotiated stream latency from a period size: `floor(quantum / rate * 1000)`.
///
/// 256 frames at 44100 Hz is 5 ms, not a hardcoded 80.
pub fn latency_ms_from_quantum(sample_rate: u32, quantum_frames: usize) -> u32 {
    if sample_rate == 0 {
        0
    } else {
        (quantum_frames as u64 * 1000 / u64::from(sample_rate)) as u32
    }
}

/// Injected output stream used by [`PipeWireSink`] and its tests.
///
/// `process` is the audio-thread hook: it must not allocate, format, or do I/O.
/// Mixing into the device buffer happens in [`PipeWireSink::process`] before
/// this is called so a mock can record the filled period.
pub trait StreamBackend {
    /// Negotiated period size in sample frames.
    fn quantum_frames(&self) -> usize;

    /// Stream latency derived from [`Self::quantum_frames`] and the sample rate.
    fn latency_ms(&self) -> u32;

    /// Open the one persistent stream. Idempotent: already-open is a no-op.
    fn open(&mut self);

    /// Tear the stream down.
    fn close(&mut self);

    /// Whether the persistent stream is currently open.
    fn is_open(&self) -> bool;

    /// How many times this backend transitioned from closed to open.
    fn open_count(&self) -> u32;

    /// Recorded subprocess names. The default in-process backend never spawns.
    fn spawn_attempts(&self) -> &[String];

    /// Number of process-callback invocations recorded by this backend.
    fn process_calls(&self) -> usize;

    /// Audio callback tail. `output` is already mixed; do not allocate.
    fn process(&mut self, output: &mut [f32]);
}

/// In-process stream: records lifecycle and callback calls, never shells out.
///
/// Latency is the injected quantum converted at the sink sample rate
/// (256 frames @ 44100 Hz → 5 ms).
#[derive(Debug)]
pub struct MockStream {
    sample_rate: u32,
    quantum_frames: usize,
    open: bool,
    open_count: u32,
    spawn_attempts: Vec<String>,
    process_calls: usize,
    process_samples: usize,
}

impl MockStream {
    /// Build a mock stream with an explicit period size in frames.
    pub fn new(sample_rate: u32, quantum_frames: usize) -> Self {
        Self {
            sample_rate,
            quantum_frames,
            open: false,
            open_count: 0,
            spawn_attempts: Vec::new(),
            process_calls: 0,
            process_samples: 0,
        }
    }

    /// Sample rate used to convert [`Self::quantum_frames`] into milliseconds.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Interleaved samples processed across all callback invocations.
    pub fn process_samples(&self) -> usize {
        self.process_samples
    }
}

impl Default for MockStream {
    fn default() -> Self {
        Self::new(DEFAULT_SAMPLE_RATE, DEFAULT_QUANTUM_FRAMES)
    }
}

impl StreamBackend for MockStream {
    fn quantum_frames(&self) -> usize {
        self.quantum_frames
    }

    fn latency_ms(&self) -> u32 {
        latency_ms_from_quantum(self.sample_rate, self.quantum_frames)
    }

    fn open(&mut self) {
        if !self.open {
            self.open = true;
            self.open_count = self.open_count.saturating_add(1);
        }
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn open_count(&self) -> u32 {
        self.open_count
    }

    fn spawn_attempts(&self) -> &[String] {
        &self.spawn_attempts
    }

    fn process_calls(&self) -> usize {
        self.process_calls
    }

    fn process(&mut self, output: &mut [f32]) {
        self.process_calls = self.process_calls.saturating_add(1);
        self.process_samples = self.process_samples.saturating_add(output.len());
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct MixCmd {
    at: usize,
    len: usize,
    slot: usize,
    dest: MixDest,
}

struct PipeWireBusControl {
    mix: Arc<Mutex<MixStorage>>,
}

impl BusControl for PipeWireBusControl {
    fn set_inserts(&self, id: BusId, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        let mut mix = self
            .mix
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(slot) = mix.buses.get_mut(id) {
            slot.data.set(inserts)
        } else {
            Ok(())
        }
    }
}

/// Fixed-capacity mix table. Enqueue copies PCM into a slot; render only reads.
///
/// Bound: [`SLOT_COUNT`] (64) slots × [`SLOT_FRAMES`] (4096) mono frames, allocated
/// once in [`Self::new`]. No heap after init. When every slot still holds a live
/// command (`cmd_end > playback`), [`Self::enqueue`] drops the incoming chunk
/// rather than overwriting audio that has not been rendered yet.
#[derive(Debug)]
struct MixStorage {
    slots: Box<[f32]>,
    cmds: [MixCmd; SLOT_COUNT],
    occupied: [bool; SLOT_COUNT],
    playback: usize,
    inserts: InsertChain,
    buses: BusTable<InsertChain>,
}

impl MixStorage {
    fn new() -> Self {
        Self {
            slots: vec![0.0f32; SLOT_COUNT * SLOT_FRAMES].into_boxed_slice(),
            cmds: [MixCmd::default(); SLOT_COUNT],
            occupied: [false; SLOT_COUNT],
            playback: 0,
            inserts: InsertChain::new(),
            buses: BusTable::new(),
        }
    }

    /// Free index, or a fully-consumed slot (`cmd_end <= playback`). Never steals live audio.
    fn alloc_slot(&mut self) -> Option<usize> {
        for i in 0..SLOT_COUNT {
            if !self.occupied[i] {
                return Some(i);
            }
        }
        for i in 0..SLOT_COUNT {
            if !self.occupied[i] {
                continue;
            }
            let cmd_end = self.cmds[i].at.saturating_add(self.cmds[i].len);
            if cmd_end <= self.playback {
                self.occupied[i] = false;
                return Some(i);
            }
        }
        None
    }

    fn enqueue(&mut self, frames: &[f32], at: usize, dest: MixDest) {
        let mut offset = 0;
        while offset < frames.len() {
            let n = (frames.len() - offset).min(SLOT_FRAMES);
            let Some(slot) = self.alloc_slot() else {
                return;
            };
            let start = slot * SLOT_FRAMES;
            self.slots[start..start + n].copy_from_slice(&frames[offset..offset + n]);
            self.cmds[slot] = MixCmd {
                at: at.saturating_add(offset),
                len: n,
                slot,
                dest,
            };
            self.occupied[slot] = true;
            offset += n;
        }
    }

    /// Sum overlapping slot PCM per destination, fold buses, then master inserts.
    ///
    /// No heap, no I/O, no formatting. Processes in [`CHUNK_FRAMES`] stacks.
    fn render(&mut self, output: &mut [f32], channels: usize) {
        for sample in output.iter_mut() {
            *sample = 0.0;
        }
        if channels == 0 {
            return;
        }
        let n_frames = output.len() / channels;
        let start = self.playback;
        let end = start.saturating_add(n_frames);

        let mut frame = 0;
        while frame < n_frames {
            let n = (n_frames - frame).min(CHUNK_FRAMES);
            let chunk_start = start.saturating_add(frame);
            let chunk_end = chunk_start.saturating_add(n);
            let mut master = [0.0f32; CHUNK_FRAMES];
            let mut bus_blocks = [[0.0f32; CHUNK_FRAMES]; MAX_BUSES];

            for i in 0..SLOT_COUNT {
                if !self.occupied[i] {
                    continue;
                }
                let cmd = self.cmds[i];
                let cmd_end = cmd.at.saturating_add(cmd.len);
                if cmd_end <= chunk_start || cmd.at >= chunk_end {
                    continue;
                }
                let mix_from = cmd.at.max(chunk_start);
                let mix_to = cmd_end.min(chunk_end);
                let slot_base = cmd.slot * SLOT_FRAMES;
                let dest = match cmd.dest {
                    MixDest::Master => &mut master[..],
                    MixDest::Bus(id) => &mut bus_blocks[id.index()][..],
                };
                for sample_frame in mix_from..mix_to {
                    dest[sample_frame - chunk_start] +=
                        self.slots[slot_base + (sample_frame - cmd.at)];
                }
            }

            for (id, slot) in self.buses.iter_mut() {
                let block = &mut bus_blocks[id.index()][..n];
                slot.data.process(block);
                for i in 0..n {
                    master[i] += block[i];
                }
            }
            self.inserts.process(&mut master[..n]);

            for i in 0..n {
                let sample = master[i];
                let base = (frame + i) * channels;
                for ch in 0..channels {
                    output[base + ch] = sample;
                }
            }
            frame += n;
        }

        for i in 0..SLOT_COUNT {
            if !self.occupied[i] {
                continue;
            }
            let cmd_end = self.cmds[i].at.saturating_add(self.cmds[i].len);
            if cmd_end <= end {
                self.occupied[i] = false;
            }
        }

        self.playback = end;
    }
}

/// One persistent PipeWire-style callback sink.
///
/// `new` installs an in-process [`MockStream`]. Tests inject a backend with
/// [`Self::with_backend`]. `start_clock` opens the single stream (idempotent);
/// `stop` leaves it open; `close` tears it down. The process callback pulls
/// mixed PCM from preallocated storage — it never spawns `pw-cat` / `pw-play`
/// / `paplay` / `aplay`.
///
/// Mix storage is 64 slots × 4096 frames, allocated at construction (no heap
/// after init). A mix/write that arrives while every slot is still live
/// (`cmd_end > playback`) is dropped; `write_cursor` and `accepted` still update.
#[derive(Debug)]
pub struct PipeWireSink<B: StreamBackend = MockStream> {
    sample_rate: u32,
    channels: u16,
    write_cursor: usize,
    accepted: bool,
    mix: Arc<Mutex<MixStorage>>,
    backend: B,
}

impl PipeWireSink<MockStream> {
    /// In-process callback sink at `sample_rate` Hz and `channels` (no hardware).
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self::with_backend(
            sample_rate,
            channels,
            MockStream::new(sample_rate, DEFAULT_QUANTUM_FRAMES),
        )
    }
}

impl Default for PipeWireSink<MockStream> {
    fn default() -> Self {
        Self::new(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS)
    }
}

impl<B: StreamBackend> PipeWireSink<B> {
    /// Own `backend` as the single persistent stream.
    pub fn with_backend(sample_rate: u32, channels: u16, backend: B) -> Self {
        Self {
            sample_rate,
            channels,
            write_cursor: 0,
            accepted: false,
            mix: Arc::new(Mutex::new(MixStorage::new())),
            backend,
        }
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channel count. Stereo (`2`) duplicates each mono frame into the callback.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// The injected stream backend (quantum, open state, spawn log).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable backend — tests drive `is_open` / `open_count` after lifecycle calls.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Open the one persistent stream if needed and mark accepted.
    ///
    /// A second call does not open a second stream.
    pub fn start_clock(&mut self) {
        if !self.backend.is_open() {
            self.backend.open();
        }
        self.accepted = true;
    }

    /// Audio callback: mix preallocated slots into `output`, then notify the backend.
    ///
    /// `output` is interleaved f32. This path does not allocate, write files,
    /// or emit NDJSON. When the clock is not running, tests (or a later
    /// callback) still drive this to emit queued PCM.
    pub fn process(&mut self, output: &mut [f32]) {
        let channels = usize::from(self.channels);
        match self.mix.try_lock() {
            Ok(mut mix) => mix.render(output, channels),
            Err(_) => {
                for sample in output.iter_mut() {
                    *sample = 0.0;
                }
            }
        }
        self.backend.process(output);
    }

    /// Add mono frames at `at_sample`, or at the write cursor when `None`.
    pub fn mix(&mut self, frames: &[f32], at_sample: Option<usize>) {
        let at = at_sample.unwrap_or(self.write_cursor);
        self.enqueue(frames, at);
        self.accepted = true;
    }

    /// Mix `frames` at the write cursor and advance it by `frames.len()`.
    ///
    /// Deposits into preallocated storage. When the stream is open the
    /// callback pulls; this never pushes a second burst to the device.
    /// When the clock is not running the frames stay queued for a later
    /// [`Self::process`].
    pub fn write(&mut self, frames: &[f32]) {
        let start = self.write_cursor;
        self.enqueue(frames, start);
        self.write_cursor = start.saturating_add(frames.len());
        self.accepted = true;
    }

    /// Keep the stream open. Silence / the callback clock can continue.
    pub fn stop(&mut self) {}

    /// Tear the persistent stream down.
    pub fn close(&mut self) {
        self.backend.close();
    }

    /// Negotiated stream latency from the backend quantum — never a hardcoded 80.
    pub fn latency_ms(&self) -> u32 {
        self.backend.latency_ms()
    }

    /// Playhead in sample frames (not interleaved samples).
    pub fn write_cursor(&self) -> usize {
        self.write_cursor
    }

    /// `true` after mix, write, or [`start_clock`](Self::start_clock) / mark.
    pub fn accepted(&self) -> bool {
        self.accepted
    }

    /// Set [`accepted`](Self::accepted) without mixing audio.
    pub fn mark_accepted(&mut self) {
        self.accepted = true;
    }

    /// Callback sinks do not keep an offline buffer.
    pub fn frames(&self) -> &[f32] {
        &[]
    }

    fn enqueue(&mut self, frames: &[f32], at: usize) {
        if frames.is_empty() {
            return;
        }
        self.mix
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .enqueue(frames, at, MixDest::Master);
    }

    /// Create or get an extra named bus. Same name returns the same bus.
    pub fn ensure_bus(&mut self, name: &str) -> Result<Bus, BusError> {
        validate_name(name)?;
        let id = self
            .mix
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .buses
            .ensure(name)?;
        Ok(Bus::new(
            id,
            name,
            Rc::new(PipeWireBusControl {
                mix: Arc::clone(&self.mix),
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
                PipeWireSink::mix(self, frames, at_sample);
                Ok(())
            }
            MixDest::Bus(id) => {
                let at = at_sample.unwrap_or(self.write_cursor);
                {
                    let mut mix = self
                        .mix
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if mix.buses.get(id).is_none() {
                        return Err(BusError::UnknownBus);
                    }
                    if !frames.is_empty() {
                        mix.enqueue(frames, at, MixDest::Bus(id));
                    }
                }
                self.accepted = true;
                Ok(())
            }
        }
    }

    /// Replace the playback insert chain. Mix and enqueue stay dry.
    pub fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        self.mix
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .inserts
            .set(inserts)
    }

    /// Apply the insert chain to `frames`. Does not rewrite queued slots.
    pub fn apply_inserts(&mut self, frames: &mut [f32]) {
        let mut mix = self
            .mix
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        apply_interleaved(&mut mix.inserts, frames, self.channels);
    }
}

impl<B: StreamBackend> crate::sink::Sink for PipeWireSink<B> {
    fn mix(&mut self, frames: &[f32], at_sample: Option<usize>) {
        PipeWireSink::mix(self, frames, at_sample);
    }

    fn write(&mut self, frames: &[f32]) {
        PipeWireSink::write(self, frames);
    }

    fn stop(&mut self) {
        PipeWireSink::stop(self);
    }

    fn close(&mut self) {
        PipeWireSink::close(self);
    }

    fn latency_ms(&self) -> u32 {
        PipeWireSink::latency_ms(self)
    }

    fn write_cursor(&self) -> usize {
        PipeWireSink::write_cursor(self)
    }

    fn accepted(&self) -> bool {
        PipeWireSink::accepted(self)
    }

    fn mark_accepted(&mut self) {
        PipeWireSink::mark_accepted(self);
    }

    fn start_clock(&mut self) {
        PipeWireSink::start_clock(self);
    }

    fn frames(&self) -> &[f32] {
        PipeWireSink::frames(self)
    }

    fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        PipeWireSink::set_inserts(self, inserts)
    }

    fn apply_inserts(&mut self, frames: &mut [f32]) {
        PipeWireSink::apply_inserts(self, frames)
    }

    fn ensure_bus(&mut self, name: &str) -> Result<Bus, BusError> {
        PipeWireSink::ensure_bus(self, name)
    }

    fn mix_on(
        &mut self,
        dest: MixDest,
        frames: &[f32],
        at_sample: Option<usize>,
    ) -> Result<(), BusError> {
        PipeWireSink::mix_on(self, dest, frames, at_sample)
    }
}
