//! Offline render through BufferSink. Writes `phrase.wav` in the cwd.
//!
//! ```text
//! cargo run --example render
//! ```

use drywet::{Context, ContextConfig, Sequence, Synth};

fn main() -> drywet::Result<()> {
    let ctx = Context::new(ContextConfig::default());
    let synth = Synth::new(&ctx);
    let seq = Sequence::new(
        |time, note| synth.trigger_attack_release(note, "8n", Some(time)),
        ["C4", "E4", "G4", "C5"],
        "4n",
    );
    seq.start(0)?;
    ctx.transport().set_bpm(120)?;
    let pcm = ctx.transport().render("1m")?;
    let expected = (ctx.to_seconds("1m")? * ctx.sample_rate() as f64) as usize;
    assert_eq!(pcm.len(), expected);
    assert!(pcm.iter().any(|sample| sample.abs() > 0.0));

    if let Some(sink) = ctx.buffer_sink() {
        std::fs::write("phrase.pcm", sink.to_pcm_s16le())?;
        println!("wrote phrase.pcm ({} frames, {} Hz)", pcm.len(), ctx.sample_rate());
    }
    Ok(())
}
