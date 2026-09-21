//! NDJSON stdio host: one JSON command per stdin line, one JSON reply per stdout line.
//!
//! Omarchy QML (and any other process host) spawns `drywet-engine` and writes
//! the verbs in this module. In-process GUIs skip this adapter and call
//! [`crate::Context`] directly. Worked examples: `docs/gui.md`.

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

use serde_json::{json, Value};

use crate::context::Context;
use crate::event::{Loop, Part, Sequence, SequenceEvent};
use crate::instrument::{Drum, Sampler, Synth};
use crate::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};
use crate::sink::Sink;
use crate::time::TimeValue;

/// Selected warmup instrument. Sequence callbacks share this via `Rc<RefCell<_>>`.
enum LiveInstrument {
    Synth(Synth),
    Drum(Drum),
    Sampler {
        sampler: Sampler,
        #[allow(dead_code)]
        loop_flag: bool,
    },
}

impl LiveInstrument {
    fn from_msg<S: Sink>(ctx: &Context<S>, msg: &Value) -> Result<Self, String> {
        let kind = msg
            .get("instrument")
            .and_then(Value::as_str)
            .unwrap_or("synth");
        match kind {
            "synth" => Ok(LiveInstrument::Synth(Synth::new(ctx))),
            "drum" => Ok(LiveInstrument::Drum(Drum::new(ctx))),
            "sampler" => {
                let loop_flag = msg.get("loop").and_then(Value::as_bool).unwrap_or(false);
                if let Some(directory) = msg.get("directory").and_then(Value::as_str) {
                    let sampler = Sampler::from_directory(ctx, directory).map_err(err_str)?;
                    return Ok(LiveInstrument::Sampler { sampler, loop_flag });
                }
                let mut sampler = Sampler::new(ctx);
                if let Some(map) = msg
                    .get("map")
                    .or_else(|| msg.get("urls"))
                    .and_then(Value::as_object)
                {
                    for (note, path) in map {
                        let path = path
                            .as_str()
                            .ok_or_else(|| format!("sampler path for {note} must be a string"))?;
                        sampler.add(note.as_str(), path).map_err(err_str)?;
                    }
                }
                Ok(LiveInstrument::Sampler { sampler, loop_flag })
            }
            other => Err(format!("unknown instrument: {other:?}")),
        }
    }

    fn trigger_attack_release<S: Sink>(
        &mut self,
        ctx: &Context<S>,
        note: &str,
        duration: TimeValue,
        time: Option<TimeValue>,
    ) -> Result<(), String> {
        match self {
            LiveInstrument::Synth(synth) => synth
                .trigger_attack_release(ctx, note, duration, time)
                .map(|_| ())
                .map_err(err_str),
            LiveInstrument::Drum(drum) => drum
                .trigger_attack_release(ctx, note, duration, time)
                .map(|_| ())
                .map_err(err_str),
            LiveInstrument::Sampler { sampler, .. } => sampler
                .trigger_attack_release(ctx, note, duration, time)
                .map(|_| ())
                .map_err(err_str),
        }
    }

    fn trigger_attack<S: Sink>(
        &mut self,
        ctx: &Context<S>,
        note: &str,
        time: Option<TimeValue>,
    ) -> Result<(), String> {
        match self {
            LiveInstrument::Synth(synth) => synth
                .trigger_attack(ctx, note, time)
                .map(|_| ())
                .map_err(err_str),
            LiveInstrument::Drum(drum) => drum
                .trigger_attack(ctx, note, time)
                .map(|_| ())
                .map_err(err_str),
            LiveInstrument::Sampler { sampler, .. } => sampler
                .trigger_attack(ctx, note, time)
                .map(|_| ())
                .map_err(err_str),
        }
    }

