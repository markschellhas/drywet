use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use drywet::insert::Insert;
use drywet::instrument::InstrumentError;
use drywet::limits::MAX_BUSES;
use drywet::{BusError, Context, MixDest, PipeWireSink, Synth};

struct Gain {
    gain: f32,
}

impl Insert for Gain {
    fn process(&mut self, frames: &mut [f32]) {
        for sample in frames.iter_mut() {
            *sample *= self.gain;
        }
    }
}

struct Integrator {
    acc: f32,
}

impl Insert for Integrator {
    fn process(&mut self, frames: &mut [f32]) {
        for sample in frames.iter_mut() {
            self.acc += *sample;
            *sample = self.acc;
        }
    }
}

fn boxed<I: Insert + 'static>(insert: I) -> Box<dyn Insert> {
    Box::new(insert)
}

fn one_sample(ctx: &Context) -> f64 {
    1.0 / f64::from(ctx.sample_rate())
}

#[test]
fn bus_voice_wets_render_master_frames_stay_dry() {
    let ctx = Context::new();
    let drums = ctx.bus("drums").unwrap();
    drums.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    ctx.sink_mut()
        .mix_on(MixDest::Bus(drums.id()), &[0.5], Some(0))
        .unwrap();
    ctx.sink_mut().mix(&[0.25], Some(0));

    let wet = ctx.render(one_sample(&ctx)).unwrap();
    assert_eq!(&wet[..1], &[1.25]);
    assert_eq!(&ctx.sink().frames()[..1], &[0.25]);
}

#[test]
fn bus_param_after_mix_changes_next_render() {
    struct LiveGain {
        bits: Arc<AtomicU32>,
    }
    impl Insert for LiveGain {
        fn process(&mut self, frames: &mut [f32]) {
            let gain = f32::from_bits(self.bits.load(Ordering::Relaxed));
            for sample in frames {
                *sample *= gain;
            }
        }
    }

    let ctx = Context::new();
    let bits = Arc::new(AtomicU32::new(1.0f32.to_bits()));
    let drums = ctx.bus("drums").unwrap();
    drums
        .set_inserts(vec![boxed(LiveGain {
            bits: Arc::clone(&bits),
        })])
        .unwrap();
    ctx.sink_mut()
        .mix_on(MixDest::Bus(drums.id()), &[1.0], Some(0))
        .unwrap();
    bits.store(0.25f32.to_bits(), Ordering::Relaxed);
    let wet = ctx.render(one_sample(&ctx)).unwrap();
    assert_eq!(&wet[..1], &[0.25]);
    assert!(ctx.sink().frames().is_empty() || ctx.sink().frames()[0] == 0.0);
}

#[test]
fn bus_integrator_state_is_per_bus() {
    let shared = Context::new();
    let drums = shared.bus("drums").unwrap();
    drums
        .set_inserts(vec![boxed(Integrator { acc: 0.0 })])
        .unwrap();
    shared
        .sink_mut()
        .mix_on(MixDest::Bus(drums.id()), &[1.0], Some(0))
        .unwrap();
    shared
        .sink_mut()
        .mix_on(MixDest::Bus(drums.id()), &[1.0], Some(1))
        .unwrap();
    let shared_wet = shared
        .render(2.0 / f64::from(shared.sample_rate()))
        .unwrap();
    assert_eq!(&shared_wet[..2], &[1.0, 2.0]);

    let split = Context::new();
    let drums = split.bus("drums").unwrap();
    drums
        .set_inserts(vec![boxed(Integrator { acc: 0.0 })])
        .unwrap();
    split.sink_mut().mix(&[1.0], Some(0));
    split
        .sink_mut()
        .mix_on(MixDest::Bus(drums.id()), &[1.0], Some(1))
        .unwrap();
    let split_wet = split.render(2.0 / f64::from(split.sample_rate())).unwrap();
    // Master hit is not fed into the bus integrator (that would be [1, 2]).
    assert_eq!(&split_wet[..2], &[1.0, 1.0]);
    assert_eq!(&split.sink().frames()[..2], &[1.0, 0.0]);
}

