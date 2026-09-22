use std::collections::HashMap;
use std::error::Error;
use std::f64::consts::PI;
use std::fmt;
use std::fs;
use std::path::Path;

use crate::bus::{Bus, BusError, MixDest};
use crate::context::Context;
use crate::limits::DEFAULT_MAX_VOICES;
use crate::pitch::{midi_to_hz, IntoNote, PitchError};
use crate::sink::Sink;
use crate::time::{IntoTime, TimeError, TimeValue};

const HARMONICS: [f64; 4] = [1.0, 0.35, 0.18, 0.08];
const GAIN: f64 = 0.18;
const ATTACK_SECONDS: f64 = 0.01;
const RELEASE_SECONDS: f64 = 0.05;
const TRIGGER_ATTACK_SECONDS: f64 = 1.0;

/// Failure triggering a voice (polyphony cap, unknown drum, bad note, or bad time).
#[derive(Debug, Clone, PartialEq)]
pub enum InstrumentError {
    /// [`Synth::trigger_attack`] or [`Sampler::trigger_attack`] would exceed the voice cap.
    VoiceLimitExceeded,
    /// [`Drum::trigger`] name was not kick, snare, hat, or a hat alias.
    UnknownDrum(String),
    /// Note-name or MIDI conversion failed.
    Pitch(PitchError),
    /// Duration or start-time conversion failed.
    Time(TimeError),
    /// WAV was missing, unreadable, or not 16-bit PCM.
    InvalidWav(String),
    /// [`Sampler`] trigger ran with an empty sample map.
    EmptySampler,
    /// Mix destination was not a live bus on this Context.
    Bus(BusError),
}

impl fmt::Display for InstrumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstrumentError::VoiceLimitExceeded => write!(f, "voice limit exceeded"),
            InstrumentError::UnknownDrum(name) => write!(f, "unknown drum: {name:?}"),
            InstrumentError::Pitch(err) => write!(f, "{err}"),
            InstrumentError::Time(err) => write!(f, "{err}"),
            InstrumentError::InvalidWav(msg) => write!(f, "{msg}"),
            InstrumentError::EmptySampler => write!(f, "sampler has no samples"),
            InstrumentError::Bus(err) => write!(f, "{err}"),
        }
    }
}

impl Error for InstrumentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            InstrumentError::Pitch(err) => Some(err),
            InstrumentError::Time(err) => Some(err),
            InstrumentError::Bus(err) => Some(err),
            InstrumentError::VoiceLimitExceeded
            | InstrumentError::UnknownDrum(_)
            | InstrumentError::InvalidWav(_)
            | InstrumentError::EmptySampler => None,
        }
    }
}

impl From<PitchError> for InstrumentError {
    fn from(err: PitchError) -> Self {
        InstrumentError::Pitch(err)
    }
}

impl From<TimeError> for InstrumentError {
    fn from(err: TimeError) -> Self {
        InstrumentError::Time(err)
    }
}

impl From<BusError> for InstrumentError {
    fn from(err: BusError) -> Self {
        InstrumentError::Bus(err)
    }
}

/// One-shot additive voice. No velocity in v1.
///
/// Mixes rendered PCM onto the context sink. `time = None` uses the sink
/// write cursor (live hits while transport is running). Held live voices
/// are out of scope for this port.
#[derive(Debug)]
pub struct Synth {
    sample_rate: u32,
    max_voices: u32,
    active_voices: u32,
}

impl Synth {
    /// [`DEFAULT_MAX_VOICES`] at `ctx.sample_rate()`.
    pub fn new<S: Sink>(ctx: &Context<S>) -> Self {
        Self::with_max_voices(ctx, DEFAULT_MAX_VOICES)
    }

    /// Polyphonic cap `max_voices` at `ctx.sample_rate()`.
    pub fn with_max_voices<S: Sink>(ctx: &Context<S>, max_voices: u32) -> Self {
        Self {
            sample_rate: ctx.sample_rate(),
            max_voices,
            active_voices: 0,
        }
    }

