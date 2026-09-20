use drywet::sink::{
    latency_ms_from_quantum, MockStream, PipeWireSink, StreamBackend, DEFAULT_QUANTUM_FRAMES,
};
use drywet::{Context, PipeWireSink as CratePipeWireSink};

#[test]
fn pipewire_sink_mixes_and_write_cursor() {
    let mut sink = PipeWireSink::new(44100, 1);
    sink.mix(&[1.0, 0.5], Some(0));
    assert_eq!(sink.write_cursor(), 0);
    sink.write(&[0.25]);
    assert_eq!(sink.write_cursor(), 1);
    assert!(sink.accepted());

    let mut out = [0.0f32; 2];
    sink.process(&mut out);
    assert_eq!(out, [1.25, 0.5]);
    assert_eq!(sink.backend().process_calls(), 1);
}

#[test]
fn pipewire_sink_latency_is_negotiated_not_hardcoded_80() {
    let backend = MockStream::new(44100, 256);
    let negotiated = latency_ms_from_quantum(44100, 256);
    assert_eq!(negotiated, 256 * 1000 / 44100);
    assert_eq!(negotiated, 5);
    assert_ne!(negotiated, 80);

    let sink = PipeWireSink::with_backend(44100, 1, backend);
    assert_eq!(sink.latency_ms(), negotiated);
    assert_eq!(sink.backend().quantum_frames(), 256);
    assert_ne!(sink.latency_ms(), 80);

    let default_sink = PipeWireSink::new(44100, 1);
    let default_latency = latency_ms_from_quantum(44100, DEFAULT_QUANTUM_FRAMES);
    assert_eq!(default_sink.latency_ms(), default_latency);
    assert_ne!(default_sink.latency_ms(), 80);
}

#[test]
fn pipewire_sink_does_not_spawn_pw_cat() {
    let mut sink = PipeWireSink::new(44100, 1);
    sink.start_clock();
    sink.write(&[0.1, 0.2, 0.3]);
    sink.mix(&[0.05], Some(0));
    let mut out = [0.0f32; 3];
    sink.process(&mut out);
    sink.stop();
    sink.close();

    assert!(sink.backend().spawn_attempts().is_empty());
    assert_eq!(sink.backend().open_count(), 1);
}

#[test]
fn pipewire_sink_stop_keeps_stream_close_tears_down() {
    let mut sink = PipeWireSink::with_backend(44100, 1, MockStream::new(44100, 256));
    assert!(!sink.backend().is_open());

    sink.start_clock();
    assert!(sink.backend().is_open());
    assert_eq!(sink.backend().open_count(), 1);

    sink.stop();
    assert!(sink.backend().is_open(), "stop must keep the stream open");

    sink.mix(&[0.75], Some(0));
    assert!(sink.accepted());
    let mut out = [0.0f32; 1];
    sink.process(&mut out);
    assert_eq!(out, [0.75]);

    sink.close();
    assert!(!sink.backend().is_open());
}

#[test]
fn pipewire_sink_start_clock_is_idempotent() {
    let mut sink = PipeWireSink::new(22050, 1);
    sink.start_clock();
    sink.start_clock();
    assert!(sink.backend().is_open());
    assert_eq!(sink.backend().open_count(), 1);
    assert!(sink.accepted());
}

#[test]
fn pipewire_sink_callback_uses_preallocated_queue() {
    // process writes into a caller-provided buffer. The callback copies from
    // preallocated slots and never allocates a Vec for the period itself.
    let mut sink = PipeWireSink::new(44100, 2);
    sink.mix(&[1.0, -0.5], Some(0));

    let mut out = [99.0f32; 4];
    sink.process(&mut out);
    assert_eq!(out, [1.0, 1.0, -0.5, -0.5]);
    assert_eq!(sink.backend().process_calls(), 1);
    assert_eq!(sink.frames(), &[] as &[f32]);
}

#[test]
fn pipewire_sink_context_with_owns_in_process_sink() {
    let ctx = Context::with(44100, 1, CratePipeWireSink::new(44100, 1));
    assert_eq!(ctx.sample_rate(), 44100);
    assert_eq!(ctx.channels(), 1);
    assert_eq!(ctx.sink().sample_rate(), 44100);
    assert_eq!(
        ctx.sink().latency_ms(),
        latency_ms_from_quantum(44100, DEFAULT_QUANTUM_FRAMES)
    );
    ctx.sink_mut().write(&[0.5, 0.25]);
    assert_eq!(ctx.sink().write_cursor(), 2);
    assert!(ctx.sink().accepted());
}

/// Live daemon probe. Ignored in default `cargo test`.
///
/// Run with `DRYWET_LIVE_AUDIO=1 cargo test -- --ignored pipewire_sink_live`
/// when a real PipeWire server is present. This crate's default backend is
/// in-process (no libpipewire link), so the body returns unless the env is set.
#[test]
#[ignore]
fn pipewire_sink_live_opens_when_env_set() {
    if std::env::var("DRYWET_LIVE_AUDIO").ok().as_deref() != Some("1") {
        return;
    }
    let mut sink = PipeWireSink::new(44100, 1);
    sink.start_clock();
    assert!(sink.backend().is_open());
    sink.close();
}

#[test]
fn pipewire_sink_write_without_clock_stays_queued_for_process() {
    let mut sink = PipeWireSink::new(44100, 1);
    assert!(!sink.backend().is_open());
    sink.write(&[0.2, 0.4]);
    assert_eq!(sink.write_cursor(), 2);
    let mut out = [0.0f32; 2];
    sink.process(&mut out);
    assert_eq!(out, [0.2, 0.4]);
}
