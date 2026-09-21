# Hooking a GUI up to drywet

drywet is a musician runtime, not a widget toolkit. A UI owns buttons,
playheads, and layout. This crate owns Transport, time, and voices.

There are two host paths:

| Host | How it talks to drywet | When to use it |
|------|------------------------|----------------|
| **Ply** (or egui) | Link the `drywet` crate. Call `Context` / `Synth` / `Sampler` on the UI thread. | A standalone Rust app. |
| **Omarchy QML** | Spawn `drywet-engine` once. Speak NDJSON on stdin/stdout. | A bar widget, panel, or menu inside `omarchy-shell`. |

`omarchy-shell` loads QML. It does not load a Rust library into the bar.
The same NDJSON verbs work from any other process host (Python, a
terminal, a test). In-process apps skip the binary and import `drywet`.

This repo does **not** ship QML, an Omarchy `manifest.json`, or a Ply
window. Those live in the app or plugin that vendors a `drywet-engine`
binary (or depends on the crate).

---

## 1. In-process: Ply

[Ply](https://plyx.iz.rs/docs/getting-started/) is a Rust UI engine
(`ply-engine`). One window loop, GPU-backed, builder-style elements.
Use it when you want pads or a transport in a normal desktop app.

`Context` and `Synth` are `!Send` (`RefCell`). Keep them on the Ply
thread. Ply's `.on_press()` callbacks are `'static`, so share state with
`Rc` / `RefCell` — the same pattern the NDJSON engine uses internally.

`plyx init` scaffolds the window config and font. The drywet-specific
part is: construct one `Context`, start the sink clock, then trigger
notes from pad presses. Omit `time` so the hit mixes at the write cursor
without stopping playback.

```rust
use std::cell::RefCell;
use std::rc::Rc;

use drywet::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};
use drywet::{Context, PipeWireSink, Sink, Synth};
use ply_engine::prelude::*;

struct Engine {
    ctx: Context<PipeWireSink>,
    synth: RefCell<Synth>,
}

impl Engine {
    fn new() -> Self {
        let ctx = Context::with(
            DEFAULT_SAMPLE_RATE,
            DEFAULT_CHANNELS,
            PipeWireSink::new(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS),
        );
        let synth = Synth::new(&ctx);
        ctx.sink_mut().start_clock();
        ctx.transport().set_bpm(120).ok();
        ctx.transport().start();
        Self {
            ctx,
            synth: RefCell::new(synth),
        }
    }

    fn hit(&self, note: &str) {
        let _ = self
            .synth
            .borrow_mut()
            .trigger_attack_release(&self.ctx, note, "8n", None);
    }

    fn toggle(&self) {
        self.ctx.transport().toggle();
    }

    fn position(&self) -> String {
        self.ctx.transport().position()
    }
}

fn pad(ui: &mut Ui, engine: &Rc<Engine>, note: &'static str) {
    let engine = Rc::clone(engine);
    ui.element()
        .width(fixed!(72.0))
        .height(fixed!(72.0))
        .corner_radius(8.0)
        .accessibility(|a| a.button(note))
        .on_press(move |_, _| engine.hit(note))
        .children(|ui| {
            let bg = if ui.pressed() {
                0xB91414
            } else if ui.hovered() || ui.focused() {
                0xFF654D
            } else {
                0x3A3533
            };
            ui.element()
                .width(grow!())
                .height(grow!())
                .background_color(bg)
                .corner_radius(8.0)
                .layout(|l| l.align(CenterX, CenterY))
                .children(|ui| {
                    ui.text(note, |t| t.font_size(16).color(0xFFFFFF));
                });
        });
}

// Inside the Ply frame loop, after ply.begin():
fn pads(ui: &mut Ui, engine: &Rc<Engine>) {
    ui.element()
        .width(grow!())
        .height(grow!())
        .layout(|l| l.direction(TopToBottom).align(CenterX, CenterY).gap(12))
        .children(|ui| {
            ui.text(&format!("pos {}", engine.position()), |t| {
                t.font_size(14).color(0xC8C2BC)
            });

            let engine_toggle = Rc::clone(engine);
            ui.element()
                .width(fit!())
                .height(fixed!(32.0))
                .on_press(move |_, _| engine_toggle.toggle())
                .children(|ui| {
                    ui.text("start / stop", |t| t.font_size(14).color(0xFFFFFF));
                });

            ui.element()
                .width(fit!())
                .height(fit!())
                .layout(|l| l.direction(LeftToRight).gap(8))
                .children(|ui| {
                    for note in ["C4", "D4", "E4", "G4"] {
                        pad(ui, engine, note);
                    }
                });
        });
}
```

Construct `Rc::new(Engine::new())` once before the frame loop. Each
frame, read `engine.position()` (bars:beats:sixteenths) if you want a
playhead. Offset wall-clock animations by
`engine.ctx.transport().latency_ms()`.

To run a `Sequence` instead of only live hits, attach it to the
transport **before** `start()`, and pass the callback `time` into the
trigger so notes lock to the sample clock:

```rust
let ctx_cb = Rc::clone(&ctx);
let synth_cb = Rc::clone(&synth);
let mut seq = drywet::Sequence::new(
    move |time, note| {
        if let Some(note) = note {
            let _ = synth_cb.borrow_mut().trigger_attack_release(
                ctx_cb.as_ref(),
                note,
                "8n",
                Some(drywet::time::TimeValue::from(time)),
            );
        }
    },
    ["C4", "G3", "A3", "F3"],
    "4n",
);
seq.start(&mut ctx.transport(), 0.0)?;
ctx.transport().start();
```

Live pad hits still omit `time`. They mix now and do not cancel the
arrangement.

### egui (same crate, different widgets)

[egui](https://www.egui.rs/) is the other common Rust immediate-mode
choice. The drywet side is identical: one `Context` on the UI thread,
`None` for live time.

```rust
use std::cell::RefCell;
use std::rc::Rc;

