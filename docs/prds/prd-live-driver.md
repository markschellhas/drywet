# PRD: Live arrangement driver

**Status:** Draft
**Owner:** drywet
**Depends on:** [prd-drywet.md](prd-drywet.md)

## Overview

drywet already has Transport, Sequence / Part / Loop, instruments, and DeviceSink. Scheduled callbacks stay silent until `fire_until` or `Context::render`. That is correct: the CPAL / PipeWire callback must not run the musician API.

What is missing is the **driver** that calls `fire_until` while an arrangement is live.

- In-process examples pre-render a finite phrase (`render` then `play` then drain). That is a clip player.
- In-process GUIs (sequenzer / ply) call `fire_until(elapsed + lookahead)` on their own frame loop. That works because they share the `Context` thread.
- `drywet-engine` registers the sequence, calls `transport.start()`, opens the device, emits `started`, then blocks on stdin. Nothing ticks. Out-of-process hosts (Omarchy QML, any stdio GUI) hear silence. `play-midi` still works because it mixes at the write cursor.

The v1 PRD already said language timers are not the clock, and that `drywet-engine` is how a QML host drives Transport without linking Rust. This PRD fills the gap those sentences assumed.

The GUI still owns controls and visualization. drywet still owns musical time, mixing, and the device. The driver is part of drywet: named `tick` on the library, run by `drywet-engine` for stdio hosts. `transport.start()` stays a state change. It does not spawn a thread.

## Goals / Non-Goals

**Goals:**

1. Name the live driver. While Transport is `started`, due Sequence / Part / Loop callbacks mix onto the Context sink when something calls `tick` (lookahead `fire_until`). Offline `render` is unchanged.
2. `drywet-engine` is that something for out-of-process hosts. After `start` with a schedule, the process keeps ticking until `stop` / `pause` / `shutdown`, interleaved with stdin, without a UI timer and without `play-midi` as the arrangement clock.
3. `{"cmd":"start","loop":true,"sequence":{...}}` loops the phrase. Boolean `loop` must set loop points from the sequence (or part) length. Today `set_loop(true)` with `loop_end == loop_start` does not wrap occurrences, so one-shots fire once.
4. Live mix origin is the device write cursor, not sample 0. If the sink has already been playing silence (warmup `start_clock`), events scheduled at `t = 0` must not land behind the playhead and get skipped.
5. `--buffer` `start` + sequence actually mixes. Engine tests today assert `started` only; `play-midi` is the only command whose tests check peak. After this work, a BufferSink session that starts a sequence has non-silent frames through the phrase (and through at least one loop if `loop: true`).
6. Docs and feature maps match the code. `docs/reference/getting-started.md` currently says scheduled events run “as the process remains alive” after `transport.start()`. That line is false.

**Non-Goals:**

1. Calling `fire_until` from the DeviceSink / CPAL callback. The audio thread stays a reader of mixed PCM (and insert fold). No `RefCell`, no Sequence, no WAV decode on that thread.
2. Spawning a scheduler from library `transport.start()`. `Context` is `Rc` / `RefCell` and single-threaded. A second clock there races any in-process host that already ticks (sequenzer).
3. QML, ply, Omarchy manifests, or sample banks. Those stay in host repos.
4. Replacing `play-midi` / `note-on`. Live pads still mix at the write cursor with `time = None`.
5. Hot-swap of a running sequence without stop, swing, Link, or a second Transport.
6. Making PipeWire part of CI.

## Current Implementation

**Library.** `Sequence::start` / `Part::start` / `Loop::start` only register schedule ids. `.features/events.yaml` is explicit: nothing sounds until `transport.start()`, `fire_until`, or `ctx.render`. `transport.start()` sets state to Started, resets seconds from Stopped, and marks the sink accepted. It does not fire callbacks. `Context::render` is the offline driver: start if needed, `fire_until(duration)`, pad, return wet frames. `DeviceSink`’s output callback only `fold_mono`s the mix buffer.