    fn trigger_release(&mut self, note: &str, time: Option<TimeValue>) -> Result<(), String> {
        match self {
            LiveInstrument::Synth(synth) => synth
                .trigger_release(note, time)
                .map(|_| ())
                .map_err(err_str),
            LiveInstrument::Drum(drum) => {
                drum.trigger_release(note, time);
                Ok(())
            }
            LiveInstrument::Sampler { sampler, .. } => sampler
                .trigger_release(note, time)
                .map(|_| ())
                .map_err(err_str),
        }
    }
}

fn err_str(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn emit<W: Write>(stdout: &mut W, value: &Value) -> std::io::Result<()> {
    writeln!(stdout, "{value}")?;
    stdout.flush()
}

fn emit_ok<W: Write>(stdout: &mut W) -> std::io::Result<()> {
    emit(stdout, &json!({"ok": true}))
}

fn emit_error<W: Write>(stdout: &mut W, message: impl std::fmt::Display) -> std::io::Result<()> {
    emit(stdout, &json!({"error": message.to_string()}))
}

fn note_from_msg(msg: &Value) -> String {
    match msg.get("note") {
        Some(Value::String(note)) => note.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => "C4".to_string(),
    }
}

fn duration_from_msg(msg: &Value) -> TimeValue {
    match msg.get("duration") {
        Some(Value::String(text)) => TimeValue::from(text.as_str()),
        Some(Value::Number(n)) => TimeValue::from(n.as_f64().unwrap_or(0.0)),
        _ => TimeValue::from("8n"),
    }
}

fn time_from_msg(msg: &Value) -> Result<Option<TimeValue>, String> {
    match msg.get("time") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => json_to_time(value).map(Some),
    }
}

fn json_to_time(value: &Value) -> Result<TimeValue, String> {
    match value {
        Value::String(text) => Ok(TimeValue::from(text.as_str())),
        Value::Number(n) => n
            .as_f64()
            .map(TimeValue::from)
            .ok_or_else(|| "invalid time".to_string()),
        _ => Err("invalid time".to_string()),
    }
}

fn json_f64(value: &Value) -> Result<f64, String> {
    match value {
        Value::Number(n) => n.as_f64().ok_or_else(|| "invalid number".to_string()),
        Value::String(text) => text
            .parse::<f64>()
            .map_err(|err| format!("invalid number: {err}")),
        _ => Err("expected number".to_string()),
    }
}

fn json_to_note(value: &Value) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Null => Err("event must not be null".to_string()),
        _ => Err("invalid event value".to_string()),
    }
}

fn parse_sequence_event(value: &Value) -> Result<SequenceEvent, String> {
    match value {
        Value::Null => Ok(SequenceEvent::Rest),
        Value::String(text) => Ok(SequenceEvent::Value(text.clone())),
        Value::Number(n) => Ok(SequenceEvent::Value(n.to_string())),
        Value::Array(items) => {
            let inner = items
                .iter()
                .map(parse_sequence_event)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SequenceEvent::Group(inner))
        }
        _ => Err("invalid sequence event".to_string()),
    }
}

fn parse_sequence_events(value: &Value) -> Result<Vec<SequenceEvent>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| "events must be a list".to_string())?;
    items.iter().map(parse_sequence_event).collect()
}

fn parse_part_events(value: &Value) -> Result<Vec<(TimeValue, String)>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| "part events must be a list".to_string())?;
    let mut out = Vec::with_capacity(items.len());
    for event in items {
        if let Some(pair) = event.as_array() {
            if pair.len() < 2 {
                return Err("part event pair needs time and event".to_string());
            }
            out.push((json_to_time(&pair[0])?, json_to_note(&pair[1])?));
        } else if let Some(obj) = event.as_object() {
            let time = obj
                .get("time")
                .ok_or_else(|| "part event missing time".to_string())?;
            let note = obj
                .get("event")
                .or_else(|| obj.get("note"))
                .ok_or_else(|| "part event missing event".to_string())?;
            out.push((json_to_time(time)?, json_to_note(note)?));
        } else {
            return Err("invalid part event".to_string());
        }
    }
    Ok(out)
}