#[test]
fn bus_chain_runs_before_master_chain() {
    let ctx = Context::new();
    let drums = ctx.bus("drums").unwrap();
    drums.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    ctx.set_inserts(vec![boxed(Gain { gain: 0.5 })]).unwrap();
    ctx.sink_mut()
        .mix_on(MixDest::Bus(drums.id()), &[1.0], Some(0))
        .unwrap();

    let wet = ctx.render(one_sample(&ctx)).unwrap();
    // bus 1.0 * 2 = 2.0, then master * 0.5 = 1.0
    assert_eq!(&wet[..1], &[1.0]);

    let master_only = Context::new();
    master_only
        .set_inserts(vec![boxed(Gain { gain: 0.5 })])
        .unwrap();
    master_only.sink_mut().mix(&[1.0], Some(0));
    let master_wet = master_only.render(one_sample(&master_only)).unwrap();
    assert_eq!(&master_wet[..1], &[0.5]);
    assert_ne!(&wet[..1], &master_wet[..1]);
}

#[test]
fn reserved_master_name_and_over_cap_keep_existing() {
    let ctx = Context::new();
    match ctx.bus("master") {
        Err(BusError::InvalidName) => {}
        other => panic!("expected InvalidName, got {other:?}"),
    }
    match ctx.bus("") {
        Err(BusError::InvalidName) => {}
        other => panic!("expected InvalidName, got {other:?}"),
    }

    let first = ctx.bus("drums").unwrap();
    first.set_inserts(vec![boxed(Gain { gain: 3.0 })]).unwrap();
    for i in 1..MAX_BUSES {
        ctx.bus(&format!("bus{i}")).unwrap();
    }
    match ctx.bus("overflow") {
        Err(BusError::BusFull { max, got }) => {
            assert_eq!(max, MAX_BUSES);
            assert_eq!(got, MAX_BUSES + 1);
        }
        other => panic!("expected BusFull, got {other:?}"),
    }
    assert_eq!(ctx.bus("drums").unwrap().id(), first.id());
    ctx.sink_mut()
        .mix_on(MixDest::Bus(first.id()), &[1.0], Some(0))
        .unwrap();
    let wet = ctx.render(one_sample(&ctx)).unwrap();
    assert_eq!(&wet[..1], &[3.0]);
}

#[test]
fn unknown_bus_on_other_context_is_err() {
    let ctx_a = Context::new();
    let ctx_b = Context::new();
    let drums = ctx_a.bus("drums").unwrap();
    let mut synth = Synth::new(&ctx_b);
    match synth.trigger_attack_on(&ctx_b, &drums, "C4", Some(0.0.into())) {
        Err(InstrumentError::Bus(BusError::UnknownBus)) => {}
        other => panic!("expected UnknownBus, got {other:?}"),
    }
}

#[test]
fn default_trigger_matches_today_with_unused_bus() {
    let plain = Context::new();
    let mut synth_plain = Synth::new(&plain);
    synth_plain
        .trigger_attack_release(&plain, "A4", 0.02, Some(0.0.into()))
        .unwrap();
    let plain_pcm = plain.render(0.05).unwrap();

    let with_bus = Context::new();
    let _drums = with_bus.bus("drums").unwrap();
    let mut synth_bus = Synth::new(&with_bus);
    synth_bus
        .trigger_attack_release(&with_bus, "A4", 0.02, Some(0.0.into()))
        .unwrap();
    let bus_pcm = with_bus.render(0.05).unwrap();
    assert_eq!(plain_pcm, bus_pcm);
    assert_eq!(plain.sink().frames(), with_bus.sink().frames());
}

