//! Native default-device output through CPAL.
//!
//! [`DeviceSink`] is the sink for runnable examples and desktop apps. It uses
//! CoreAudio on macOS and CPAL's native host on other supported platforms.
//! Tests continue to use [`crate::BufferSink`] or an explicitly mocked
//! [`crate::PipeWireSink`].

use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, ErrorKind, FromSample, OutputCallbackInfo, SampleFormat, SizedSample, Stream,
    StreamConfig, I24,
};

use crate::sink::Sink;

/// Failure opening, starting, or draining the system's default audio device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSinkError(String);

impl DeviceSinkError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for DeviceSinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for DeviceSinkError {}

#[derive(Debug, Default)]
struct Playback {
    mono: Vec<f32>,
    base_cursor: usize,
    playback_cursor: usize,
    staging_cursor: usize,
    playing: bool,
    accepted: bool,
    stream_error: Option<String>,
}

impl Playback {
    fn mix(&mut self, frames: &[f32], at_sample: Option<usize>) {
        let mut at = at_sample.unwrap_or_else(|| self.write_cursor());
        let mut frames = frames;
        if at < self.base_cursor {
            let skip = (self.base_cursor - at).min(frames.len());
            frames = &frames[skip..];
            at = self.base_cursor;
        }
        if frames.is_empty() {
            return;
        }
        if self.mono.is_empty() {
            self.base_cursor = self.playback_cursor.min(at);
        }
        let start = at.saturating_sub(self.base_cursor);
        let needed = start.saturating_add(frames.len());
        if self.mono.len() < needed {
            self.mono.resize(needed, 0.0);
        }
        for (destination, source) in self.mono[start..needed].iter_mut().zip(frames) {
            *destination += *source;
        }
        self.accepted = true;
    }

    fn write(&mut self, frames: &[f32]) {
        let at = self.write_cursor();
        self.mix(frames, Some(at));
        if !self.playing {
            self.staging_cursor = at.saturating_add(frames.len());
        }
    }

    fn write_cursor(&self) -> usize {
        if self.playing {
            self.playback_cursor
        } else {
            self.staging_cursor
        }
    }

    fn queued_end(&self) -> usize {
        self.base_cursor
            .saturating_add(self.mono.len())
            .max(self.staging_cursor)
    }

    fn sample_at(&self, cursor: usize) -> f32 {
        cursor
            .checked_sub(self.base_cursor)
            .and_then(|index| self.mono.get(index))
            .copied()
            .unwrap_or(0.0)
    }

    fn reclaim_consumed(&mut self) {
        if self.playback_cursor >= self.base_cursor.saturating_add(self.mono.len()) {
            self.mono.clear();
            self.base_cursor = self.playback_cursor;
        }
    }
}

/// Real output sink connected to the system's default audio device.
///
/// Construct this before the [`crate::Context`] because the context must use
/// the device's negotiated sample rate and channel count:
///
/// ```no_run
/// use drywet::{Context, DeviceSink};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let sink = DeviceSink::new()?;
/// let ctx = Context::with(sink.sample_rate(), sink.channels(), sink);
/// # Ok(())
/// # }
/// ```
pub struct DeviceSink {
    sample_rate: u32,
    channels: u16,
    state: Arc<Mutex<Playback>>,
    stream: Option<Stream>,
}

impl fmt::Debug for DeviceSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceSink")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("state", &self.state)
            .field("stream_open", &self.stream.is_some())
            .finish()
    }
}

