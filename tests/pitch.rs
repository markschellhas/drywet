use drywet::limits::{
    BPM_MAX, BPM_MIN, DEFAULT_CHANNELS, DEFAULT_MAX_VOICES, DEFAULT_PPQ, DEFAULT_SAMPLE_RATE,
    HZ_MAX, HZ_MIN, MAX_SCHEDULE_SECONDS, MIDI_MAX, MIDI_MIN,
};
use drywet::pitch::{midi_to_hz, note_to_midi};

#[test]
fn pitch_constants() {
    assert_eq!((MIDI_MIN, MIDI_MAX), (0, 127));
    assert_eq!((HZ_MIN, HZ_MAX), (20.0, 20000.0));
    assert_eq!((BPM_MIN, BPM_MAX), (40, 240));
    assert_eq!(DEFAULT_PPQ, 192);
    assert_eq!(DEFAULT_SAMPLE_RATE, 44100);
    assert_eq!(DEFAULT_CHANNELS, 1);
    assert_eq!(DEFAULT_MAX_VOICES, 32);
    assert_eq!(MAX_SCHEDULE_SECONDS, 600.0);
}

#[test]
fn pitch_note_to_midi_scientific_and_int() {
    assert_eq!(note_to_midi("C4").unwrap(), 60);
    assert_eq!(note_to_midi("A4").unwrap(), 69);
    assert_eq!(note_to_midi("C#4").unwrap(), 61);
    assert_eq!(note_to_midi("Db4").unwrap(), 61);
    assert_eq!(note_to_midi(60).unwrap(), 60);
}

#[test]
fn pitch_note_to_midi_rejects_out_of_range() {
    assert!(note_to_midi(-1).is_err());
    assert!(note_to_midi(128).is_err());
    assert!(note_to_midi("H4").is_err());
}

#[test]
fn pitch_midi_to_hz_a4() {
    let hz = midi_to_hz(69).unwrap();
    assert!((hz - 440.0).abs() < 1e-9);
}
