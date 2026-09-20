use std::error::Error;
use std::f64::consts::PI;
use std::fmt;

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
    /// [`Synth::trigger_attack`] would exceed [`Synth::with_max_voices`].
    VoiceLimitExceeded,
    /// [`Drum::trigger`] name was not kick, snare, hat, or a hat alias.
    UnknownDrum(String),
    /// Note-name or MIDI conversion failed.
    Pitch(PitchError),
    /// Duration or start-time conversion failed.
    Time(TimeError),
}

impl fmt::Display for InstrumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstrumentError::VoiceLimitExceeded => write!(f, "voice limit exceeded"),
            InstrumentError::UnknownDrum(name) => write!(f, "unknown drum: {name:?}"),
            InstrumentError::Pitch(err) => write!(f, "{err}"),
            InstrumentError::Time(err) => write!(f, "{err}"),
        }
    }
}

impl Error for InstrumentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            InstrumentError::Pitch(err) => Some(err),
            InstrumentError::Time(err) => Some(err),
            InstrumentError::VoiceLimitExceeded | InstrumentError::UnknownDrum(_) => None,
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
    /// Duration and time convert via [`crate::transport::TransportRef::to_seconds`].
    /// `time = None` mixes at the write cursor.
    pub fn trigger_attack_release<S, N, D>(
        &mut self,
        ctx: &mut Context<S>,
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
        let result = self.mix_note(ctx, note, Some(duration), time);
        self.release_voice();
        result.map(|()| self)
    }

    /// Acquire a voice and mix a 1.0s additive note (heritage one-shot).
    pub fn trigger_attack<S, N>(
        &mut self,
        ctx: &mut Context<S>,
        note: N,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError>
    where
        S: Sink,
        N: IntoNote,
    {
        self.acquire()?;
        match self.mix_note(ctx, note, None::<f64>, time) {
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
        ctx: &mut Context<S>,
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
            Some(value) => ctx.transport().to_seconds(value)?,
            None => TRIGGER_ATTACK_SECONDS,
        };
        let frames = render_additive(freq, duration_s, self.sample_rate);
        mix_at_time(ctx, &frames, time)
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
        ctx: &mut Context<S>,
        name: &str,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        let frames = render_drum(name, self.sample_rate)?;
        mix_at_time(ctx, &frames, time)?;
        Ok(self)
    }

    /// Alias of [`Drum::trigger`].
    pub fn trigger_attack<S: Sink>(
        &mut self,
        ctx: &mut Context<S>,
        name: &str,
        time: Option<TimeValue>,
    ) -> Result<&mut Self, InstrumentError> {
        self.trigger(ctx, name, time)
    }

    /// No-op. Drum hits are one-shot transients.
    pub fn trigger_release(&mut self, _name: &str, _time: Option<TimeValue>) -> &mut Self {
        self
    }

    /// Alias of [`Drum::trigger`]. `duration` is ignored.
    pub fn trigger_attack_release<S, D>(
        &mut self,
        ctx: &mut Context<S>,
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

    /// No-op. Drum hits are one-shot transients.
    pub fn release_all(&mut self, _time: Option<TimeValue>) -> &mut Self {
        self
    }
}

/// Mix `frames` at `time`, or at the write cursor when `time` is `None`.
fn mix_at_time<S: Sink>(
    ctx: &mut Context<S>,
    frames: &[f32],
    time: Option<TimeValue>,
) -> Result<(), InstrumentError> {
    let at_sample = match time {
        None => None,
        Some(value) => {
            let seconds = ctx.transport().to_seconds(value)?;
            Some((seconds * f64::from(ctx.sample_rate())).round() as usize)
        }
    };
    ctx.sink_mut().mix(frames, at_sample);
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