use drywet::{Context, Sink, Synth};

let ctx = Rc::new(Context::new()); // BufferSink in tests; swap for PipeWireSink to hear it
let synth = Rc::new(RefCell::new(Synth::new(ctx.as_ref())));
ctx.sink_mut().start_clock();
ctx.transport().start();

// inside eframe / egui::CentralPanel:
for note in ["C4", "D4", "E4", "G4"] {
    if ui.button(note).clicked() {
        let _ = synth.borrow_mut().trigger_attack_release(
            ctx.as_ref(),
            note,
            "8n",
            None,
        );
    }
}
ui.label(format!("pos {}", ctx.transport().position()));
```

Iced, Slint, and Relm4 follow the same rule: do not put `Context` on a
worker thread; fire notes from the UI thread (or send note names over a
channel *to* the thread that owns `Context`).

---

## 2. Omarchy QML: spawn `drywet-engine`

An Omarchy plugin is a directory with a `manifest.json` and QML, loaded
by `omarchy-shell` ([shell plugins manual](https://omarchy.org/manual/shell-plugins/)).
The plugin UI is QML. Audio is a child process.

```
plugin/
  manifest.json
  BarWidget.qml          // pill on the bar
  Panel.qml              // pad grid; owns the engine Process
  Engine.js              // NDJSON helpers (node-testable)
  bin/drywet-engine      // vendored Linux binary (not built by end users)
```

`omarchy-shell` is one long-lived Quickshell process. Use
`Quickshell.Io.Process` with **stdin open** and a **line splitter** on
stdout. `StdioCollector { waitForEnd: true }` is for one-shot `curl`,
not for a daemon that must stay up.

### `manifest.json`

Lives in the **plugin** repo. This crate does not ship it.

```json
{
  "schemaVersion": 1,
  "id": "you.pads",
  "name": "Pads",
  "version": "1.0.0",
  "author": "You",
  "license": "MIT",
  "description": "A four-pad instrument on the Omarchy bar.",
  "kinds": ["bar-widget"],
  "entryPoints": { "barWidget": "BarWidget.qml" },
  "barWidget": {
    "displayName": "Pads",
    "category": "Sound",
    "allowMultiple": false,
    "defaultSection": "right"
  }
}
```

Install under `~/.config/omarchy/plugins/you.pads/`, then:

```bash
omarchy plugin validate ~/.config/omarchy/plugins/you.pads
omarchy-shell shell rescanPlugins
omarchy plugin enable you.pads
```

### `Engine.js`

Keep JSON parse out of inline QML so you can unit-test it with `node`.

```javascript
.pragma library

function line(cmd) {
    return JSON.stringify(cmd) + "\n"
}

function warmup(instrument) {
    return line({ cmd: "warmup", instrument: instrument || "synth" })
}

function start(bpm) {
    return line({ cmd: "start", bpm: bpm || 120, loop: false })
}

function play(note, duration) {
    return line({ cmd: "play-midi", note: note, duration: duration || "8n" })
}

function noteOn(note) {
    return line({ cmd: "note-on", note: note })
}

function noteOff(note) {
    return line({ cmd: "note-off", note: note })
}

function stop() {
    return line({ cmd: "stop" })
}

function shutdown() {
    return line({ cmd: "shutdown" })
}

function parseReply(text) {
    try {
        return JSON.parse(text)
    } catch (err) {
        return { error: String(err) }
    }
}

function isStarted(reply) {
    return reply && reply.event === "started"
}

function isOk(reply) {
    return reply && reply.ok === true
}
```

### `Panel.qml` — one process, pads write NDJSON

```qml
import QtQuick
import Quickshell
import Quickshell.Io
import "Engine.js" as Engine