    /// Voices currently counted toward the polyphony cap.
    pub fn active_voices(&self) -> u32 {
        self.active_voices
    }

    /// Render `duration` of `note` at `time`, then release the voice.
    ///
    /// Duration and time convert via [`crate::Context::to_seconds`].
    /// `time = None` mixes at the write cursor.
    pub fn trigger_attack_release<S, N, D>(
        &mut self,
        ctx: &Context<S>,
        note: N,
        duration: D,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
        D: IntoTime,
    {
        self.acquire()?;
        let result = self.mix_note(ctx, MixDest::Master, note, Some(duration), time);
        self.release_voice();
        result.map(|()| self)
    }

    /// [`Self::trigger_attack_release`] mixing onto `bus` instead of master.
    pub fn trigger_attack_release_on<S, N, D>(
        &mut self,
        ctx: &Context<S>,
        bus: &Bus,
        note: N,
        duration: D,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
        D: IntoTime,
    {
        self.acquire()?;
        let result = self.mix_note(ctx, MixDest::Bus(bus.id()), note, Some(duration), time);
        self.release_voice();
        result.map(|()| self)
    }

    /// Acquire a voice and mix a 1.0s additive note (heritage one-shot).
    pub fn trigger_attack<S, N>(
        &mut self,
        ctx: &Context<S>,
        note: N,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
    {
        self.acquire()?;
        match self.mix_note(ctx, MixDest::Master, note, None::<f64>, time) {
            Ok(()) => Ok(self),
            Err(err) => {
                self.release_voice();
                Err(err)
            }
        }
    }

    /// [`Self::trigger_attack`] mixing onto `bus` instead of master.
    pub fn trigger_attack_on<S, N>(
        &mut self,
        ctx: &Context<S>,
        bus: &Bus,
        note: N,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
    {
        self.acquire()?;
        match self.mix_note(ctx, MixDest::Bus(bus.id()), note, None::<f64>, time) {
            Ok(()) => Ok(self),
            Err(err) => {
                self.release_voice();
                Err(err)
            }
        }
    }

    /// Decrement the voice count. v1 one-shot does not silence already-mixed PCM.
    pub fn trigger_release(
        &mut self,
        note: impl IntoNote,
        _time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        let _midi = note.into_midi()?;
        self.release_voice();
        Ok(self)
    }

    /// [`Self::trigger_release`]. Destination does not rewrite already-mixed PCM.
    pub fn trigger_release_on(
        &mut self,
        _bus: &Bus,
        note: impl IntoNote,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        self.trigger_release(note, time)
    }

    /// Drop every counted voice. v1 one-shot does not silence already-mixed PCM.
    pub fn release_all(&mut self, _time: Option<TimeValue>) -> &mut Self {
        self.active_voices = 0;
        self
    }

    fn acquire(&mut self) -> Result<(), InstrumentError> {
        if self.active_voices >= self.max_voices {
            return Err(InstrumentError::VoiceLimitExceeded);
        }
        self.active_voices += 1;
        Ok(())
    }

    fn release_voice(&mut self) {
        if self.active_voices > 0 {
            self.active_voices -= 1;
        }
    }

    fn mix_note<S, N, D>(
        &self,
        ctx: &Context<S>,
        dest: MixDest,
        note: N,
        duration: Option<D>,
        time: Option<TimeValue>,
    ) -> Result<(), InstrumentError>
    where
        S: Sink,
        N: IntoNote,
        D: IntoTime,
    {
        let freq = midi_to_hz(note)?;
        let duration_s = match duration {
            Some(value) => ctx.to_seconds(value)?,
            None => TRIGGER_ATTACK_SECONDS,
        };
        let frames = render_additive(freq, duration_s, self.sample_rate);
        mix_at_time_on(ctx, dest, &frames, time)
    }
}

/// One-shot kick / snare / hat transients mixed onto the context sink.
///
/// Names are `kick`, `snare`, and `hat` (`hi-hat` / `hihat` alias hat).
/// `time = None` mixes at the write cursor. Hits are short; release is a no-op.
#[derive(Debug)]
pub struct Drum {
    sample_rate: u32,
}

impl Drum {
    /// Sample rate from `ctx.sample_rate()`.
    pub fn new<S: Sink>(ctx: &Context<S>) -> Self {
        Self {
            sample_rate: ctx.sample_rate(),
        }
    }

    /// Sixteenth-note steps in one bar: `numerator * 16 / denominator`.
    pub fn steps_per_bar(time_signature: (u32, u32)) -> u32 {
        let (numerator, denominator) = time_signature;
        numerator * 16 / denominator
    }

    /// Mix a named drum hit at `time`.
    pub fn trigger<S: Sink>(
        &mut self,
        ctx: &Context<S>,
        name: &str,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        let frames = render_drum(name, self.sample_rate)?;
        mix_at_time_on(ctx, MixDest::Master, &frames, time)?;
        Ok(self)
    }

    /// [`Self::trigger`] mixing onto `bus` instead of master.
    pub fn trigger_on<S: Sink>(
        &mut self,
        ctx: &Context<S>,
        bus: &Bus,
        name: &str,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        let frames = render_drum(name, self.sample_rate)?;
        mix_at_time_on(ctx, MixDest::Bus(bus.id()), &frames, time)?;
        Ok(self)
    }

    /// Alias of [`Drum::trigger`].
    pub fn trigger_attack<S: Sink>(
        &mut self,
        ctx: &Context<S>,
        name: &str,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        self.trigger(ctx, name, time)
    }

    /// Alias of [`Drum::trigger_on`].
    pub fn trigger_attack_on<S: Sink>(
        &mut self,
        ctx: &Context<S>,
        bus: &Bus,
        name: &str,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        self.trigger_on(ctx, bus, name, time)
    }

    /// No-op. Drum hits are one-shot transients.
    pub fn trigger_release(&mut self, _name: &str, _time: Option<TimeValue>) -> &mut Self {
        self
    }

    /// No-op. Drum hits are one-shot transients.
    pub fn trigger_release_on(
        &mut self,
        _bus: &Bus,
        name: &str,
        time: Option<TimeValue>,
    ) -> &mut Self {
        self.trigger_release(name, time)
    }

    /// Alias of [`Drum::trigger`]. `duration` is ignored.
    pub fn trigger_attack_release<S, D>(
        &mut self,
        ctx: &Context<S>,
        name: &str,
        _duration: D,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        D: IntoTime,
    {
        self.trigger(ctx, name, time)
    }

    /// Alias of [`Drum::trigger_on`]. `duration` is ignored.
    pub fn trigger_attack_release_on<S, D>(
        &mut self,
        ctx: &Context<S>,
        bus: &Bus,
        name: &str,
        _duration: D,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        D: IntoTime,
    {
        self.trigger_on(ctx, bus, name, time)
    }

    /// No-op. Drum hits are one-shot transients.
    pub fn release_all(&mut self, _time: Option<TimeValue>) -> &mut Self {
        self
    }
}

/// Note/MIDI → WAV map with nearest-sample pitch fill and a polyphony cap.
///
/// Missing pitches resample the nearest loaded sample by the semitone ratio.
/// Files are resampled on load when their rate differs from the context.
/// `loop_flag` is stored for API parity; the held-loop mixer is out of scope.
#[derive(Debug)]
pub struct Sampler {
    sample_rate: u32,
    max_voices: u32,
    active: u32,
    samples: HashMap<u8, Vec<f32>>,
    loop_flag: bool,
}

impl Sampler {
    /// Empty map, [`DEFAULT_MAX_VOICES`], at `ctx.sample_rate()`.
    pub fn new<S: Sink>(ctx: &Context<S>) -> Self {
        Self::with_max_voices(ctx, DEFAULT_MAX_VOICES)
    }

    /// Empty map with polyphonic cap `max_voices`.
    pub fn with_max_voices<S: Sink>(ctx: &Context<S>, max_voices: u32) -> Self {
        Self {
            sample_rate: ctx.sample_rate(),
            max_voices,
            active: 0,
            samples: HashMap::new(),
            loop_flag: false,
        }
    }

    /// Load each `(note, path)` pair at `max_voices`.
    pub fn with_map<S, I, N, P>(
        ctx: &Context<S>,
        urls: I,
        max_voices: u32,
    ) -> Result<Self, InstrumentError>
    where
        S: Sink,
        I: IntoIterator<Item = (N, P)>,
        N: IntoNote,
        P: AsRef<Path>,
    {
        let mut sampler = Self::with_max_voices(ctx, max_voices);
        for (note, path) in urls {
            sampler.add(note, path)?;
        }
        Ok(sampler)
    }

    /// Load `C4.wav`-style filenames from `dir` (`^([A-Ga-g][#b]?\\d+)\\.wav$`).
    pub fn from_directory<S: Sink>(
        ctx: &Context<S>,
        dir: impl AsRef<Path>,
    ) -> Result<Self, InstrumentError> {
        let mut sampler = Self::new(ctx);
        let entries = fs::read_dir(dir.as_ref()).map_err(wav_io_error)?;
        for entry in entries {
            let entry = entry.map_err(wav_io_error)?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };
            if let Some(note) = wav_note_stem(name) {
                sampler.add(note, &path)?;
            }
        }
        Ok(sampler)
    }

    /// Voices currently counted toward the polyphony cap.
    pub fn active_voices(&self) -> u32 {
        self.active
    }

    /// Whether `loop` was requested. The held-loop mixer is not implemented.
    pub fn loop_flag(&self) -> bool {
        self.loop_flag
    }

    /// MIDI keys currently mapped to loaded PCM.
    pub fn samples(&self) -> &HashMap<u8, Vec<f32>> {
        &self.samples
    }

    /// Decode `path` as 16-bit PCM and store it at `note`.
    pub fn add(
        &mut self,
        note: impl IntoNote,
        path: impl AsRef<Path>,
    ) -> Result<&mut Self, InstrumentError> {
        let midi = note.into_midi()?;
        let frames = load_wav(path.as_ref(), self.sample_rate)?;
        self.samples.insert(midi, frames);
        Ok(self)
    }

    /// Mix the full (pitch-filled) sample at `time` and keep the voice.
    pub fn trigger_attack<S, N>(
        &mut self,
        ctx: &Context<S>,
        note: N,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
    {
        self.acquire()?;
        match self.mix_sample(ctx, MixDest::Master, note, None::<f64>, time) {
            Ok(()) => Ok(self),
            Err(err) => {
                self.release_voice();
                Err(err)
            }
        }
    }

    /// [`Self::trigger_attack`] mixing onto `bus` instead of master.
    pub fn trigger_attack_on<S, N>(
        &mut self,
        ctx: &Context<S>,
        bus: &Bus,
        note: N,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
    {
        self.acquire()?;
        match self.mix_sample(ctx, MixDest::Bus(bus.id()), note, None::<f64>, time) {
            Ok(()) => Ok(self),
            Err(err) => {
                self.release_voice();
                Err(err)
            }
        }
    }

    /// Mix up to `duration` of the (pitch-filled) sample, then release.
    pub fn trigger_attack_release<S, N, D>(
        &mut self,
        ctx: &Context<S>,
        note: N,
        duration: D,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
        D: IntoTime,
    {
        self.acquire()?;
        let result = self.mix_sample(ctx, MixDest::Master, note, Some(duration), time);
        self.release_voice();
        result.map(|()| self)
    }

    /// [`Self::trigger_attack_release`] mixing onto `bus` instead of master.
    pub fn trigger_attack_release_on<S, N, D>(
        &mut self,
        ctx: &Context<S>,
        bus: &Bus,
        note: N,
        duration: D,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
        D: IntoTime,
    {
        self.acquire()?;
        let result = self.mix_sample(ctx, MixDest::Bus(bus.id()), note, Some(duration), time);
        self.release_voice();
        result.map(|()| self)
    }

    /// Decrement the voice count. v1 one-shot does not silence already-mixed PCM.
    pub fn trigger_release(
        &mut self,
        note: impl IntoNote,
        _time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        let _midi = note.into_midi()?;
        self.release_voice();
        Ok(self)
    }

    /// [`Self::trigger_release`]. Destination does not rewrite already-mixed PCM.
    pub fn trigger_release_on(
        &mut self,
        _bus: &Bus,
        note: impl IntoNote,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        self.trigger_release(note, time)
    }

    /// Drop every counted voice. v1 one-shot does not silence already-mixed PCM.
    pub fn release_all(&mut self, _time: Option<TimeValue>) -> &mut Self {
        self.active = 0;
        self
    }

    fn acquire(&mut self) -> Result<(), InstrumentError> {
        if self.active >= self.max_voices {
            return Err(InstrumentError::VoiceLimitExceeded);
        }
        self.active += 1;
        Ok(())
    }

    fn release_voice(&mut self) {
        if self.active > 0 {
            self.active -= 1;
        }
    }

    fn mix_sample<S, N, D>(
        &self,
        ctx: &Context<S>,
        dest: MixDest,
        note: N,
        duration: Option<D>,
        time: Option<TimeValue>,
    ) -> Result<(), InstrumentError>
    where
        S: Sink,
        N: IntoNote,
        D: IntoTime,
    {
        let midi = note.into_midi()?;
        let mut frames = self.nearest(midi)?;
        if let Some(value) = duration {
            let duration_s = ctx.to_seconds(value)?;
            let n = (duration_s * f64::from(self.sample_rate)).round() as usize;
            if n < frames.len() {
                frames.truncate(n);
            }
        }
        mix_at_time_on(ctx, dest, &frames, time)
    }

    fn nearest(&self, midi: u8) -> Result<Vec<f32>, InstrumentError> {
        if self.samples.is_empty() {
            return Err(InstrumentError::EmptySampler);
        }
        if let Some(frames) = self.samples.get(&midi) {
            return Ok(frames.clone());
        }
        let nearest = self
            .samples
            .keys()
            .copied()
            .min_by_key(|key| (key.abs_diff(midi), *key))
            .expect("samples is non-empty");
        let semitones = i32::from(midi) - i32::from(nearest);
        Ok(pitch_shift(&self.samples[&nearest], semitones))
    }
}

/// Mix `frames` onto `dest` at `time`, or at the write cursor when `time` is `None`.
fn mix_at_time_on<S: Sink>(
    ctx: &Context<S>,
    dest: MixDest,
    frames: &[f32],
    time: Option<TimeValue>,
) -> Result<(), InstrumentError> {
    let at_sample = match time {
        None => None,
        Some(value) => {
            let seconds = ctx.to_seconds(value)?;
            Some((seconds * f64::from(ctx.sample_rate())).round() as usize)
        }
    };
    ctx.sink_mut().mix_on(dest, frames, at_sample)?;
    Ok(())
}

fn envelope(index: usize, n: usize, attack: usize, release: usize) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    if index < attack {
        return if attack == 0 {
            1.0
        } else {
            index as f64 / attack as f64
        };
    }
    let tail = n - 1 - index;
    if tail < release {
        return if release == 0 {
            1.0
        } else {
            tail as f64 / release as f64
        };
    }
    1.0
}

/// Port of drywet-py `render_additive`.
fn render_additive(freq: f64, duration: f64, sample_rate: u32) -> Vec<f32> {
    let sr = f64::from(sample_rate);
    let n = (duration * sr).round().max(1.0) as usize;
    let mut attack = ((ATTACK_SECONDS * sr) as usize).max(1);
    let mut release = ((RELEASE_SECONDS * sr) as usize).max(1);
    if attack + release >= n {
        attack = (n / 5).max(1);
        release = n.saturating_sub(attack + 1).max(1);
    }
    let mut frames = Vec::with_capacity(n);
    for i in 0..n {
        let env = envelope(i, n, attack, release);
        let t = i as f64 / sr;
        let mut sample = 0.0;
        for (k, &amp) in HARMONICS.iter().enumerate() {
            let harmonic = (k + 1) as f64;
            sample += amp * (2.0 * PI * freq * harmonic * t).sin();
        }
        frames.push((sample * env * GAIN) as f32);
    }
    frames
}

fn drum_len(duration: f64, sample_rate: u32) -> usize {
    (duration * f64::from(sample_rate)).round().max(1.0) as usize
}

fn lcg_noise(seed: &mut u32) -> f64 {
    *seed = 1103515245u32.wrapping_mul(*seed).wrapping_add(12345) & 0x7FFF_FFFF;
    (f64::from(*seed) / f64::from(0x7FFF_FFFF)) * 2.0 - 1.0
}

fn render_drum(name: &str, sample_rate: u32) -> Result<Vec<f32>, InstrumentError> {
    match name.to_ascii_lowercase().as_str() {
        "kick" => Ok(render_kick(sample_rate)),
        "snare" => Ok(render_snare(sample_rate)),
        "hat" | "hi-hat" | "hihat" => Ok(render_hat(sample_rate)),
        _ => Err(InstrumentError::UnknownDrum(name.to_string())),
    }
}

/// Port of drywet-py `render_kick`.
fn render_kick(sample_rate: u32) -> Vec<f32> {
    let sr = f64::from(sample_rate);
    let n = drum_len(0.22, sample_rate);
    let mut frames = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / sr;
        let env = 1.0 - (i as f64 / n as f64);
        let freq = 150.0 * (40.0_f64 / 150.0).powf(i as f64 / n as f64);
        frames.push(((2.0 * PI * freq * t).sin() * env * 0.7) as f32);
    }
    frames
}

/// Port of drywet-py `render_snare`.
fn render_snare(sample_rate: u32) -> Vec<f32> {
    let sr = f64::from(sample_rate);
    let n = drum_len(0.16, sample_rate);
    let mut frames = Vec::with_capacity(n);
    let mut seed = 1_234_567u32;
    for i in 0..n {
        let t = i as f64 / sr;
        let env = 1.0 - (i as f64 / n as f64);
        let noise = lcg_noise(&mut seed);
        let tone = (2.0 * PI * 180.0 * t).sin();
        frames.push(((0.65 * noise + 0.35 * tone) * env * 0.45) as f32);
    }
    frames
}

/// Port of drywet-py `render_hat`.
fn render_hat(sample_rate: u32) -> Vec<f32> {
    let n = drum_len(0.05, sample_rate);
    let mut frames = Vec::with_capacity(n);
    let mut seed = 7_654_321u32;
    for i in 0..n {
        let env = 1.0 - (i as f64 / n as f64);
        let noise = lcg_noise(&mut seed);
        frames.push((noise * env * 0.28) as f32);
    }
    frames
}

fn wav_io_error(err: std::io::Error) -> InstrumentError {
    InstrumentError::InvalidWav(err.to_string())
}

fn invalid_wav(msg: impl Into<String>) -> InstrumentError {
    InstrumentError::InvalidWav(msg.into())
}

/// `^([A-Ga-g][#b]?\\d+)\\.wav$` (case-insensitive extension).
fn wav_note_stem(name: &str) -> Option<&str> {
    let bytes = name.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    let ext = &bytes[bytes.len() - 4..];
    if !ext.eq_ignore_ascii_case(b".wav") {
        return None;
    }
    let stem = &name[..name.len() - 4];
    let stem_bytes = stem.as_bytes();
    if stem_bytes.is_empty() || !matches!(stem_bytes[0], b'A'..=b'G' | b'a'..=b'g') {
        return None;
    }
    let mut idx = 1;
    if let Some(&acc) = stem_bytes.get(idx) {
        if acc == b'#' || acc == b'b' {
            idx += 1;
        }
    }
    let digits = &stem_bytes[idx..];
    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(stem)
}

fn load_wav(path: &Path, sample_rate: u32) -> Result<Vec<f32>, InstrumentError> {
    let data = fs::read(path).map_err(wav_io_error)?;
    decode_wav(&data, sample_rate)
}

fn decode_wav(data: &[u8], sample_rate: u32) -> Result<Vec<f32>, InstrumentError> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(invalid_wav("not a RIFF/WAVE file"));
    }

    let mut offset = 12usize;
    let mut fmt = None;
    let mut pcm: Option<&[u8]> = None;
    while offset.saturating_add(8) <= data.len() {
        let id = &data[offset..offset + 4];
        let size = u32::from_le_bytes(
            data[offset + 4..offset + 8]
                .try_into()
                .expect("chunk size is 4 bytes"),
        ) as usize;
        let start = offset + 8;
        let end = start
            .checked_add(size)
            .ok_or_else(|| invalid_wav("WAV chunk overflows"))?;
        if end > data.len() {
            return Err(invalid_wav("WAV chunk truncated"));
        }
        if id == b"fmt " {
            fmt = Some(parse_fmt_chunk(&data[start..end])?);
        } else if id == b"data" {
            pcm = Some(&data[start..end]);
        }
        offset = end;
        if size % 2 == 1 && offset < data.len() {
            offset += 1;
        }
    }

    let (channels, src_rate) = fmt.ok_or_else(|| invalid_wav("WAV missing fmt chunk"))?;
    let raw = pcm.ok_or_else(|| invalid_wav("WAV missing data chunk"))?;
    if channels == 0 {
        return Err(invalid_wav("WAV has no channels"));
    }

    let samples: Vec<i16> = raw
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let ch = usize::from(channels);
    let frames: Vec<f32> = if ch == 1 {
        samples.iter().map(|&s| f32::from(s) / 32767.0).collect()
    } else {
        samples
            .chunks_exact(ch)
            .map(|frame| {
                let sum: f32 = frame.iter().map(|&s| f32::from(s)).sum();
                (sum / ch as f32) / 32767.0
            })
            .collect()
    };
    Ok(resample(&frames, src_rate, sample_rate))
}

fn parse_fmt_chunk(chunk: &[u8]) -> Result<(u16, u32), InstrumentError> {
    if chunk.len() < 16 {
        return Err(invalid_wav("WAV fmt chunk too short"));
    }
    let format = u16::from_le_bytes([chunk[0], chunk[1]]);
    let channels = u16::from_le_bytes([chunk[2], chunk[3]]);
    let rate = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
    let bits = u16::from_le_bytes([chunk[14], chunk[15]]);
    if format != 1 {
        return Err(invalid_wav("WAV must be PCM"));
    }
    if bits != 16 {
        return Err(invalid_wav("WAV must be 16-bit"));
    }
    Ok((channels, rate))
}

fn resample(frames: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate {
        return frames.to_vec();
    }
    if frames.is_empty() {
        return Vec::new();
    }
    let ratio = f64::from(src_rate) / f64::from(dst_rate);
    let n = (frames.len() as f64 / ratio).round() as usize;
    let last = frames.len() - 1;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let pos = i as f64 * ratio;
        let lo = (pos as usize).min(last);
        let hi = (lo + 1).min(last);
        let frac = (pos - lo as f64) as f32;
        out.push(frames[lo] * (1.0 - frac) + frames[hi] * frac);
    }
    out
}

fn pitch_shift(frames: &[f32], semitones: i32) -> Vec<f32> {
    if frames.is_empty() || semitones == 0 {
        return frames.to_vec();
    }
    let ratio = 2.0_f64.powf(f64::from(semitones) / 12.0);
    let n = ((frames.len() as f64 / ratio).round() as usize).max(1);
    let last = frames.len() - 1;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let pos = i as f64 * ratio;
        let lo = (pos as usize).min(last);
        let hi = (lo + 1).min(last);
        let frac = (pos - pos.floor()) as f32;
        out.push(frames[lo] * (1.0 - frac) + frames[hi] * frac);
    }
    out
}
