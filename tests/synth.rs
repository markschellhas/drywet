use drywet::{Context, Synth};

#[test]
fn synth_trigger_attack_release_mixes_nonzero_pcm() {
    let ctx = Context::new();
    let mut synth = Synth::new(&ctx);
    synth
        .trigger_attack_release(&ctx, "A4", 0.05, Some(0.0.into()))
        .unwrap();
    let sink = ctx.sink();
    let frames = sink.frames();
    assert!(frames.len() >= (0.05 * f64::from(ctx.sample_rate())) as usize);
    let peak = frames.iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()));
    assert!(peak > 0.01);
}

#[test]
fn synth_polyphony_cap_and_release_all() {
    let ctx = Context::new();
    let mut synth = Synth::with_max_voices(&ctx, 2);
    synth.trigger_attack(&ctx, "C4", Some(0.0.into())).unwrap();
    synth.trigger_attack(&ctx, "E4", Some(0.0.into())).unwrap();
    assert!(synth.trigger_attack(&ctx, "G4", Some(0.0.into())).is_err());
    synth.release_all(None);
    assert_eq!(synth.active_voices(), 0);
}

#[test]
fn synth_trigger_attack_release_none_mixes_at_write_cursor() {
    let ctx = Context::new();
    let mut synth = Synth::new(&ctx);
    let prefix = [0.5_f32, 0.25, -0.25];
    ctx.sink_mut().write(&prefix);
    let cursor = ctx.sink().write_cursor();
    assert_eq!(cursor, prefix.len());

    synth
        .trigger_attack_release(&ctx, "A4", 0.05, None)
        .unwrap();

    {
        let sink = ctx.sink();
        let frames = sink.frames();
        assert_eq!(&frames[..cursor], &prefix[..]);
        let tail = &frames[cursor..];
        assert!(tail.len() >= (0.05 * f64::from(ctx.sample_rate())) as usize);
        let peak = tail.iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()));
        assert!(peak > 0.01);
    }

    synth.trigger_release("A4", None).unwrap();
    synth.release_all(None);
    synth.trigger_attack(&ctx, "C4", None).unwrap();
}
