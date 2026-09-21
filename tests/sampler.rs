use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use drywet::instrument::InstrumentError;
use drywet::{Context, Sampler};

fn unique_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("drywet-sampler-{}-{}", std::process::id(), nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_pcm16_wav(path: &Path, frames: &[f32], rate: u32, channels: u16) {
    let mut pcm = Vec::with_capacity(frames.len() * usize::from(channels) * 2);
    for &sample in frames {
        let encoded = (sample * 32767.0) as i16;
        for _ in 0..channels {
            pcm.extend_from_slice(&encoded.to_le_bytes());
        }
    }
    write_wav_bytes(path, &pcm, rate, channels, 16, 1);
}

fn write_pcm8_wav(path: &Path, frames: &[f32], rate: u32) {
    let pcm: Vec<u8> = frames
        .iter()
        .map(|&sample| ((sample * 0.5 + 0.5) * 255.0) as u8)
        .collect();
    write_wav_bytes(path, &pcm, rate, 1, 8, 1);
}

fn write_wav_bytes(path: &Path, pcm: &[u8], rate: u32, channels: u16, bits: u16, format: u16) {
    let bytes_per_sample = u32::from(bits / 8);
    let data_size = pcm.len() as u32;
    let byte_rate = rate * u32::from(channels) * bytes_per_sample;
    let block_align = channels * (bits / 8);
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&format.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(pcm);
    fs::write(path, out).unwrap();
}

fn peak(frames: &[f32]) -> f32 {
    frames.iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()))
}

#[test]
fn sampler_map_pitch_shift_directory_and_polyphony() {
    let dir = unique_dir();
    let c4 = dir.join("C4.wav");
    write_pcm16_wav(&c4, &[0.4; 64], 44100, 1);

    let ctx = Context::new();
    let mut sampler = Sampler::with_map(&ctx, [("C4", c4.as_path())], 1).unwrap();
    sampler
        .trigger_attack_release(&ctx, "C4", 0.01, Some(0.0.into()))
        .unwrap();
    assert!(peak(ctx.sink().frames()) > 0.01);

    sampler
        .trigger_attack_release(&ctx, "C5", 0.01, Some(0.0.into()))
        .unwrap();
    sampler.add("D4", &c4).unwrap();

    let bank = dir.join("bank");
    fs::create_dir_all(&bank).unwrap();
    write_pcm16_wav(&bank.join("A4.wav"), &[0.3; 32], 44100, 1);
    fs::write(bank.join("readme.txt"), b"not a sample").unwrap();
    let loaded = Sampler::from_directory(&ctx, &bank).unwrap();
    assert!(loaded.samples().contains_key(&69));
    assert_eq!(loaded.samples().len(), 1);

    sampler
        .trigger_attack(&ctx, "C4", Some(0.0.into()))
        .unwrap();
    assert!(matches!(
        sampler.trigger_attack(&ctx, "E4", Some(0.0.into())),
        Err(InstrumentError::VoiceLimitExceeded)
    ));
}

#[test]
fn sampler_rejects_non_16bit_wav() {
    let dir = unique_dir();
    let path = dir.join("C4.wav");
    write_pcm8_wav(&path, &[0.4; 16], 44100);
    let ctx = Context::new();
    let err = Sampler::with_map(&ctx, [("C4", path.as_path())], 1).unwrap_err();
    assert!(matches!(
        err,
        InstrumentError::InvalidWav(msg) if msg == "WAV must be 16-bit"
    ));
}

#[test]
fn sampler_stereo_averages_to_mono() {
    let dir = unique_dir();
    let path = dir.join("C4.wav");
    let mut pcm = Vec::new();
    for _ in 0..16 {
        pcm.extend_from_slice(&((0.8 * 32767.0) as i16).to_le_bytes());
        pcm.extend_from_slice(&0i16.to_le_bytes());
    }
    write_wav_bytes(&path, &pcm, 44100, 2, 16, 1);
    let ctx = Context::new();
    let sampler = Sampler::with_map(&ctx, [("C4", path.as_path())], 1).unwrap();
    let frames = &sampler.samples()[&60];
    assert_eq!(frames.len(), 16);
    assert!((frames[0] - 0.4).abs() < 0.01);
}

#[test]
fn sampler_resamples_on_load_when_rate_differs() {
    let dir = unique_dir();
    let path = dir.join("C4.wav");
    write_pcm16_wav(&path, &[0.4; 32], 22050, 1);
    let ctx = Context::new();
    let mut sampler = Sampler::with_map(&ctx, [("C4", path.as_path())], 1).unwrap();
    assert_eq!(sampler.samples()[&60].len(), 64);
    sampler
        .trigger_attack_release(&ctx, "C4", 0.01, Some(0.0.into()))
        .unwrap();
    assert!(peak(ctx.sink().frames()) > 0.01);
}

#[test]
fn sampler_release_all_and_empty_map() {
    let dir = unique_dir();
    let path = dir.join("C4.wav");
    write_pcm16_wav(&path, &[0.4; 16], 44100, 1);
    let ctx = Context::new();
    let mut sampler = Sampler::with_map(&ctx, [("C4", path.as_path())], 1).unwrap();
    sampler
        .trigger_attack(&ctx, "C4", Some(0.0.into()))
        .unwrap();
    assert_eq!(sampler.active_voices(), 1);
    sampler.trigger_release("C4", None).unwrap();
    assert_eq!(sampler.active_voices(), 0);
    sampler
        .trigger_attack(&ctx, "C4", Some(0.0.into()))
        .unwrap();
    sampler.release_all(None);
    assert_eq!(sampler.active_voices(), 0);

    let mut empty = Sampler::new(&ctx);
    assert!(matches!(
        empty.trigger_attack(&ctx, "C4", Some(0.0.into())),
        Err(InstrumentError::EmptySampler)
    ));
    assert!(!empty.loop_flag());
}
