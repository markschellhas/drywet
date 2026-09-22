use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use drywet::insert::{apply_interleaved, Insert, InsertChain, InsertError};
use drywet::limits::MAX_INSERTS;

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

struct Add {
    delta: f32,
}

impl Insert for Add {
    fn process(&mut self, frames: &mut [f32]) {
        for sample in frames.iter_mut() {
            *sample += self.delta;
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

#[test]
fn empty_chain_is_identity() {
    let mut chain = InsertChain::new();
    let mut frames = [0.25, -0.5];
    chain.process(&mut frames);
    assert_eq!(frames, [0.25, -0.5]);
}

#[test]
fn gain_scales_the_block() {
    let mut chain = InsertChain::new();
    chain.set(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    let mut frames = [0.25, -0.5];
    chain.process(&mut frames);
    assert_eq!(frames, [0.5, -1.0]);
}

#[test]
fn chain_order_is_playback_order() {
    let mut add_then_gain = InsertChain::new();
    add_then_gain
        .set(vec![boxed(Add { delta: 1.0 }), boxed(Gain { gain: 2.0 })])
        .unwrap();
    let mut a = [1.0];
    add_then_gain.process(&mut a);

    let mut gain_then_add = InsertChain::new();
    gain_then_add
        .set(vec![boxed(Gain { gain: 2.0 }), boxed(Add { delta: 1.0 })])
        .unwrap();
    let mut b = [1.0];
    gain_then_add.process(&mut b);

    assert_eq!(a, [4.0]);
    assert_eq!(b, [3.0]);
}

#[test]
fn integrator_keeps_state_across_calls() {
    let mut chain = InsertChain::new();
    chain.set(vec![boxed(Integrator { acc: 0.0 })]).unwrap();
    let mut first = [1.0];
    chain.process(&mut first);
    let mut second = [1.0];
    chain.process(&mut second);
    assert_eq!(first, [1.0]);
    assert_eq!(second, [2.0]);
}

#[test]
fn process_chunks_match_one_call() {
    let mut whole = InsertChain::new();
    whole.set(vec![boxed(Integrator { acc: 0.0 })]).unwrap();
    let mut all = [0.5, 0.5];
    whole.process(&mut all);

    let mut parts = InsertChain::new();
    parts.set(vec![boxed(Integrator { acc: 0.0 })]).unwrap();
    let mut a = [0.5];
    let mut b = [0.5];
    parts.process(&mut a);
    parts.process(&mut b);

    assert_eq!(all, [0.5, 1.0]);
    assert_eq!([a[0], b[0]], all);
}

#[test]
fn stereo_apply_processes_mono_then_duplicates() {
    let mut chain = InsertChain::new();
    chain.set(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    let mut interleaved = [0.25, 0.25, 0.5, 0.5];
    apply_interleaved(&mut chain, &mut interleaved, 2);
    assert_eq!(interleaved, [0.5, 0.5, 1.0, 1.0]);
}

#[test]
fn chain_full_keeps_previous_inserts() {
    let mut chain = InsertChain::new();
    chain.set(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    let too_many: Vec<Box<dyn Insert>> = (0..=MAX_INSERTS)
        .map(|_| boxed(Gain { gain: 3.0 }))
        .collect();
    match chain.set(too_many) {
        Err(InsertError::ChainFull { max, got }) => {
            assert_eq!(max, MAX_INSERTS);
            assert_eq!(got, MAX_INSERTS + 1);
        }
        other => panic!("expected ChainFull, got {other:?}"),
    }
    let mut frames = [1.0];
    chain.process(&mut frames);
    assert_eq!(frames, [2.0]);
}

#[test]
fn live_gain_handle_changes_next_block() {
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

    let bits = Arc::new(AtomicU32::new(1.0f32.to_bits()));
    let mut chain = InsertChain::new();
    chain
        .set(vec![boxed(LiveGain {
            bits: Arc::clone(&bits),
        })])
        .unwrap();
    let mut first = [1.0];
    chain.process(&mut first);
    bits.store(0.5f32.to_bits(), Ordering::Relaxed);
    let mut second = [1.0];
    chain.process(&mut second);
    assert_eq!(first, [1.0]);
    assert_eq!(second, [0.5]);
}