impl DeviceSink {
    /// Open the system's default output device in a paused state.
    pub fn new() -> Result<Self, DeviceSinkError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| DeviceSinkError::new("no default audio output device"))?;
        let supported = device
            .default_output_config()
            .map_err(|err| DeviceSinkError::new(format!("default output config: {err}")))?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels();
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let state = Arc::new(Mutex::new(Playback::default()));
        let stream = build_stream(&device, &config, sample_format, Arc::clone(&state))?;

        Ok(Self {
            sample_rate,
            channels,
            state,
            stream: Some(stream),
        })
    }

    /// Negotiated output sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Negotiated output channel count.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Start or resume the native output stream.
    pub fn play(&mut self) -> Result<&mut Self, DeviceSinkError> {
        let stream = self
            .stream
            .as_ref()
            .ok_or_else(|| DeviceSinkError::new("audio output is closed"))?;
        {
            let mut state = lock(&self.state);
            state.stream_error = None;
            state.playing = true;
            state.accepted = true;
        }
        if let Err(err) = stream.play() {
            lock(&self.state).playing = false;
            return Err(DeviceSinkError::new(format!(
                "start default audio output: {err}"
            )));
        }
        Ok(self)
    }

    /// Block until every currently queued frame has reached the device callback.
    ///
    /// This wait keeps a CLI example alive; it does not drive musical timing.
    /// Sequence timing has already been resolved to sample offsets by
    /// [`crate::Context::render`].
    pub fn wait_until_end(&self) -> Result<(), DeviceSinkError> {
        let (target, cursor) = {
            let state = lock(&self.state);
            (state.queued_end(), state.playback_cursor)
        };
        let remaining = target.saturating_sub(cursor);
        let expected = Duration::from_secs_f64(remaining as f64 / f64::from(self.sample_rate));
        let deadline = Instant::now() + expected + Duration::from_secs(5);

        loop {
            {
                let state = lock(&self.state);
                if let Some(err) = &state.stream_error {
                    return Err(DeviceSinkError::new(format!(
                        "audio output stream failed: {err}"
                    )));
                }
                if state.playback_cursor >= target {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(DeviceSinkError::new("timed out waiting for audio output"));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn pause(&mut self) {
        if let Some(stream) = &self.stream {
            let _ = stream.pause();
        }
        lock(&self.state).playing = false;
    }
}

impl Sink for DeviceSink {
    fn mix(&mut self, frames: &[f32], at_sample: Option<usize>) {
        lock(&self.state).mix(frames, at_sample);
    }

    fn write(&mut self, frames: &[f32]) {
        lock(&self.state).write(frames);
    }

    fn stop(&mut self) {
        self.pause();
    }

    fn close(&mut self) {
        self.pause();
        self.stream = None;
    }

    fn latency_ms(&self) -> u32 {
        0
    }

    fn write_cursor(&self) -> usize {
        lock(&self.state).write_cursor()
    }

    fn accepted(&self) -> bool {
        lock(&self.state).accepted
    }

    fn mark_accepted(&mut self) {
        lock(&self.state).accepted = true;
    }

    fn start_clock(&mut self) {
        if let Err(err) = self.play() {
            lock(&self.state).stream_error = Some(err.to_string());
        }
    }
}

fn lock(state: &Arc<Mutex<Playback>>) -> std::sync::MutexGuard<'_, Playback> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn build_stream(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    state: Arc<Mutex<Playback>>,
) -> Result<Stream, DeviceSinkError> {
    match sample_format {
        SampleFormat::I8 => build_typed_stream::<i8>(device, config, state),
        SampleFormat::I16 => build_typed_stream::<i16>(device, config, state),
        SampleFormat::I24 => build_typed_stream::<I24>(device, config, state),
        SampleFormat::I32 => build_typed_stream::<i32>(device, config, state),
        SampleFormat::I64 => build_typed_stream::<i64>(device, config, state),
        SampleFormat::U8 => build_typed_stream::<u8>(device, config, state),
        SampleFormat::U16 => build_typed_stream::<u16>(device, config, state),
        SampleFormat::U32 => build_typed_stream::<u32>(device, config, state),
        SampleFormat::U64 => build_typed_stream::<u64>(device, config, state),
        SampleFormat::F32 => build_typed_stream::<f32>(device, config, state),
        SampleFormat::F64 => build_typed_stream::<f64>(device, config, state),
        other => Err(DeviceSinkError::new(format!(
            "unsupported output sample format: {other}"
        ))),
    }
}

fn build_typed_stream<T>(
    device: &Device,
    config: &StreamConfig,
    state: Arc<Mutex<Playback>>,
) -> Result<Stream, DeviceSinkError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let error_state = Arc::clone(&state);
    device
        .build_output_stream(
            *config,
            move |output: &mut [T], _: &OutputCallbackInfo| {
                let mut state = lock(&state);
                for frame in output.chunks_mut(channels) {
                    let sample = state.sample_at(state.playback_cursor);
                    state.playback_cursor = state.playback_cursor.saturating_add(1);
                    let sample = T::from_sample(sample.clamp(-1.0, 1.0));
                    for output_sample in frame {
                        *output_sample = sample;
                    }
                }
                state.reclaim_consumed();
            },
            move |err| {
                if !matches!(err.kind(), ErrorKind::Xrun | ErrorKind::RealtimeDenied) {
                    lock(&error_state).stream_error = Some(err.to_string());
                }
            },
            None,
        )
        .map_err(|err| DeviceSinkError::new(format!("build default audio stream: {err}")))
}

#[cfg(test)]
mod tests {
    use super::Playback;

    #[test]
    fn playback_stages_and_mixes_without_a_device() {
        let mut playback = Playback::default();
        playback.mix(&[0.5, 0.25], Some(1));
        playback.write(&[0.0; 4]);
        assert_eq!(playback.mono, [0.0, 0.5, 0.25, 0.0]);
        assert_eq!(playback.write_cursor(), 4);
        assert_eq!(playback.queued_end(), 4);
    }

    #[test]
    fn live_cursor_follows_device_playhead() {
        let mut playback = Playback {
            staging_cursor: 100,
            playback_cursor: 25,
            playing: true,
            ..Playback::default()
        };
        assert_eq!(playback.write_cursor(), 25);
        playback.mix(&[0.75], None);
        assert_eq!(playback.base_cursor, 25);
        assert_eq!(playback.mono, [0.75]);
    }

    #[test]
    fn consumed_audio_reuses_the_buffer_at_the_current_cursor() {
        let mut playback = Playback::default();
        playback.mix(&[0.25; 128], Some(0));
        playback.playback_cursor = 128;
        playback.reclaim_consumed();
        assert!(playback.mono.is_empty());
        assert_eq!(playback.base_cursor, 128);

        playback.playing = true;
        playback.mix(&[0.5], None);
        assert_eq!(playback.mono, [0.5]);
    }
}
