use drywet::{Context, Synth};

#[test]
fn synth_trigger_attack_release_mixes_nonzero_pcm() {
    let mut ctx = Context::new();
    let mut synth = Synth::new(&ctx);
    synth
        .trigger_attack_release(&mut ctx, "A4", 0.05, Some(0.0))
        .unwrap();
    let frames = ctx.sink().frames();
    assert!(frames.len() >= (0.05 * f64::from(ctx.sample_rate())) as usize);
    let peak = frames.iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()));
    assert!(peak > 0.01);
}

#[test]
fn synth_polyphony_cap_and_release_all() {
    let mut ctx = Context::new();
    let mut synth = Synth::with_max_voices(&ctx, 2);
    synth.trigger_attack(&mut ctx, "C4", Some(0.0)).unwrap();
    synth.trigger_attack(&mut ctx, "E4", Some(0.0)).unwrap();
    assert!(synth.trigger_attack(&mut ctx, "G4", Some(0.0)).is_err());
    synth.release_all(Some(0.01));
    assert_eq!(synth.active_voices(), 0);
}