#[test]
fn trigger_on_mixes_to_bus_not_master_frames() {
    let ctx = Context::new();
    let drums = ctx.bus("drums").unwrap();
    drums.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    let mut synth = Synth::new(&ctx);
    synth
        .trigger_attack_release_on(&ctx, &drums, "A4", 0.02, Some(0.0.into()))
        .unwrap();
    let dry = ctx.sink().frames().to_vec();
    let wet = ctx.render(0.05).unwrap();
    assert!(dry.iter().all(|&s| s == 0.0) || dry.is_empty());
    let peak = wet.iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()));
    assert!(peak > 0.01);
}

#[test]
fn dropping_bus_handle_keeps_the_bus() {
    let ctx = Context::new();
    {
        let drums = ctx.bus("drums").unwrap();
        drums.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
        ctx.sink_mut()
            .mix_on(MixDest::Bus(drums.id()), &[0.5], Some(0))
            .unwrap();
    }
    let again = ctx.bus("drums").unwrap();
    let wet = ctx.render(one_sample(&ctx)).unwrap();
    assert_eq!(&wet[..1], &[1.0]);
    assert_eq!(again.name(), "drums");
}

#[test]
fn pipewire_bus_fold_and_live_param() {
    struct LiveGain {
        bits: Arc<AtomicU32>,
    }
    impl Insert for LiveGain {
        fn process(&mut self, frames: &mut [f32]) {
            let gain = f32::from_bits(self.bits.load(Ordering::Relaxed));
            for sample in frames {
                *sample *= gain;
            }
        }
    }

    let mut sink = PipeWireSink::new(44100, 1);
    let drums = sink.ensure_bus("drums").unwrap();
    drums.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    sink.mix(&[0.25], Some(0));
    sink.mix_on(MixDest::Bus(drums.id()), &[0.5], Some(0))
        .unwrap();
    let mut out = [0.0f32; 1];
    sink.process(&mut out);
    assert_eq!(out, [1.25]);

    let bits = Arc::new(AtomicU32::new(1.0f32.to_bits()));
    let mut live = PipeWireSink::new(44100, 1);
    let bus = live.ensure_bus("drums").unwrap();
    bus.set_inserts(vec![boxed(LiveGain {
        bits: Arc::clone(&bits),
    })])
    .unwrap();
    live.mix_on(MixDest::Bus(bus.id()), &[1.0], Some(0))
        .unwrap();
    bits.store(0.25f32.to_bits(), Ordering::Relaxed);
    let mut wet = [0.0f32; 1];
    live.process(&mut wet);
    assert_eq!(wet, [0.25]);
}

#[test]
fn pipewire_bus_integrator_ignores_master_hits() {
    let mut shared = PipeWireSink::new(44100, 1);
    let drums = shared.ensure_bus("drums").unwrap();
    drums
        .set_inserts(vec![boxed(Integrator { acc: 0.0 })])
        .unwrap();
    shared
        .mix_on(MixDest::Bus(drums.id()), &[1.0, 1.0], Some(0))
        .unwrap();
    let mut a = [0.0f32; 1];
    shared.process(&mut a);
    let mut b = [0.0f32; 1];
    shared.process(&mut b);
    assert_eq!(a, [1.0]);
    assert_eq!(b, [2.0]);

    let mut split = PipeWireSink::new(44100, 1);
    let drums = split.ensure_bus("drums").unwrap();
    drums
        .set_inserts(vec![boxed(Integrator { acc: 0.0 })])
        .unwrap();
    split.mix(&[1.0], Some(0));
    split
        .mix_on(MixDest::Bus(drums.id()), &[1.0], Some(1))
        .unwrap();
    let mut c = [0.0f32; 1];
    split.process(&mut c);
    let mut d = [0.0f32; 1];
    split.process(&mut d);
    assert_eq!(c, [1.0]);
    assert_eq!(d, [1.0]);
}

#[test]
fn same_name_returns_same_bus() {
    let ctx = Context::new();
    let a = ctx.bus("drums").unwrap();
    let b = ctx.bus("drums").unwrap();
    assert_eq!(a.id(), b.id());
    assert_eq!(a.name(), "drums");
}