Item {
    id: root

    // Plugin-relative binary. End users do not run cargo.
    readonly property string enginePath: {
        const url = Qt.resolvedUrl("bin/drywet-engine")
        return url.toString().replace(/^file:\/\//, "")
    }

    property real latencyMs: 0
    property string position: "0:0:0"
    property bool started: false

    function send(payload) {
        if (proc.running)
            proc.write(payload)
    }

    function hit(note) {
        send(Engine.play(note, "8n"))
    }

    Process {
        id: proc
        command: [root.enginePath]
        running: true
        stdinEnabled: true

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: data => {
                const reply = Engine.parseReply(data)
                if (Engine.isStarted(reply)) {
                    root.started = true
                    root.latencyMs = reply.latencyMs || 0
                    root.position = reply.position || "0:0:0"
                }
            }
        }

        onStarted: {
            send(Engine.warmup("synth"))
            send(Engine.start(120))
        }

        // Quickshell kills the child when the Process dies; still be polite.
        Component.onDestruction: send(Engine.shutdown())
    }

    Row {
        spacing: 8
        Repeater {
            model: ["C4", "D4", "E4", "G4"]
            delegate: Rectangle {
                required property string modelData
                width: 64; height: 64; radius: 8
                color: padArea.pressed ? "#B91414" : "#3A3533"

                Text {
                    anchors.centerIn: parent
                    text: modelData
                    textFormat: Text.PlainText
                    color: "#FFFFFF"
                }

                MouseArea {
                    id: padArea
                    anchors.fill: parent
                    onClicked: root.hit(modelData)
                }
            }
        }
    }
}
```

`BarWidget.qml` is the manifest entry point: a pill that hosts this
panel. Follow the [Omarchy plugin develop
guide](https://plugins.omarchy.org/develop.html) for `open()` / `close()`
and bar theming (`bar.foreground`, `Style.space`, `textFormat:
Text.PlainText`).

A `menu` kind is the same Process contract — only the surface changes.
Keep the engine in a `service` kind if several widgets should share one
child; otherwise one panel owns one process.

### Held notes and sequences

Click → `play-midi` (attack-release, duration `"8n"` by default).

Pointer down / up → `note-on` / `note-off` (aliases: `trigger_attack` /
`trigger_release`).

A looping phrase is JSON on `start`, not a host song document:

```javascript
send(Engine.line({
    cmd: "start",
    bpm: 120,
    sequence: { events: ["C4", "G3", "A3", "F3"], subdivision: "4n" }
}))
```

`part` is a list of `[time, note]` or `{ time, event }` objects.
`loop: true` is Transport loop. `loop: { interval: "4n" }` is a drywet
`Loop` callback.

`stop` keeps the process and the PipeWire stream. `shutdown` disposes
and exits. After `stop`, `play-midi` / `note-on` still mix on the same
stream.

---

## Engine protocol

One JSON object per stdin line. One JSON object per stdout line, flushed.

| `cmd` | Role |
|-------|------|
| `warmup` | Select `synth` / `drum` / `sampler`; open the sink clock |
| `start` | Start Transport; optional `bpm`, `loop`, `sequence` / `part` / `loop` JSON |
| `stop` | Stop the arrangement clock; keep the process |
| `pause` / `resume` | Pause / continue the clock |
| `play-midi` | Live attack-release (`note`, `duration`) |
| `note-on` / `note-off` | Held notes (`trigger_attack` / `trigger_release` aliases) |
| `bpm` | `{ "cmd": "bpm", "value": 100 }` — applies on the next `start` if already running |
| `shutdown` | Close the sink and exit |

Replies:

```json
{"event":"started","latencyMs":5,"position":"0:0:0"}
{"ok":true}
{"error":"unknown cmd: nope"}
```

Default sink is PipeWire. `drywet-engine --buffer` is BufferSink for
tests (no sound server).

Sampler warmup:

```json
{"cmd":"warmup","instrument":"sampler","directory":"/path/to/wavs"}
```

or an explicit note → path map (`map` or `urls`). Drum warmup uses
trigger names `kick`, `snare`, `hat` as the `note` field.

---

## Playhead

`started.latencyMs` is the negotiated stream latency, not a hardcoded
80 ms. A QML or Ply playhead that follows wall time should subtract it:

```
audiblePositionMs = Date.now() - startedAtMs - latencyMs
```

Or poll `Transport.position()` in-process each frame. The engine does
not stream playhead ticks; hosts that need a moving BBS clock either
link the crate or interpolate from `started` + BPM.

---

## What not to do

* Load `drywet` as a QML plugin or `cdylib` inside `omarchy-shell`. The
  supported Omarchy path is a child `drywet-engine`.
* Spawn `pw-play` / `pw-cat` per pad hit. One Process, one sink.
* Drive the clock with `Timer` / `setTimeout`. Language timers are not
  the clock; scheduled callbacks receive sample-accurate `time`.
* Put `Context` on a second thread and call it from the UI without a
  queue. The musician API is single-threaded.
* Ask end users to install Rust. The widget repo vendors
  `x86_64-unknown-linux-gnu` (or musl) `drywet-engine`.
