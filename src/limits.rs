/// Inclusive MIDI note-number bounds.
pub const MIDI_MIN: u8 = 0;
pub const MIDI_MAX: u8 = 127;

/// Inclusive audible frequency bounds in Hz.
pub const HZ_MIN: f64 = 20.0;
pub const HZ_MAX: f64 = 20000.0;

/// Inclusive tempo bounds in beats per minute.
pub const BPM_MIN: u32 = 40;
pub const BPM_MAX: u32 = 240;

/// Pulses per quarter note for the arrangement clock.
pub const DEFAULT_PPQ: u32 = 192;

/// Default PCM sample rate in Hz.
pub const DEFAULT_SAMPLE_RATE: u32 = 44100;

/// Default sink channel count (mono).
pub const DEFAULT_CHANNELS: u16 = 1;

/// Default polyphonic voice cap.
pub const DEFAULT_MAX_VOICES: u32 = 32;

/// Longest schedule interval accepted by Transport, in seconds.
pub const MAX_SCHEDULE_SECONDS: f64 = 600.0;

/// Max playback inserts on one sink. Replacing the chain never grows in the callback.
pub const MAX_INSERTS: usize = 8;

/// Max extra named output buses on one sink (not counting implicit master).
pub const MAX_BUSES: usize = 8;
