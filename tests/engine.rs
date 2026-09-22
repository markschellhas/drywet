use std::io::Cursor;
use std::process::{Command, Stdio};
use std::rc::Rc;

use drywet::{run, BufferSink, Context};
use serde_json::{json, Value};

fn send(commands: &[Value]) -> (Vec<Value>, Rc<Context<BufferSink>>) {
    let stdin = commands
        .iter()
        .map(|cmd| format!("{cmd}\n"))
        .collect::<String>();
    let mut stdout = Vec::new();
    let ctx = run(Cursor::new(stdin), &mut stdout, BufferSink::new(44100, 1)).expect("engine run");
    let lines = String::from_utf8(stdout)
        .expect("utf8 stdout")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("json line"))
        .collect();
    (lines, ctx)
}

fn peak(frames: &[f32]) -> f32 {
    frames
        .iter()
        .fold(0.0_f32, |acc, &sample| acc.max(sample.abs()))
}

#[test]
fn engine_warmup_start_play_stop_shutdown() {
    let (lines, ctx) = send(&[
        json!({"cmd": "warmup", "instrument": "synth"}),
        json!({"cmd": "start", "bpm": 120, "loop": false}),
        json!({"cmd": "play-midi", "note": "C4", "duration": "8n"}),
        json!({"cmd": "bpm", "value": 100}),
        json!({"cmd": "pause"}),
        json!({"cmd": "resume"}),
        json!({"cmd": "stop"}),
        json!({"cmd": "shutdown"}),
    ]);

    let started = lines
        .iter()
        .find(|row| row.get("event") == Some(&json!("started")))
        .expect("started event");
    assert!(started.get("latencyMs").is_some());
    assert_eq!(started.get("latencyMs"), Some(&json!(0)));
    assert!(started.get("position").is_some());
    assert!(lines.iter().any(|row| row.get("ok") == Some(&json!(true))));
    assert!(peak(ctx.sink().frames()) > 0.0);
}

#[test]
fn engine_sequence_payload_and_error() {
    let (lines, ctx) = send(&[
        json!({"cmd": "warmup", "instrument": "synth"}),
        json!({
            "cmd": "start",
            "bpm": 120,
            "sequence": {"events": ["C4", "G3"], "subdivision": "4n"},
        }),
        json!({"cmd": "shutdown"}),
    ]);
    assert!(lines
        .iter()
        .any(|row| row.get("event") == Some(&json!("started"))));
    assert!(
        peak(ctx.sink().frames()) > 0.0,
        "start + sequence must mix the first phrase"
    );

    let mut err_out = Vec::new();
    run(
        Cursor::new(json!({"cmd": "nope"}).to_string() + "\n"),
        &mut err_out,
        BufferSink::new(44100, 1),
    )
    .expect("unknown cmd run");
    let first = String::from_utf8(err_out)
        .expect("utf8")
        .lines()
        .next()
        .expect("error line")
        .to_string();
    let payload: Value = serde_json::from_str(&first).expect("error json");
    assert!(payload.get("error").is_some());
}

#[test]
fn engine_note_on_after_stop_keeps_stream() {
    let (lines, ctx) = send(&[
        json!({"cmd": "warmup", "instrument": "synth"}),
        json!({"cmd": "start", "bpm": 120}),
        json!({"cmd": "stop"}),
        json!({"cmd": "note-on", "note": "C4"}),
    ]);
    assert!(lines.iter().all(|row| row.get("error").is_none()));
    assert!(peak(ctx.sink().frames()) > 0.01);
}

#[test]
fn engine_note_off_and_trigger_aliases() {
    let (lines, _ctx) = send(&[
        json!({"cmd": "warmup", "instrument": "synth"}),
        json!({"cmd": "note-on", "note": "E4"}),
        json!({"cmd": "note-off", "note": "E4"}),
        json!({"cmd": "trigger_attack", "note": "G4"}),
        json!({"cmd": "trigger_release", "note": "G4"}),
        json!({"cmd": "shutdown"}),
    ]);
    assert!(lines.iter().all(|row| row.get("error").is_none()));
    assert!(lines.iter().any(|row| row.get("ok") == Some(&json!(true))));
}

#[test]
fn engine_part_and_loop_payload() {
    let (lines, ctx) = send(&[
        json!({"cmd": "warmup", "instrument": "synth"}),
        json!({
            "cmd": "start",
            "part": {"events": [["0:0:0", "C4"], {"time": "4n", "event": "E4"}]},
            "loop": {"interval": "4n"},
        }),
        json!({"cmd": "shutdown"}),
    ]);
    assert!(lines
        .iter()
        .any(|row| row.get("event") == Some(&json!("started"))));
    assert!(lines.iter().all(|row| row.get("error").is_none()));
    assert!(peak(ctx.sink().frames()) > 0.0);
}

#[test]
fn engine_looped_sequence_mixes_second_cycle() {
    let (lines, ctx) = send(&[
        json!({"cmd": "warmup", "instrument": "synth"}),
        json!({
            "cmd": "start",
            "bpm": 120,
            "loop": true,
            "sequence": {"events": ["C4", "E4", "G4", "B4"], "subdivision": "4n"},
        }),
        json!({"cmd": "shutdown"}),
    ]);
    assert!(lines
        .iter()
        .any(|row| row.get("event") == Some(&json!("started"))));
    let frames = ctx.sink().frames();
    let bar = (2.0 * f64::from(ctx.sample_rate())) as usize;
    assert!(
        frames.len() > bar,
        "expected mix past the first bar, got {} frames",
        frames.len()
    );
    assert!(
        peak(&frames[bar..]) > 0.0,
        "loop:true must mix a second cycle after the phrase length"
    );
}

#[test]
fn engine_spawn_buffer_warmup_shutdown() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_drywet-engine"))
        .arg("--buffer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn drywet-engine --buffer");
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("child stdin");
        writeln!(stdin, r#"{{"cmd":"warmup","instrument":"synth"}}"#).unwrap();
        writeln!(stdin, r#"{{"cmd":"start","bpm":120}}"#).unwrap();
        writeln!(stdin, r#"{{"cmd":"shutdown"}}"#).unwrap();
    }
    let output = child.wait_with_output().expect("engine exit");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("spawn json"))
        .collect();
    assert!(lines
        .iter()
        .any(|row| row.get("event") == Some(&json!("started"))));
    assert!(lines.iter().any(|row| row.get("ok") == Some(&json!(true))));
}
