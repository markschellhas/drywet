use drywet::sink::Sink;
use drywet::transport::TransportState;
use drywet::{BufferSink, Context};

#[test]
fn context_owns_sink_and_defaults() {
    let sink = BufferSink::new(22050, 1);
    let rate = sink.sample_rate();
    let channels = sink.channels();
    let ctx = Context::with(rate, channels, sink);

    assert_eq!(ctx.sample_rate(), 22050);
    assert_eq!(ctx.channels(), 1);
    assert_eq!(ctx.sink().sample_rate(), rate);
    assert_eq!(ctx.sink().channels(), channels);
    assert_eq!(ctx.transport().state(), TransportState::Stopped);
}

#[test]
fn context_default_is_buffer_sink() {
    let ctx = Context::new();
    assert_eq!(ctx.sample_rate(), 44100);
    assert_eq!(ctx.channels(), 1);
    assert_eq!(ctx.sink().sample_rate(), 44100);
    assert_eq!(ctx.sink().channels(), 1);
    assert_eq!(ctx.transport().state(), TransportState::Stopped);
}

#[test]
fn live_mix_time_behind_playhead_is_now() {
    let ctx = Context::new();
    ctx.sink_mut().write(&vec![0.0; ctx.sample_rate() as usize]);
    assert!(ctx.live_mix_time(0.0, 0.0).is_none());
    assert!(ctx.live_mix_time(1.0, 0.0).is_some());
    assert_eq!(ctx.live_origin(), 1.0);
}