**Examples.** `examples/support/mod.rs` `play()` is `render(duration)` then `play()` then `wait_until_end()`. `examples/drums.rs` sets `loop` and `loop_points(0, "1m")` then uses that clip path. There is no long-lived in-tree DeviceSink loop that ticks.

**Engine.** `handle_start` (`src/engine.rs`): set bpm, `apply_transport_loop`, `attach_schedule`, `ensure_clock`, `transport.start()`, emit `started`. No `fire_until`. `run` / `run_with_config` iterate `stdin.lines()`. While the host sends nothing, the arrangement does not advance. `play-midi` calls `ensure_clock` and `trigger_attack_release(..., None)` — that path is audible. `tests/engine.rs` `engine_sequence_payload_and_error` does not check BufferSink peak.

**Boolean loop.** `apply_transport_loop` on `loop: true` calls `set_loop(true)` without `set_loop_points`. Occurrence expansion requires `loop_end > loop_start`.

**In-process host that already works.** [sequenzer](https://github.com/markschellhas/sequenzer) does not use `drywet-engine`. ply calls `audio.tick()` every frame: `fire_until(elapsed + 0.04s)`, mix times offset by the sink `write_cursor` at Play so a late start is not in the past. That is the live-app counterpart of `render` + `play`. This PRD does not change that host contract.

**Out-of-process host that does not.** Omarchy QML (omabeatbox) can only write NDJSON. With current `start` + `sequence` it receives `started`, the device stream is active, PCM is silence. The widget is forced to fire `play-midi` from a QTimer, which the v1 PRD forbade as the arrangement clock.

## Proposed Implementation

Three layers stay as they are. Only the driver is added.

| Layer | Owns | Must not |
|--------|------|----------|
| DeviceSink callback | Consume mixed PCM | `fire_until`, instruments, JSON |
| Runtime (`Context` thread) | Transport, Sequence, mix at sample frames | Toolkit widgets |
| Host | Grid, knobs, playhead paint | Sample clock |

### Library (`drywet`)

Add a documented driver on `TransportRef` / `Context`, name **`tick`**:

- `tick(lookahead)` while Started: `fire_until(transport.seconds() + lookahead)` (or equivalent from the live origin below).
- Default lookahead on the order of **40 ms** (sequenzer’s `LOOKAHEAD_S`). Constant, documented, overridable.
- Idempotent: `fire_until` already records fired occurrences. A host that also ticks must not double-mix.
- `transport.start()` / `stop()` / `pause()` / `resume()` stay state + sink play/pause. No thread, no hidden pump.
- Offline `render` stays the finite driver. CLI examples keep `render` + `play` + drain.

Live origin for DeviceSink:

- When arming a live arrangement, mix timestamps are **origin + event time**, where origin is `write_cursor / sample_rate` at the moment of `start` (sequenzer `origin_s`). If `when` is already behind the playhead, mix at the live cursor (`time = None`) so a lagged tick still clicks.
- Do not mix a live `start` at absolute sample 0 after `warmup` has already called `start_clock`.

Optional but useful: engine **warmup does not start the device clock**. `start` (or the first `play-midi`) starts it after the first mix, or after origin is captured. Today `handle_warmup` calls `ensure_clock`, so the playhead is already moving before any sequence exists.

### Engine (`drywet-engine`)

The binary **is** the Context thread for stdio hosts.

- After a successful `start` that attached a schedule (or whenever Transport is Started), pump `tick(lookahead)` until `stop`, `pause`, or `shutdown`.
- Interleave with stdin: poll / timeout on the input pipe (tens of ms), handle one JSON line when present, otherwise tick. Do not block forever on `stdin.lines()` while Started. `resume` resumes the pump. `pause` stops ticking but keeps the process and sink.
- `start` + `loop: true` + `sequence` (or `part` with a finite span): `set_loop_points(0, phrase_length)` then tick. Phrase length for a sequence is `events.len() * subdivision` at the current BPM. A `loop` **object** remains `Loop::schedule_repeat` as today.
- `start` without a schedule still emits `started` and starts the clock so `play-midi` works (existing engine tests).
- `--buffer`: on `start` with a schedule, `fire_until` at least one phrase (and one extra loop cycle if looping) before answering `started`, so BufferSink tests can assert peak without a real device.

No new NDJSON verbs required for the happy path. Hosts keep sending `start` / `stop` / `play-midi`. Do not add a `tick` command that QML would have to call on a timer.

### Docs and maps

- `docs/reference/getting-started.md`: live transport requires `tick` (in-process) or `drywet-engine` (stdio). Delete “scheduled events then run as the process remains alive.”
- `docs/reference/gui.md`: in-process GUI frame loop must `tick`; stdio GUI must not.
- `docs/reference/engine.md`: `start` + sequence is audible; boolean `loop` repeats the phrase; `play-midi` is live hits only.
- `docs/reference/scheduling.md`: device output is tick + play, or render + play + drain, not start alone.
- `.features/transport.yaml`: `user_flow.schedule` names `tick` as the live driver; `start()` is not it.
- `.features/engine.yaml`: engine pumps `tick` while Started; stdin is not the clock.
- `.features/events.yaml`: `fire` line lists `tick`.

## Technical Details

- Keep `Context` on one thread. Engine pump and JSON parsing share that thread. DeviceSink already uses `Mutex<Playback>` for the callback; tick only `mix`es from the Context thread.
- Lookahead too small underruns the arrangement; too large delays grid edits that reschedule via stop+start. 40 ms is the known working value. Cap `fire_until` by `MAX_SCHEDULE_SECONDS`.
- Loop wrap stays in `occurrences()` (`src/transport.rs`). Engine must set points; do not invent a second looper in JSON (no 128-bar unrolled sequences as product).
- `bpm` while started: leave existing “applies on next start from stopped” unless a follow-up changes it. Engine hosts that need a new tempo already stop+start the bar.
- Tests (`cargo test`, BufferSink only):
  - `start` + sequence + `--buffer` (or `run` with BufferSink) → peak > 0 in the first phrase.
  - `loop: true` + one-bar sequence → peak > 0 after the first bar boundary (second cycle mixed).
  - `play-midi` after `start` without sequence still peaks (regression).
  - `fire_until` twice over the same window does not double amplitude beyond idempotent mix (fired set).
  - Library `tick` while Stopped is a no-op.
- Effort: engine pump **S–M**; live origin + loop points on start **S**; docs/maps **S**; tests **S**.

## Open Questions

1. Pump implementation in the engine: `poll` on stdin with a timeout vs a dedicated tick thread posting work back to the Context thread. Prefer one thread (timeout/poll) so `RefCell` stays valid.
2. Should warmup stop calling `ensure_clock`, or is capturing origin at `start` enough?
3. Default lookahead 40 ms vs making it an engine flag / JSON field. Prefer a documented constant first.
4. For `--buffer` with `loop: true`, how far to `fire_until` in the `start` reply (one extra bar vs a fixed 2 s). Prefer one extra cycle so tests stay fast.

## Related Docs

- Land as: `docs/prds/prd-live-driver.md`
- Parent: [prd-drywet.md](prd-drywet.md) (goals 2, 5, 7)
- [getting-started.md](../reference/getting-started.md), [gui.md](../reference/gui.md), [engine.md](../reference/engine.md), [scheduling.md](../reference/scheduling.md)
- `.features/transport.yaml`, `engine.yaml`, `events.yaml`, `sinks.yaml`
- In-process reference host: sequenzer `src/audio.rs` (`tick`, `LOOKAHEAD_S`, `origin_s`) and `src/main.rs` (ply calls `audio.tick()` every frame)
