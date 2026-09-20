# PRD: drywet

**Status:** Draft
**Owner:** drywet

---

## Overview

drywet is a Rust library and a small stdio binary that give desktop music widgets a musician-facing runtime: one Transport, musical time, Sampler, simple synths and drums, Sequence / Part / Loop, and live notes mixed onto one playing stream.

The first consumer is an Omarchy bar or menu plugin. Omarchy loads QML inside `omarchy-shell`. It does not load a Rust library into the bar. The plugin UI is QML. This repo is the process that QML starts once and talks to over NDJSON. The same library can be used in-process by a later standalone app.

Tone.js feels complete because it is two layers: a musician API (Transport, `"4n"` time, Sampler, Sequence) and a hard audio engine (the browser). drywet takes the first layer and implements the second in Rust on PipeWire.

A Python prototype of the same API lives in [drywet-py](https://github.com/markschellhas/drywet-py). Its feature maps (`context`, `transport`, `time`, `instruments`, `events`, `sinks`, `engine`) are the contract to port. That package is reference only. It is not a runtime this repo depends on.

There is no equivalent Rust crate. `cpal` is I/O. `fundsp` schedules audio in seconds. Neither is a shared playhead plus a folder-of-WAVs Sampler for a pad grid.

## Goals / Non-Goals

**Goals:**

1. One Transport per Context: `start` / `stop` / `pause` / `toggle`, `bpm`, `time_signature`, `loop` / `loop_start` / `loop_end`, state in `{started, stopped, paused}`, playhead as `position` (bars:beats:sixteenths), `seconds`, and `ticks`.
2. Musical time strings resolve against that Transport (`"4n"`, `"8n."`, `"8t"`, `"1m"`, `"+4n"`, `"4:0:0"`, raw seconds). Scheduled events lock to the sample clock.
3. Sampler maps note or MIDI number to WAV, pitch-shifts missing pitches, and plays polyphonically via `trigger_attack_release`.
4. Built-in voices cover a chord widget and a beat widget: Sampler, a small additive Synth, and Drum (kick, snare, hi-hat) with no SoundFont.
5. Sequence, Part, and Loop attach to the Transport. Callbacks receive sample-accurate `time` and pass it into instrument triggers. Live hits with no `time` mix at the current write cursor without stopping playback.
6. Default Linux output is a PipeWire callback (not one `pw-play` per hit). `BufferSink` exists so tests never open a sound server.
7. `drywet-engine` reads NDJSON on stdin and writes NDJSON on stdout so a QML host can drive Transport, Sampler, and live notes without linking Rust.
8. Omarchy install: the plugin repo vendors a prebuilt Linux binary. End users do not run `cargo` or `pip`.

**Non-Goals:**

1. A Python runtime, or keeping drywet-py as a shipped engine.
2. Wire-compatible Tone.js or Web Audio (`AudioContext`, nodes, `toDestination()`, Signals, `Tone.Draw`).
3. A node-graph effects catalog, swing, humanize, velocity layers, Ableton Link, or clip warping in v1.
4. A full DAW, mixer UI, recording, or tape emulation (herman-band / fourtrack is a later app on this crate, not this PRD).
5. Shipping QML, an Omarchy `manifest.json`, or any host UI from this repo.
6. VST3 / JUCE. CLAP export is later, not v1.
7. Go, LinnUI, or a second engine in another language.
8. Making PipeWire or hardware devices part of CI.

## Current Implementation

No current implementation in this repository — greenfield Rust crate.

Heritage to port, not to import:

- [drywet-py](https://github.com/markschellhas/drywet-py) package `drywet` (`Context`, `Transport`, `Sampler`, `Synth`, `Drum`, `Sequence`, `Part`, `Loop`, `BufferSink`, `PipeWireSink`, `python -m drywet.engine`).
- Feature maps in that repo: `.features/context.yaml`, `transport.yaml`, `time.yaml`, `instruments.yaml`, `events.yaml`, `sinks.yaml`, `engine.yaml`.
- Tests there (`test_transport_*`, `test_schedule`, `test_sequence`, `test_sampler`, `test_engine`) are the behavior spec until this repo has its own.

Known limits of that prototype to leave behind: live output is raw PCM piped to `pw-cat` with a fixed ~80 ms `latency_ms`, and rendering is Python on the writer thread. v1 here uses a PipeWire (or equivalent) callback and does not allocate on that callback.

## Proposed Implementation

One Cargo workspace, two packages.

**`drywet` (library).** In-process API. Names stay snake_case. Types stay Context, Transport, Sampler, Sequence, Part, Loop.

```
let ctx = Context::new(Default::default());
let synth = Synth::new(&ctx);
let seq = Sequence::new(|time, note| {
    synth.trigger_attack_release(note, "8n", Some(time));
}, ["C4", "G3", "A3", "F3"], "1m");
seq.start(0);
ctx.transport().set_bpm(120);
ctx.transport().start();
```

Language timers are not the clock. The callback `time` is. Live UI notes omit `time` and mix now.

**`drywet-engine` (binary).** Same NDJSON commands as drywet-py `engine` so an Omarchy widget can switch hosts without a new protocol:

| cmd | role |
|-----|------|
| `warmup` | load Synth, Drum, or Sampler; open the sink |
| `start` | start Transport (optional bpm, loop, sequence/part/loop JSON) |
| `stop` | stop arrangement clock; keep process and sink |
| `pause` / `resume` | pause / continue clock |
| `play-midi` | live attack-release |
| `note-on` / `note-off` | held notes |
| `bpm` | set tempo |
| `shutdown` | close sink and exit |

Events out: `started` (`latencyMs`, position), `ok`, `error`. Schedule payloads are drywet Sequence / Part / Loop JSON, not a host song document.

**Contracts this repo owns:**

- Library API (`drywet`): Context, Transport, time parse, instruments, events, Sink trait.
- Engine protocol (`drywet-engine`): NDJSON command and event lines (same verbs as drywet-py).
- Sink: `write` / `mix` / `stop` / `close`; `PipeWireSink` default on Linux; `BufferSink` for tests and `transport.render`.

**Contracts this repo does not own:** Omarchy `manifest.json`, QML entry points, sample banks. Those live in the widget repo, which vendors the engine binary.

### Context and Transport

- `Context` owns sample rate (default 44100), channel count (default 1, stereo allowed), the sink, and exactly one Transport.
- Transport follows Tone roles as in drywet-py `.features/transport.yaml`. `start()` returns when the sink has accepted the first audio (immediately for BufferSink) and exposes `latency_ms` so a UI playhead can offset from wall clock.
- PPQ default 192. BPM 40–240. `time_signature` is `(n, d)`.
- Invalid time strings and out-of-range pitch/BPM raise errors. Do not hang the sink.

### Instruments

- `Sampler`: note or MIDI → WAV path; `from_directory` of `C4.wav`-style files; pitch-shift nearest sample; optional loop; polyphony cap; `add`; `release_all`.
- `Synth`: small additive voice with attack/release.
- `Drum`: kick, snare, hi-hat transients. Beat grids are sixteenths from `numerator * 16 / denominator`.
- Shared: `trigger_attack`, `trigger_release`, `trigger_attack_release`. WAV only in v1. Resample on load if file rate differs.

### Scheduling

- `Sequence(callback, events, subdivision)` — nested lists subdivide.
- `Part(callback, [(time, event), ...])`.
- `Loop(callback, interval)`.
- `Transport.schedule` / `schedule_once` / `schedule_repeat` / `cancel` / `clear` / `dispose`.
- Callbacks must not block and must not write PCM. They only trigger instruments at `time`.

### Output

- Instruments do not connect to a node graph. The Context sink is the destination.
- `PipeWireSink`: one persistent stream. No process-per-note.
- `BufferSink`: in-memory PCM for CI and offline `render`.
- Arrangement is rendered onto integer sample frames. Drum tails mix forward. Loop wraps so each cycle stays the nominal length.

## Technical Details

- Workspace: `Cargo.toml` at repo root. Packages `drywet` (lib) and `drywet-engine` (`src/bin` or a small crate).
- Likely modules in `drywet`: `context`, `transport`, `time`, `instrument`, `event`, `sink`.
- Clock: integer ticks at PPQ derived from BPM and sample rate. Convert all schedule times to sample frames.
- Audio thread: no heap allocation, no locks that can block, no engine protocol parsing. UI and NDJSON run on another thread. Pass note/transport changes through a lock-free queue.
- I/O: PipeWire first on Linux (Omarchy). Keep the Sink trait so a later `cpal` backend can cover other machines without changing the musician API.
- Tests use `BufferSink` only. Port cases from drywet-py: transport state/time/loop, schedule, sequence/part, sampler, synth, drum, engine protocol with `--buffer`.
- Limits (documented constants): MIDI 0–127, Hz 20–20000, BPM 40–240, voice cap, max schedule length 600 seconds.
- Release for widgets: statically linked `x86_64-unknown-linux-gnu` (or musl) binary committed by the widget repo, not necessarily by this repo. This repo publishes source and CI tests.
- License: MIT, matching drywet-py and typical Omarchy plugins.

## Effort Estimates

- Time parser + Transport + BufferSink + render tests: **M**
- Sampler + Synth + Drum + live mix: **M**
- Sequence / Part / Loop + schedule APIs: **S**
- PipeWire callback sink: **M**
- `drywet-engine` NDJSON host: **S**
- Port drywet-py tests onto BufferSink: **S**

## Open Questions

1. PipeWire via a Rust crate vs a thin C bindgen around `pw-stream`. Prefer whichever keeps the callback allocation-free and is maintainable on Omarchy.
2. Default sample rate 44100 (drywet-py) vs 48000 (common PipeWire). Pick one Context default and resample files on load.
3. Whether `bpm` changes while started take effect immediately or on the next `start` (drywet-py deferred to next start).
4. Whether this repo also builds and attaches the Linux binary on tagged releases, or only the widget repo vendors a local build.
5. crates.io name `drywet` — confirm it is free before the first publish. Not required for v1 widgets.

## Related Docs

- This file: `docs/prds/prd-drywet.md`
- Heritage maps: [drywet-py `.features/`](https://github.com/markschellhas/drywet-py/tree/master/.features) (`context`, `transport`, `time`, `instruments`, `events`, `sinks`, `engine`)
- Heritage PRD: [drywet-py `docs/prds/prd-drywet.md`](https://github.com/markschellhas/drywet-py/blob/master/docs/prds/prd-drywet.md)
- Omarchy plugin shape: QML `bar-widget` / `menu` + child process (see Omarchy shell plugins manual)
- Feature maps for *this* repo are not written yet. When they exist they belong in `.features/` with the same door names as drywet-py.