fn apply_transport_loop<S: Sink>(ctx: &Context<S>, msg: &Value) {
    match msg.get("loop") {
        Some(Value::Bool(enabled)) => ctx.transport().set_loop(*enabled),
        Some(Value::Object(_)) => {}
        Some(Value::Null) | None => ctx.transport().set_loop(false),
        Some(other) => ctx.transport().set_loop(other.as_bool().unwrap_or(true)),
    }
}

fn attach_schedule<S: Sink + 'static>(
    ctx: &Rc<Context<S>>,
    inst: &Rc<RefCell<LiveInstrument>>,
    msg: &Value,
) -> Result<(), String> {
    if let Some(spec) = msg.get("sequence").and_then(Value::as_object) {
        let events = parse_sequence_events(spec.get("events").unwrap_or(&json!([])))?;
        let subdivision = spec
            .get("subdivision")
            .and_then(Value::as_str)
            .unwrap_or("4n");
        let ctx_cb = Rc::clone(ctx);
        let inst_cb = Rc::clone(inst);
        let mut sequence = Sequence::new(
            move |time, note| {
                if let Some(note) = note {
                    let _ = inst_cb.borrow_mut().trigger_attack_release(
                        ctx_cb.as_ref(),
                        note,
                        TimeValue::from("8n"),
                        Some(TimeValue::from(time)),
                    );
                }
            },
            events,
            subdivision,
        );
        sequence.start(&mut ctx.transport(), 0.0).map_err(err_str)?;
    }

    if let Some(spec) = msg.get("part").and_then(Value::as_object) {
        let events = parse_part_events(spec.get("events").unwrap_or(&json!([])))?;
        let ctx_cb = Rc::clone(ctx);
        let inst_cb = Rc::clone(inst);
        let mut part = Part::new(
            move |time, note| {
                let _ = inst_cb.borrow_mut().trigger_attack_release(
                    ctx_cb.as_ref(),
                    note,
                    TimeValue::from("8n"),
                    Some(TimeValue::from(time)),
                );
            },
            events,
        );
        part.start(&mut ctx.transport(), 0.0).map_err(err_str)?;
    }

    if let Some(spec) = msg.get("loop").and_then(Value::as_object) {
        let interval = spec.get("interval").and_then(Value::as_str).unwrap_or("4n");
        let ctx_cb = Rc::clone(ctx);
        let inst_cb = Rc::clone(inst);
        let mut looper = Loop::new(
            move |time| {
                let _ = inst_cb.borrow_mut().trigger_attack_release(
                    ctx_cb.as_ref(),
                    "C4",
                    TimeValue::from("8n"),
                    Some(TimeValue::from(time)),
                );
            },
            interval,
        );
        looper.start(&mut ctx.transport(), 0.0).map_err(err_str)?;
    }

    Ok(())
}

fn unknown_cmd(msg: &Value) -> String {
    match msg.get("cmd") {
        Some(Value::String(cmd)) => format!("unknown cmd: {cmd}"),
        Some(other) => format!("unknown cmd: {other}"),
        None => "unknown cmd: null".to_string(),
    }
}

fn ensure_clock<S: Sink>(ctx: &Context<S>) {
    ctx.sink_mut().start_clock();
}

