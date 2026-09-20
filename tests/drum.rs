use drywet::instrument::InstrumentError;
use drywet::{Context, Drum};

#[test]
fn drum_named_voices_and_unknown() {
    let ctx = Context::new();
    let mut drum = Drum::new(&ctx);
    drum.trigger(&ctx, "kick", Some(0.0.into())).unwrap();
    drum.trigger(&ctx, "snare", Some(0.05.into())).unwrap();
    drum.trigger_attack_release(&ctx, "hat", 0.02, Some(0.1.into()))
        .unwrap();
    let peak = ctx
        .sink()
        .frames()
        .iter()
        .fold(0.0_f32, |acc, &s| acc.max(s.abs()));
    assert!(peak > 0.01);
    assert!(matches!(
        drum.trigger(&ctx, "cowbell", None),
        Err(InstrumentError::UnknownDrum(name)) if name == "cowbell"
    ));
}

#[test]
fn drum_beat_grid_sixteenths() {
    assert_eq!(Drum::steps_per_bar((4, 4)), 16);
    assert_eq!(Drum::steps_per_bar((3, 4)), 12);
    assert_eq!(Drum::steps_per_bar((6, 8)), 12);
}

#[test]
fn drum_hat_aliases_and_noop_release() {
    let ctx = Context::new();
    let mut drum = Drum::new(&ctx);
    drum.trigger_attack(&ctx, "hi-hat", Some(0.0.into()))
        .unwrap();
    drum.trigger(&ctx, "hihat", Some(0.02.into())).unwrap();
    drum.trigger_release("hat", None);
    drum.release_all(None);
    let peak = ctx
        .sink()
        .frames()
        .iter()
        .fold(0.0_f32, |acc, &s| acc.max(s.abs()));
    assert!(peak > 0.01);
}
