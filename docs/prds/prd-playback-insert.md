# PRD: Playback Insert

**Status:** Draft
**Owner:** drywet

---

## Overview

Instruments today mix dry PCM into the Context sink at enqueue time. There is no processing stage after that mix, so a host that wants bitcrush, a low-pass, or any later effect has to run it inside `mix` / `write`. Two notes from another implementation capture the result:

- Crush inside the sink mix bakes the amount into queued frames. Later amount changes do not touch audio already in the buffer.
- A low-pass inside the same mix, after crush, starts each hit from a zero filter state. Queued audio keeps the cutoff it was mixed with.

This PRD adds a **playback insert**: an ordered, stateful chain that runs on the mixed stream at output time (device callback, PipeWire `process`, or a render-out copy). Mix stays dry. Parameter changes take effect on the next output block. Filter state lives on the insert, not on each hit.

The v1 drywet PRD still excludes a node-graph effects catalog. This work is the hook that catalog would need, not the catalog itself. Filter and bitcrush are the motivating consumers, not shipped products here.

Heritage maps in [drywet-py `.features/`](https://github.com/markschellhas/drywet-py/tree/master/.features) (`sinks`, `instruments`, `context`) already state that instruments mix onto the Context sink and do not connect to a node graph. This repo has no `.features/` maps yet; current behavior is taken from `src/sink.rs`, `src/sink_pipewire.rs`, `src/sink_device.rs`, and `src/instrument.rs`.

## Goals / Non-Goals

**Goals:**

1. One ordered insert chain per Context sink, default empty (identity), so existing mix tests keep seeing dry PCM.
2. Inserts run at playback / output, not inside `Sink::mix` / `Sink::write`. Audio already queued is re-processed with the current parameters on the next output block.
3. Stateful inserts keep continuous state across hits and across mix chunks (a low-pass does not reset to zero at each `trigger_*`).
4. Chain order is playback order (crush then low-pass is a chain of two inserts, not mix-time nesting).
5. Musician-facing mix/write/trigger APIs stay the same. Hosts attach inserts without changing Sequence callbacks or instrument constructors.
6. The audio-thread path stays allocation-free: insert `process` does not allocate, parse NDJSON, or take a blocking lock.
7. All three sinks honor the same contract: `BufferSink` (offline / tests), `PipeWireSink::process`, `DeviceSink` CPAL callback.

**Non-Goals:**

1. Shipping bitcrush, filter, delay, or any other effect as a public product API. A test double insert is enough to prove the hook.
2. A Tone.js / Web Audio node graph (`toDestination()`, `chain()`, Signals).
3. Per-voice or per-instrument inserts. The complaints are about the mixed stream and continuous filter state, which is a bus insert.
4. Changing `Sink::mix` / `Sink::write` signatures, slot capacity, or the one-shot mix-at-time instrument path.
5. Engine NDJSON verbs for effect parameters (no `cmd` for crush/cutoff in this PRD). In-process hosts attach inserts on `Context` / the sink.
6. Mixer UI, send/return, sidechain, or recording.
7. Re-opening the v1 drywet PRD non-goals (swing, velocity layers, Ableton Link, CLAP/VST).

## Current Implementation

There is no playback insert. Every voice is rendered to a PCM slice and summed into the sink; that summed buffer is what playback reads.

**Entry points**

- Instruments: `src/instrument.rs` `mix_at_time` converts `time` to a sample index (or `None` → write cursor) and calls `ctx.sink_mut().mix(frames, at_sample)`.
- Trait: `src/sink.rs` `Sink::mix` / `Sink::write`. Mix does not advance the cursor; write does. Tails sum. Stereo duplicates each mono frame.
- Offline: `src/sink.rs` `BufferSink::mix_at` resizes an expanding `Vec<f32>` and adds into it. `Context::render` (`src/context.rs`) fires the schedule, pads, and returns `sink.frames().to_vec()` — the dry mix.
- PipeWire-style callback: `src/sink_pipewire.rs` `MixStorage::enqueue` copies dry PCM into a preallocated 64×4096 slot table. `PipeWireSink::process` sums live slots into `output`, then `StreamBackend::process`. No post-sum processing.
- Native device: `src/sink_device.rs` `Playback::mix` adds dry frames into `mono`. The CPAL callback reads `sample_at(playback_cursor)` and writes that baked sample to the device.
- Engine: `src/engine.rs` / `src/bin/drywet-engine.rs` expose warmup / start / stop / play-midi / note-on / note-off / bpm / shutdown. No insert or effect command.

**Behavior this causes**

- Once a hit is mixed, its samples are the sound. Changing an effect parameter cannot rewrite queued slots, `BufferSink` frames, or `Playback.mono`.
- Applying a filter at mix time gives each `mix` chunk a fresh zero state, so hits do not share a filter memory.
- Hosts that need crush-then-filter must nest that order inside mix, which is the second complaint.

Tests that lock this in: `tests/buffer_sink.rs` (offset mix, tails, stereo duplicate), `tests/pipewire_sink.rs` (`process` equals the summed mix), `tests/render.rs` (sequence/live mix onto `frames()`). Heritage drywet-py `.features/sinks.yaml` and `.features/instruments.yaml` describe the same mix-onto-sink door.

## Proposed Implementation

Keep mix dry. Add a small insert chain that processes the mixed stream when it leaves the sink.

**User-facing**

In-process hosts attach inserts on the Context (or a short-lived sink guard), not inside Sequence callbacks:

```
ctx.set_inserts([crush, lowpass]); // order = playback order
synth.trigger_attack_release(&ctx, "C4", "8n", None)?; // still dry mix
```

Empty chain is identity. Detach / clear restores dry output. Parameter fields on an insert (amount, cutoff) are readable from the audio callback on the next block; they are not sampled at `mix` time.

**API / types**

- New `Insert` trait: `process(&mut self, frames: &mut [f32])` on a mono (or already-interleaved) output block. Object-safe so a `dyn Insert` chain can live on the sink.
- Chain is ordered, finite, and owned by the Context sink. One chain per Context, matching one sink per Context.
- `Sink::mix` / `Sink::write` do not call inserts. Instruments keep calling `mix_at_time` unchanged.

**Where the chain runs**

| Sink | Dry store | Insert runs |
| --- | --- | --- |
| `BufferSink` | `frames()` stays the dry mix (existing tests) | On the `Vec` `Context::render` returns, and on any playback/export copy (`to_pcm_s16le` if that path is the audible product) |
| `PipeWireSink` | `MixStorage` slots stay dry | After slot sum in `PipeWireSink::process`, before `StreamBackend::process` |
| `DeviceSink` | `Playback.mono` stays dry | In the CPAL callback, on the sample (or block) after `sample_at`, before `FromSample` |

**Data model**

No new persistence. Inserts are runtime objects. No schema change to Sequence / Part / Loop JSON. No engine command in this PRD.

**Cross-package**

Library only for this PRD. `drywet-engine` keeps current verbs. A later PRD can add NDJSON once a host needs QML-driven crush/cutoff.

## Technical Details

- New module `src/insert.rs` (or `src/sink/insert.rs`): `Insert` trait, `InsertChain` (fixed small cap or `Vec` built on the control thread, not grown in the callback).
- Wire the chain through `Context` so `render` and `sink_mut` share it. Avoid a second chain on the sink that can drift from Context.
- Audio thread: `process` must not allocate, format, or I/O. Parameter updates from the UI / NDJSON thread use atomics or a preallocated slot, same rule as note/transport queues in `docs/prds/prd-drywet.md`.
- Stereo: process after mix has already duplicated mono into channels, or process mono then duplicate — pick one and test both `channels = 1` and `channels = 2`. Prefer processing the stream the callback already has (interleaved output in PipeWire/Device) so filter state is per output channel if stereo.
- Bypass: empty chain or a no-op insert leaves `tests/buffer_sink.rs`, `tests/pipewire_sink.rs`, and `tests/render.rs` unchanged.
- Proof tests (BufferSink + PipeWire mock, no device):
  - Gain insert on render-out changes returned PCM; `sink.frames()` stays dry.
  - Mix a hit, then change insert gain, then `process` / render-out: the already-queued hit reflects the new gain.
  - Stateful running-sum or one-pole insert: two sequential hits share state (second block does not start at zero).
  - Order: insert A then B ≠ B then A on the same dry mix.
- Feature flags / i18n / analytics: none.
- Dependencies: none beyond current crate deps (`cpal` stays DeviceSink-only).
- Limits: document a small max chain length (constant next to `DEFAULT_MAX_VOICES` in `src/limits.rs`) so the callback never grows storage.
- `docs/reference/output.md` and the drywet skill should describe the insert as a playback stage after mix, once implemented. This PRD does not edit those files.

## Effort Estimates

- `Insert` trait + empty chain on Context: **S**
- Apply chain on `BufferSink` render-out / export copy; keep `frames()` dry: **S**
- Apply chain in `PipeWireSink::process` and `DeviceSink` callback: **M**
- Proof tests (gain, param-after-queue, shared state, order): **S**
- Engine / QML protocol for insert params: out of scope (would be **S** later)

## Open Questions

1. Does `Context::render` return wet PCM (audible product) while `sink.frames()` stays dry, or does render stay dry and hosts call a separate `processed_frames()`? Wet `render` matches Device/PipeWire output; dry `render` matches today’s tests that only check peaks. Peak tests still pass if the default chain is empty.
2. Process interleaved callback buffers vs process mono then duplicate. Stereo DeviceSink already duplicates in the callback; a mono insert before duplicate is simpler state, but a stereo insert matches the device stream.
3. Who owns the chain — `Context` or each `Sink`? Context ownership matches one-sink-per-context. Sink ownership makes `PipeWireSink::process` obvious. Prefer Context-owned, borrowed by the sink at output.
4. When a host needs QML-driven crush/cutoff, add engine cmds in a follow-up rather than expanding this PRD.
5. Should `to_pcm_s16le` stay a dry dump for tests, or become a wet export? Same split as `frames()` vs `render`.

## Related Docs

- Source brief: `docs/feature-insert.md`
- This file: `docs/prds/prd-playback-insert.md`
- Runtime PRD: `docs/prds/prd-drywet.md` (v1 non-goal: node-graph effects catalog)
- Output: `docs/reference/output.md`
- Engine protocol: `docs/reference/engine.md` (no insert cmds today)
- GUI hosts: `docs/reference/gui.md`
- Heritage maps: [drywet-py `.features/sinks.yaml`](https://github.com/markschellhas/drywet-py/blob/master/.features/sinks.yaml), [instruments.yaml](https://github.com/markschellhas/drywet-py/blob/master/.features/instruments.yaml), [context.yaml](https://github.com/markschellhas/drywet-py/blob/master/.features/context.yaml)
- Implementation plan (mix/sink already shipped): `docs/plans/2026-09-20-drywet.md`
- This repo has no `.features/` maps yet. When they exist, playback insert belongs next to sinks / context, not a new musician-facing door.
