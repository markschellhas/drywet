use drywet::sink::Sink;
use drywet::transport::{TransportError, TransportState};
use drywet::Context;

#[derive(Default)]
struct ProbeSink {
    accepted: bool,
    stop_calls: usize,
    close_calls: usize,
    latency_ms: u32,
}

impl Sink for ProbeSink {
    fn mix(&mut self, _frames: &[f32], _at_sample: Option<usize>) {}

    fn write(&mut self, _frames: &[f32]) {}

    fn stop(&mut self) {
        self.stop_calls += 1;
    }

    fn close(&mut self) {
        self.close_calls += 1;
    }

    fn latency_ms(&self) -> u32 {
        self.latency_ms
    }

    fn write_cursor(&self) -> usize {
        0
    }

    fn accepted(&self) -> bool {
        self.accepted
    }

    fn mark_accepted(&mut self) {
        self.accepted = true;
    }
}

#[test]
fn transport_state_start_stop_pause_toggle_and_playhead() {
    let mut ctx = Context::new();
    {
        let mut t = ctx.transport();
        assert_eq!(t.state(), TransportState::Stopped);
        assert_eq!(t.seconds(), 0.0);
        assert_eq!(t.position(), "0:0:0");
        t.start();
        assert_eq!(t.state(), TransportState::Started);
        assert_eq!(t.latency_ms(), 0);
        t.pause();
        assert_eq!(t.state(), TransportState::Paused);
        t.toggle();
        assert_eq!(t.state(), TransportState::Started);
        t.stop();
        assert_eq!(t.state(), TransportState::Stopped);
        assert_eq!(t.seconds(), 0.0);
        assert_eq!(t.position(), "0:0:0");
    }
    assert!(ctx.sink().accepted());
}

#[test]
fn transport_state_start_is_idempotent_and_stop_keeps_sink_open() {
    let mut ctx = Context::new();
    ctx.transport().start();
    ctx.transport().start();
    assert_eq!(ctx.transport().state(), TransportState::Started);
    ctx.transport().stop();
    assert_eq!(ctx.transport().state(), TransportState::Stopped);
    assert!(ctx.sink().accepted());
    let _ = ctx.sink().frames();
    let _ = ctx.sink_mut().to_pcm_s16le();
}

#[test]
fn transport_state_start_marks_sink_accepted_and_reads_latency() {
    let sink = ProbeSink {
        latency_ms: 12,
        ..ProbeSink::default()
    };
    let mut ctx = Context::with(44100, 1, sink);
    assert!(!ctx.sink().accepted());
    assert_eq!(ctx.transport().latency_ms(), 12);
    ctx.transport().start();
    assert!(ctx.sink().accepted());
    assert_eq!(ctx.transport().latency_ms(), 12);
    ctx.transport().stop();
    assert_eq!(ctx.sink().stop_calls, 1);
    assert_eq!(ctx.sink().close_calls, 0);
    assert!(ctx.sink().accepted());
}

#[test]
fn transport_state_defaults_bpm_and_signature() {
    let mut ctx = Context::new();
    let t = ctx.transport();
    assert_eq!(t.bpm(), 120.0);
    assert_eq!(t.clock_bpm(), 120.0);
    assert_eq!(t.time_signature(), (4, 4));
    assert_eq!(t.seconds(), 0.0);
    assert_eq!(t.position(), "0:0:0");
}

#[test]
fn transport_state_bpm_bounds() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.set_bpm(100.0).unwrap();
    assert_eq!(t.set_bpm(39.0), Err(TransportError::InvalidBpm(39.0)));
    assert_eq!(t.set_bpm(241.0), Err(TransportError::InvalidBpm(241.0)));
    assert_eq!(t.bpm(), 100.0);
    t.set_bpm(40.0).unwrap();
    t.set_bpm(240.0).unwrap();
    assert_eq!(t.bpm(), 240.0);
}

#[test]
fn transport_state_bpm_written_while_started_applies_on_next_start() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.set_bpm(100.0).unwrap();
    t.start();
    t.set_bpm(140.0).unwrap();
    assert_eq!(t.bpm(), 140.0);
    assert_eq!(t.clock_bpm(), 100.0);
    t.stop();
    t.start();
    assert_eq!(t.bpm(), 140.0);
    assert_eq!(t.clock_bpm(), 140.0);
}

#[test]
fn transport_state_time_signature_int_and_pair() {
    let mut ctx = Context::new();
    let mut t = ctx.transport();
    t.set_time_signature(4).unwrap();
    assert_eq!(t.time_signature(), (4, 4));
    t.set_time_signature((6, 8)).unwrap();
    assert_eq!(t.time_signature(), (6, 8));
    t.set_time_signature((4, 4)).unwrap();
    assert_eq!(t.time_signature(), (4, 4));
    assert_eq!(
        t.set_time_signature(0),
        Err(TransportError::InvalidTimeSignature)
    );
    assert_eq!(
        t.set_time_signature((0, 4)),
        Err(TransportError::InvalidTimeSignature)
    );
    assert_eq!(
        t.set_time_signature((4, 0)),
        Err(TransportError::InvalidTimeSignature)
    );
}

#[test]
fn transport_state_pause_is_noop_when_not_started() {
    let mut ctx = Context::new();
    ctx.transport().pause();
    assert_eq!(ctx.transport().state(), TransportState::Stopped);
    ctx.transport().start().pause();
    assert_eq!(ctx.transport().state(), TransportState::Paused);
    ctx.transport().pause();
    assert_eq!(ctx.transport().state(), TransportState::Paused);
}

#[test]
fn transport_state_toggle_from_stopped_starts() {
    let mut ctx = Context::new();
    ctx.transport().toggle();
    assert_eq!(ctx.transport().state(), TransportState::Started);
    ctx.transport().toggle();
    assert_eq!(ctx.transport().state(), TransportState::Paused);
}

#[test]
fn transport_state_stop_resets_playhead_from_paused() {
    let mut ctx = Context::new();
    ctx.transport().start().pause();
    ctx.transport().stop();
    let t = ctx.transport();
    assert_eq!(t.state(), TransportState::Stopped);
    assert_eq!(t.seconds(), 0.0);
    assert_eq!(t.position(), "0:0:0");
}

#[test]
fn transport_state_buffer_sink_stays_readable_after_stop() {
    let mut ctx = Context::new();
    ctx.sink_mut().write(&[0.25, -0.25]);
    ctx.transport().start();
    ctx.transport().stop();
    assert_eq!(ctx.sink().frames(), &[0.25, -0.25]);
    assert!(ctx.sink().accepted());
}