/// Read NDJSON commands from `stdin` and write one flushed JSON reply per line.
///
/// Returns the context so in-process tests can inspect the sink after `run`.
/// `shutdown` disposes the transport and returns; EOF also returns without dispose.
pub fn run<S, R, W>(stdin: R, stdout: W, sink: S) -> std::io::Result<Rc<Context<S>>>
where
    S: Sink + 'static,
    R: BufRead,
    W: Write,
{
    let ctx = Rc::new(Context::with(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS, sink));
    let instrument = Rc::new(RefCell::new(LiveInstrument::Synth(Synth::new(
        ctx.as_ref(),
    ))));
    let mut stdout = stdout;

    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(err) => {
                emit_error(&mut stdout, err)?;
                continue;
            }
        };
        if !msg.is_object() {
            emit_error(&mut stdout, "command must be a JSON object")?;
            continue;
        }

        let cmd = msg.get("cmd").and_then(Value::as_str);
        let result = match cmd {
            Some("warmup") => handle_warmup(&ctx, &instrument, &msg),
            Some("start") => handle_start(&ctx, &instrument, &msg),
            Some("stop") => {
                ctx.transport().stop();
                Ok(json!({"ok": true}))
            }
            Some("pause") => {
                ctx.transport().pause();
                Ok(json!({"ok": true}))
            }
            Some("resume") => {
                ctx.transport().start();
                Ok(json!({"ok": true}))
            }
            Some("play-midi") => handle_play_midi(&ctx, &instrument, &msg),
            Some("note-on") | Some("trigger_attack") => handle_note_on(&ctx, &instrument, &msg),
            Some("note-off") | Some("trigger_release") => handle_note_off(&instrument, &msg),
            Some("bpm") => handle_bpm(&ctx, &msg),
            Some("shutdown") => {
                ctx.transport().dispose();
                emit_ok(&mut stdout)?;
                return Ok(ctx);
            }
            _ => Err(unknown_cmd(&msg)),
        };

        match result {
            Ok(reply) => emit(&mut stdout, &reply)?,
            Err(err) => emit_error(&mut stdout, err)?,
        }
    }

    Ok(ctx)
}

fn handle_warmup<S: Sink>(
    ctx: &Rc<Context<S>>,
    instrument: &Rc<RefCell<LiveInstrument>>,
    msg: &Value,
) -> Result<Value, String> {
    let next = LiveInstrument::from_msg(ctx.as_ref(), msg)?;
    *instrument.borrow_mut() = next;
    ensure_clock(ctx.as_ref());
    Ok(json!({"ok": true}))
}

fn handle_start<S: Sink + 'static>(
    ctx: &Rc<Context<S>>,
    instrument: &Rc<RefCell<LiveInstrument>>,
    msg: &Value,
) -> Result<Value, String> {
    if let Some(bpm) = msg.get("bpm") {
        ctx.transport().set_bpm(json_f64(bpm)?).map_err(err_str)?;
    }
    apply_transport_loop(ctx.as_ref(), msg);
    attach_schedule(ctx, instrument, msg)?;
    ensure_clock(ctx.as_ref());
    let mut transport = ctx.transport();
    transport.start();
    Ok(json!({
        "event": "started",
        "latencyMs": transport.latency_ms(),
        "position": transport.position(),
    }))
}

fn handle_play_midi<S: Sink>(
    ctx: &Rc<Context<S>>,
    instrument: &Rc<RefCell<LiveInstrument>>,
    msg: &Value,
) -> Result<Value, String> {
    ensure_clock(ctx.as_ref());
    instrument.borrow_mut().trigger_attack_release(
        ctx.as_ref(),
        &note_from_msg(msg),
        duration_from_msg(msg),
        None,
    )?;
    Ok(json!({"ok": true}))
}

fn handle_note_on<S: Sink>(
    ctx: &Rc<Context<S>>,
    instrument: &Rc<RefCell<LiveInstrument>>,
    msg: &Value,
) -> Result<Value, String> {
    ensure_clock(ctx.as_ref());
    instrument.borrow_mut().trigger_attack(
        ctx.as_ref(),
        &note_from_msg(msg),
        time_from_msg(msg)?,
    )?;
    Ok(json!({"ok": true}))
}

fn handle_note_off(instrument: &Rc<RefCell<LiveInstrument>>, msg: &Value) -> Result<Value, String> {
    instrument
        .borrow_mut()
        .trigger_release(&note_from_msg(msg), time_from_msg(msg)?)?;
    Ok(json!({"ok": true}))
}

fn handle_bpm<S: Sink>(ctx: &Rc<Context<S>>, msg: &Value) -> Result<Value, String> {
    if let Some(value) = msg.get("value") {
        ctx.transport().set_bpm(json_f64(value)?).map_err(err_str)?;
    }
    Ok(json!({"ok": true}))
}
