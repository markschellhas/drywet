# Playback Insert Implementation Plan

> **For agent:** REQUIRED SUB-SKILL: Use subagent-driven-development to implement this plan task-by-task.
> Update each task's **Status** as work advances (not only at the end). Progress bar counts only `done` tasks.
> On resume: read **Progress** + each task's **Status** / **Resume** — do not re-do completed phases.

**Goal:** Add a playback insert chain that processes mixed PCM at output time so later effects (filter, bitcrush) can change already-queued audio and keep continuous state.

**Architecture:** Instruments keep mixing dry PCM onto the Context sink (`.features/instruments.yaml`, `.features/sinks.yaml`: no node graph). The insert chain lives on that one sink (`BufferSink` / `PipeWireSink` mix table / `DeviceSink` playback state). `Context::set_inserts` forwards to the sink. `Sink::mix` / `Sink::write` never run inserts. Output does: `Context::render` returns a wet copy, `PipeWireSink::process` runs the chain after slot sum, the DeviceSink callback runs it after `sample_at`. Empty chain is identity.

**Areas affected:** `src/`, `tests/`, `.features/`, `docs/reference/`, `.cursor/skills/drywet/references/`, `.claude/skills/drywet/references/`, `.agents/skills/drywet/references/`

**Tech Stack:** Rust edition 2021, `cargo test -q`. No new crates. No engine NDJSON. Tests use `BufferSink` and `PipeWireSink<MockStream>` only — never open a sound server.

**Feature map:** `.features/sinks.yaml` (primary door). Also `.features/context.yaml` (`render` becomes wet). Do not add a musician-facing `insert.yaml`.

**PRD:** `docs/prds/prd-playback-insert.md`

**Locked decisions (from the PRD):**

- Mix stays dry. `BufferSink::frames()` and `to_pcm_s16le()` stay dry.
- `Context::render` returns a **wet** copy (`apply_inserts` on the copy). Length still matches `frames()`. Peak-only render tests keep passing with an empty chain.
- `Insert::process` is a **mono** sample stream. `channels > 1` processes one sample per frame then duplicates (same as mix). Chunking must be equivalent to one call (`process(a∥b) == process(a); process(b)` for the test doubles).
- Chain is stored on the sink, not a second copy on `Context`. One Context → one sink → one chain.
- `MAX_INSERTS = 8`. Replacing the chain with more than 8 is `Err`; the previous chain stays.
- No crush/filter product types. Test doubles live in `tests/insert.rs`.
- No `drywet-engine` insert commands.
- `Insert: Send + 'static`. `process` must not allocate, format, I/O, or take a blocking lock besides the sink mutex already held.

## Progress

**Status:** `█████████████████░░░` 6/7 done (86%) · Task 7 in flight

| # | Task | Status | Next |
|---|------|--------|------|
| 1 | Insert trait and chain | `done` | — |
| 2 | BufferSink stores chain; mix stays dry | `done` | — |
| 3 | Context::set_inserts; render returns wet | `done` | — |
| 4 | PipeWireSink::process applies chain | `done` | — |
| 5 | DeviceSink callback applies chain | `done` | — |
| 6 | Patch maps and output docs | `done` | — |
| 7 | Full-suite regression | `implementing` | implement |

Shared test command: `cargo test -q`

---

### [x] Task 1: Insert trait and chain

**Status:** `done`
**Resume:** —
**Commits:** `a11f770` feat(insert): add playback insert trait and chain; `f28d8a8` test(insert): lock stereo, chunk, and max-chain limits

**Files:**
- Create: `src/insert.rs`
- Modify: `src/limits.rs`, `src/lib.rs`
- Test: `tests/insert.rs`

**Step 1: Write the failing test**

Create `tests/insert.rs`:

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use drywet::insert::{apply_interleaved, Insert, InsertChain, InsertError};
use drywet::limits::MAX_INSERTS;

struct Gain {
    gain: f32,
}

impl Insert for Gain {
    fn process(&mut self, frames: &mut [f32]) {
        for sample in frames.iter_mut() {
            *sample *= self.gain;
        }
    }
}

struct Add {
    delta: f32,
}

impl Insert for Add {
    fn process(&mut self, frames: &mut [f32]) {
        for sample in frames.iter_mut() {
            *sample += self.delta;
        }
    }
}

struct Integrator {
    acc: f32,
}

impl Insert for Integrator {
    fn process(&mut self, frames: &mut [f32]) {
        for sample in frames.iter_mut() {
            self.acc += *sample;
            *sample = self.acc;
        }
    }
}

fn boxed<I: Insert + 'static>(insert: I) -> Box<dyn Insert> {
    Box::new(insert)
}

#[test]
fn empty_chain_is_identity() {
    let mut chain = InsertChain::new();
    let mut frames = [0.25, -0.5];
    chain.process(&mut frames);
    assert_eq!(frames, [0.25, -0.5]);
}

#[test]
fn gain_scales_the_block() {
    let mut chain = InsertChain::new();
    chain.set(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    let mut frames = [0.25, -0.5];
    chain.process(&mut frames);
    assert_eq!(frames, [0.5, -1.0]);
}

#[test]
fn chain_order_is_playback_order() {
    let mut add_then_gain = InsertChain::new();
    add_then_gain
        .set(vec![boxed(Add { delta: 1.0 }), boxed(Gain { gain: 2.0 })])
        .unwrap();
    let mut a = [1.0];
    add_then_gain.process(&mut a);

    let mut gain_then_add = InsertChain::new();
    gain_then_add
        .set(vec![boxed(Gain { gain: 2.0 }), boxed(Add { delta: 1.0 })])
        .unwrap();
    let mut b = [1.0];
    gain_then_add.process(&mut b);

    assert_eq!(a, [4.0]);
    assert_eq!(b, [3.0]);
}

#[test]
fn integrator_keeps_state_across_calls() {
    let mut chain = InsertChain::new();
    chain.set(vec![boxed(Integrator { acc: 0.0 })]).unwrap();
    let mut first = [1.0];
    chain.process(&mut first);
    let mut second = [1.0];
    chain.process(&mut second);
    assert_eq!(first, [1.0]);
    assert_eq!(second, [2.0]);
}

#[test]
fn process_chunks_match_one_call() {
    let mut whole = InsertChain::new();
    whole.set(vec![boxed(Integrator { acc: 0.0 })]).unwrap();
    let mut all = [0.5, 0.5];
    whole.process(&mut all);

    let mut parts = InsertChain::new();
    parts.set(vec![boxed(Integrator { acc: 0.0 })]).unwrap();
    let mut a = [0.5];
    let mut b = [0.5];
    parts.process(&mut a);
    parts.process(&mut b);

    assert_eq!(all, [0.5, 1.0]);
    assert_eq!([a[0], b[0]], all);
}

#[test]
fn stereo_apply_processes_mono_then_duplicates() {
    let mut chain = InsertChain::new();
    chain.set(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    let mut interleaved = [0.25, 0.25, 0.5, 0.5];
    apply_interleaved(&mut chain, &mut interleaved, 2);
    assert_eq!(interleaved, [0.5, 0.5, 1.0, 1.0]);
}

#[test]
fn chain_full_keeps_previous_inserts() {
    let mut chain = InsertChain::new();
    chain.set(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    let too_many: Vec<Box<dyn Insert>> = (0..=MAX_INSERTS)
        .map(|_| boxed(Gain { gain: 3.0 }))
        .collect();
    match chain.set(too_many) {
        Err(InsertError::ChainFull { max, got }) => {
            assert_eq!(max, MAX_INSERTS);
            assert_eq!(got, MAX_INSERTS + 1);
        }
        other => panic!("expected ChainFull, got {other:?}"),
    }
    let mut frames = [1.0];
    chain.process(&mut frames);
    assert_eq!(frames, [2.0]);
}

#[test]
fn live_gain_handle_changes_next_block() {
    struct LiveGain {
        bits: Arc<AtomicU32>,
    }
    impl Insert for LiveGain {
        fn process(&mut self, frames: &mut [f32]) {
            let gain = f32::from_bits(self.bits.load(Ordering::Relaxed));
            for sample in frames {
                *sample *= gain;
            }
        }
    }

    let bits = Arc::new(AtomicU32::new(1.0f32.to_bits()));
    let mut chain = InsertChain::new();
    chain
        .set(vec![boxed(LiveGain {
            bits: Arc::clone(&bits),
        })])
        .unwrap();
    let mut first = [1.0];
    chain.process(&mut first);
    bits.store(0.5f32.to_bits(), Ordering::Relaxed);
    let mut second = [1.0];
    chain.process(&mut second);
    assert_eq!(first, [1.0]);
    assert_eq!(second, [0.5]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q insert`

Expected: FAIL — unresolved `drywet::insert` / `MAX_INSERTS`.

**Step 3: Write minimal implementation**

Add to `src/limits.rs`:

```rust
/// Max playback inserts on one sink. Replacing the chain never grows in the callback.
pub const MAX_INSERTS: usize = 8;
```

Create `src/insert.rs`:

```rust
use crate::limits::MAX_INSERTS;

/// Playback-time processor. Runs on mixed PCM at output, not inside `mix` / `write`.
///
/// `process` must not allocate, format, or do I/O. Treat `frames` as a mono
/// sample sequence: `process(a)` then `process(b)` must match `process(a∥b)`
/// for causal per-sample inserts.
pub trait Insert: Send + 'static {
    fn process(&mut self, frames: &mut [f32]);
}

/// Ordered insert list. First entry runs first (crush then low-pass).
pub struct InsertChain {
    inserts: Vec<Box<dyn Insert>>,
}

impl Default for InsertChain {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for InsertChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InsertChain")
            .field("len", &self.inserts.len())
            .finish()
    }
}

impl InsertChain {
    pub fn new() -> Self {
        Self {
            inserts: Vec::new(),
        }
    }

    pub fn set(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        if inserts.len() > MAX_INSERTS {
            return Err(InsertError::ChainFull {
                max: MAX_INSERTS,
                got: inserts.len(),
            });
        }
        self.inserts = inserts;
        Ok(())
    }

    pub fn process(&mut self, frames: &mut [f32]) {
        for insert in &mut self.inserts {
            insert.process(frames);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertError {
    ChainFull { max: usize, got: usize },
}

impl std::fmt::Display for InsertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InsertError::ChainFull { max, got } => {
                write!(f, "insert chain full: max {max}, got {got}")
            }
        }
    }
}

impl std::error::Error for InsertError {}

/// Process one sample per frame, then duplicate across `channels` (mix layout).
pub fn apply_interleaved(chain: &mut InsertChain, interleaved: &mut [f32], channels: u16) {
    let ch = usize::from(channels.max(1));
    if ch == 1 {
        chain.process(interleaved);
        return;
    }
    let frames = interleaved.len() / ch;
    let mut offset = 0;
    let mut mono = [0.0f32; 64];
    while offset < frames {
        let n = (frames - offset).min(mono.len());
        for i in 0..n {
            mono[i] = interleaved[(offset + i) * ch];
        }
        chain.process(&mut mono[..n]);
        for i in 0..n {
            let sample = mono[i];
            let base = (offset + i) * ch;
            for c in 0..ch {
                interleaved[base + c] = sample;
            }
        }
        offset += n;
    }
}
```

In `src/lib.rs` add `pub mod insert;` and re-export:

```rust
pub use insert::{Insert, InsertChain, InsertError};
```

**Step 4: Run test to verify it passes**

Run: `cargo test -q insert`

Expected: PASS (all `insert` tests).

**Step 5: Commit**

```bash
git add src/insert.rs src/limits.rs src/lib.rs tests/insert.rs
git commit -m "feat(insert): add playback insert trait and chain"
```

---

### [x] Task 2: BufferSink stores chain; mix stays dry

**Status:** `done`
**Resume:** —
**Commits:** `38d7cf7` feat(sink): keep BufferSink mix dry behind inserts; `3315da9` test(insert): drop unused Sink import

**Files:**
- Modify: `src/sink.rs`
- Test: `tests/insert.rs` (append), `tests/buffer_sink.rs` (unchanged; must still pass)

**Step 1: Write the failing test**

Append to `tests/insert.rs`:

```rust
use drywet::sink::Sink;
use drywet::BufferSink;

#[test]
fn buffer_mix_stays_dry_when_inserts_attached() {
    let mut sink = BufferSink::new(8, 1);
    sink.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    sink.mix(&[0.25, 0.5], Some(0));
    assert_eq!(sink.frames(), &[0.25, 0.5]);
    let pcm = sink.to_pcm_s16le();
    let mut expected = Vec::new();
    expected.extend_from_slice(&((0.25f32.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    expected.extend_from_slice(&((0.5f32.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    assert_eq!(pcm, expected);
}

#[test]
fn buffer_apply_inserts_wets_a_copy_not_frames() {
    let mut sink = BufferSink::new(8, 1);
    sink.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    sink.mix(&[0.25], Some(0));
    let mut wet = sink.frames().to_vec();
    sink.apply_inserts(&mut wet);
    assert_eq!(wet, vec![0.5]);
    assert_eq!(sink.frames(), &[0.25]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q insert`

Expected: FAIL — `set_inserts` / `apply_inserts` not found on `BufferSink`.

**Step 3: Write minimal implementation**

In `src/sink.rs`:

- `use crate::insert::{apply_interleaved, Insert, InsertChain, InsertError};`
- Add `inserts: InsertChain` to `BufferSink`.
- In `BufferSink::new`, set `inserts: InsertChain::new()`.
- Add inherent methods and trait methods:

```rust
impl BufferSink {
    pub fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        self.inserts.set(inserts)
    }

    pub fn apply_inserts(&mut self, frames: &mut [f32]) {
        apply_interleaved(&mut self.inserts, frames, self.channels);
    }
}
```

On `trait Sink` (keep object-safe). Default no-ops so `PipeWireSink` / `DeviceSink` still compile until Tasks 4–5:

```rust
    /// Replace the playback insert chain. Empty is identity. Default ignores the chain.
    fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        let _ = inserts;
        Ok(())
    }

    /// Run the chain on `frames` (caller-owned copy). Must not rewrite the dry mix store.
    fn apply_inserts(&mut self, frames: &mut [f32]) {
        let _ = frames;
    }
```

`impl Sink for BufferSink`: override and delegate to the inherent methods.

**Step 4: Run tests to verify they pass**

Run: `cargo test -q insert && cargo test -q buffer_sink`

Expected: PASS. Existing mix/tail/stereo tests unchanged.

**Step 5: Commit**

```bash
git add src/sink.rs tests/insert.rs
git commit -m "feat(sink): keep BufferSink mix dry behind inserts"
```

---

### [x] Task 3: Context::set_inserts; render returns wet

**Status:** `done`
**Resume:** —
**Commits:** `8c73b49` feat(context): apply insert chain on render copy

**Files:**
- Modify: `src/context.rs`
- Test: `tests/insert.rs`

`Sink` defaults from Task 2 are no-ops on PipeWire/Device. BufferSink already overrides.

**Step 1: Write the failing test**

Append to `tests/insert.rs`:

```rust
use drywet::Context;

#[test]
fn render_returns_wet_and_frames_stay_dry() {
    let ctx = Context::new();
    ctx.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    ctx.sink_mut().mix(&[0.25], Some(0));
    let wet = ctx.render(1.0 / f64::from(ctx.sample_rate())).unwrap();
    assert_eq!(&wet[..1], &[0.5]);
    assert_eq!(&ctx.sink().frames()[..1], &[0.25]);
}

#[test]
fn render_without_inserts_matches_frames() {
    let ctx = Context::new();
    ctx.sink_mut().mix(&[0.25], Some(0));
    let pcm = ctx.render(1.0 / f64::from(ctx.sample_rate())).unwrap();
    assert_eq!(pcm.len(), ctx.sink().frames().len());
    assert_eq!(&pcm[..1], &ctx.sink().frames()[..1]);
}

#[test]
fn set_inserts_after_mix_changes_next_render() {
    let ctx = Context::new();
    ctx.sink_mut().mix(&[1.0], Some(0));
    ctx.set_inserts(vec![boxed(Gain { gain: 0.5 })]).unwrap();
    let wet = ctx.render(1.0 / f64::from(ctx.sample_rate())).unwrap();
    assert_eq!(&wet[..1], &[0.5]);
    assert_eq!(&ctx.sink().frames()[..1], &[1.0]);
}
```

`render(1.0 / sample_rate)` is one frame. `render` also **pads** with zeros via `write` if the cursor is behind. Mixing at sample 0 does not advance the cursor, so pad will `write` zeros from cursor 0 and **sum** into the existing 0.25. Check `BufferSink::write` / `mix_at`: write at cursor 0 adds zeros — the 0.25 stays.

Safer: mix then `write` enough to set the cursor, or call `render` of 1 sample after `write(&[0.25])` which both mixes and advances.

Replace the mix-only setup with:

```rust
ctx.sink_mut().write(&[0.25]);
let wet = ctx.render(1.0 / f64::from(ctx.sample_rate())).unwrap();
```

`write` advances cursor to 1. `render` needs `ceil(duration * sr)` frames. `needed = 1`, cursor is 1, no pad. Dry frames `[0.25]`, wet `[0.5]`.

Use `write`, not `mix`, in these three tests.

**Step 2: Run test to verify it fails**

Run: `cargo test -q insert`

Expected: FAIL — `Context::set_inserts` missing, or `render` equals dry frames.

**Step 3: Write minimal implementation**

In `src/context.rs`:

```rust
use crate::insert::{Insert, InsertError};

impl<S: Sink> Context<S> {
    pub fn set_inserts(
        &self,
        inserts: impl IntoIterator<Item = Box<dyn Insert>>,
    ) -> Result<(), InsertError> {
        self.sink
            .borrow_mut()
            .set_inserts(inserts.into_iter().collect())
    }

    pub fn clear_inserts(&self) -> Result<(), InsertError> {
        self.set_inserts(Vec::new())
    }
}
```

Change the end of `render`:

```rust
        let mut sink = self.sink.borrow_mut();
        let cursor = sink.write_cursor();
        if cursor < needed {
            sink.write(&vec![0.0; needed - cursor]);
        }
        let mut pcm = sink.frames().to_vec();
        sink.apply_inserts(&mut pcm);
        Ok(pcm)
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -q insert && cargo test -q render && cargo test -q context && cargo test -q buffer_sink`

Expected: PASS. `tests/render.rs` only checks peaks and lengths — empty chain keeps `pcm == frames`.

**Step 5: Commit**

```bash
git add src/context.rs tests/insert.rs
git commit -m "feat(context): apply insert chain on render copy"
```

---

### [x] Task 4: PipeWireSink::process applies chain

**Status:** `done`
**Resume:** —
**Commits:** `07b0d28` feat(sink): run insert chain in PipeWire process

**Files:**
- Modify: `src/sink_pipewire.rs`
- Test: `tests/insert.rs`, `tests/pipewire_sink.rs` (existing process==sum must still pass with empty chain)

**Step 1: Write the failing test**

Append to `tests/insert.rs`:

```rust
use drywet::PipeWireSink;

#[test]
fn pipewire_process_is_wet_slots_stay_queued_until_callback() {
    let mut sink = PipeWireSink::new(44100, 1);
    sink.set_inserts(vec![boxed(Gain { gain: 2.0 })]).unwrap();
    sink.mix(&[0.25, 0.5], Some(0));
    let mut out = [0.0f32; 2];
    sink.process(&mut out);
    assert_eq!(out, [0.5, 1.0]);
}

#[test]
fn pipewire_param_after_enqueue_affects_already_queued_audio() {
    let bits = Arc::new(AtomicU32::new(1.0f32.to_bits()));
    struct LiveGain {
        bits: Arc<AtomicU32>,
    }
    impl Insert for LiveGain {
        fn process(&mut self, frames: &mut [f32]) {
            let gain = f32::from_bits(self.bits.load(Ordering::Relaxed));
            for sample in frames {
                *sample *= gain;
            }
        }
    }

    let mut sink = PipeWireSink::new(44100, 1);
    sink.set_inserts(vec![boxed(LiveGain {
        bits: Arc::clone(&bits),
    })])
    .unwrap();
    sink.mix(&[1.0], Some(0));
    bits.store(0.25f32.to_bits(), Ordering::Relaxed);
    let mut out = [0.0f32; 1];
    sink.process(&mut out);
    assert_eq!(out, [0.25]);
}

#[test]
fn pipewire_integrator_state_survives_two_process_calls() {
    let mut sink = PipeWireSink::new(44100, 1);
    sink.set_inserts(vec![boxed(Integrator { acc: 0.0 })]).unwrap();
    sink.mix(&[1.0, 1.0], Some(0));
    let mut a = [0.0f32; 1];
    sink.process(&mut a);
    let mut b = [0.0f32; 1];
    sink.process(&mut b);
    assert_eq!(a, [1.0]);
    assert_eq!(b, [2.0]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q insert`

Expected: FAIL — `process` output equals dry mix (`[0.25, 0.5]` / `[1.0]`), or default no-op `set_inserts` ignores the chain.

**Step 3: Write minimal implementation**

In `src/sink_pipewire.rs`:

- Import `apply_interleaved`, `Insert`, `InsertChain`, `InsertError`.
- Add `inserts: InsertChain` to `MixStorage` (same mutex as slots so `process` takes one lock).
- `MixStorage::new`: `inserts: InsertChain::new()`.
- After the slot-sum loop in `MixStorage::render` (before `self.playback = end`), apply:

```rust
        let channels_u16 = channels as u16;
        apply_interleaved(&mut self.inserts, output, channels_u16);
        self.playback = end;
```

Apply **after** summing dry slots, **before** `StreamBackend::process`.

- `PipeWireSink::set_inserts`: lock `self.mix`, `mix.inserts.set(inserts)`.
- `PipeWireSink::apply_inserts`: lock and `apply_interleaved` on the caller buffer (for trait completeness; `process` uses the mix-lock path).
- `impl Sink for PipeWireSink<B>`: override `set_inserts` / `apply_inserts` (replace Task 2 no-ops).

Do not apply inserts inside `enqueue`. Slots stay dry.

**Step 4: Run tests to verify they pass**

Run: `cargo test -q insert && cargo test -q pipewire_sink`

Expected: PASS. Existing `pipewire_sink_mixes_and_write_cursor` still `out == [1.25, 0.5]` (empty chain).

**Step 5: Commit**

```bash
git add src/sink_pipewire.rs tests/insert.rs
git commit -m "feat(sink): run insert chain in PipeWire process"
```

---

### [x] Task 5: DeviceSink callback applies chain

**Status:** `done`
**Resume:** —
**Commits:** `1067763` feat(sink): run insert chain in DeviceSink callback

**Files:**
- Modify: `src/sink_device.rs`
- Test: `src/sink_device.rs` `#[cfg(test)]` (Playback has no device)

**Step 1: Write the failing test**

Add to `src/sink_device.rs` tests:

```rust
    use crate::insert::Insert;

    struct Gain {
        gain: f32,
    }
    impl Insert for Gain {
        fn process(&mut self, frames: &mut [f32]) {
            for sample in frames {
                *sample *= self.gain;
            }
        }
    }

    #[test]
    fn playback_process_output_applies_inserts_after_mix() {
        let mut playback = Playback::default();
        playback
            .set_inserts(vec![Box::new(Gain { gain: 2.0 })])
            .unwrap();
        playback.mix(&[0.25, 0.5], Some(0));
        let mut out = [0.0f32; 2];
        playback.process_output(&mut out, 1);
        assert_eq!(out, [0.5, 1.0]);
        assert_eq!(playback.mono, [0.25, 0.5]);
    }

    #[test]
    fn playback_process_output_stereo_duplicates_after_mono_insert() {
        let mut playback = Playback::default();
        playback
            .set_inserts(vec![Box::new(Gain { gain: 2.0 })])
            .unwrap();
        playback.mix(&[0.25], Some(0));
        let mut out = [0.0f32; 2];
        playback.process_output(&mut out, 2);
        assert_eq!(out, [0.5, 0.5]);
    }
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q --lib playback_process_output`

Expected: FAIL — `set_inserts` / `process_output` missing on `Playback`.

**Step 3: Write minimal implementation**

Add `inserts: InsertChain` to `Playback` (update `Default`).

```rust
impl Playback {
    fn set_inserts(&mut self, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError> {
        self.inserts.set(inserts)
    }

    fn process_output(&mut self, output: &mut [f32], channels: u16) {
        let ch = usize::from(channels.max(1));
        if ch == 0 || output.is_empty() {
            return;
        }
        let n_frames = output.len() / ch;
        for frame in 0..n_frames {
            let sample = self.sample_at(self.playback_cursor);
            self.playback_cursor = self.playback_cursor.saturating_add(1);
            let base = frame * ch;
            for c in 0..ch {
                output[base + c] = sample;
            }
        }
        apply_interleaved(&mut self.inserts, output, channels);
        self.reclaim_consumed();
    }
}
```

`DeviceSink::set_inserts`: `lock(&self.state).set_inserts(inserts)`.

`DeviceSink::apply_inserts`: `lock` and `apply_interleaved` on the caller slice (trait). The callback must use `process_output` so already-queued `mono` stays dry.

Change `build_typed_stream` to capture `channels` and fill an `f32` period then convert. To stay allocation-free, process **one frame at a time** through the same helper as `apply_interleaved` for `channels` samples:

```rust
            move |output: &mut [T], _: &OutputCallbackInfo| {
                let mut state = lock(&state);
                for frame in output.chunks_mut(channels) {
                    let sample = state.sample_at(state.playback_cursor);
                    state.playback_cursor = state.playback_cursor.saturating_add(1);
                    let mut mono = [sample];
                    apply_interleaved(&mut state.inserts, &mut mono, 1);
                    let converted = T::from_sample(mono[0].clamp(-1.0, 1.0));
                    for output_sample in frame {
                        *output_sample = converted;
                    }
                }
                state.reclaim_consumed();
            }
```

One-sample `process` matches Task 1 chunking contract. Capture `channels` as `usize::from(config.channels)`.

`impl Sink for DeviceSink`: `set_inserts` / `apply_inserts`.

**Step 4: Run tests to verify they pass**

Run: `cargo test -q --lib playback_process_output && cargo test -q --lib playback_stages && cargo test -q insert && cargo test -q pipewire_sink`

Expected: PASS.

**Step 5: Commit**

```bash
git add src/sink_device.rs
git commit -m "feat(sink): run insert chain in DeviceSink callback"
```

---

### [x] Task 6: Patch maps and output docs

**Status:** `done`
**Resume:** —
**Commits:** `e456e7b` docs: map playback inserts onto sinks and context

**Files:**
- Modify: `.features/sinks.yaml`, `.features/context.yaml`
- Modify: `docs/reference/output.md`
- Modify: `.cursor/skills/drywet/references/rust-api.md`, `.claude/skills/drywet/references/rust-api.md`, `.agents/skills/drywet/references/rust-api.md`
- Do **not** create `.features/insert.yaml`
- Do **not** change `.features/engine.yaml` (no new cmds)

**Step 1: Patch feature maps (dense fields only)**

`.features/sinks.yaml`:

- `entry_points`: add `src/insert.rs`
- `core_components`: add `Insert: src/insert.rs` and `InsertChain: src/insert.rs`
- `user_flow.insert`: `App calls ctx.set_inserts(...) → mix/write stay dry; render/process/callback run the chain`
- `notes`: keep the no-node-graph sentence; add `Playback inserts run at output, not inside mix.`

`.features/context.yaml`:

- `user_flow.render`: `App calls ctx.render(duration) → starts clock, fires events, pads the sink, returns wet PCM; sink.frames() stays dry`
- `user_flow.insert`: `App calls ctx.set_inserts(...) → forwards to the owned sink`

**Step 2: Validate maps**

Run:

```bash
export PATH="$HOME/.local/bin:$PATH"
./bin/feature-map validate
./bin/feature-map check
./bin/feature-map search insert
```

Expected: validate pass, no stale paths, `search insert` hits `sinks` and `context`.

**Step 3: Docs and rust-api**

`docs/reference/output.md` — add a **Playback insert** section after Buffer output. Cover:

- `mix` / `write` stay dry
- `ctx.set_inserts(vec![Box::new(my_insert) as Box<dyn drywet::Insert>])?`
- `ctx.render("1m")` returns a processed copy; `sink.frames()` length matches and stays dry
- `PipeWireSink::process` and the `DeviceSink` callback apply the same chain after summing queued PCM
- empty chain is identity
- no engine NDJSON command for inserts

In each `rust-api.md` (three copies, keep them identical):

- Crate root re-exports: add `Insert`, `InsertChain`, `InsertError`.
- Context methods: `set_inserts`, `clear_inserts`; `render` returns wet PCM.
- Sink trait: `set_inserts`, `apply_inserts`.
- Limits table: `MAX_INSERTS` = 8.
- Short **Inserts** subsection: trait `process(&mut [f32])`, chain order, mix stays dry.

**Step 4: Run tests (docs-only; rust-api is not compiled)**

Run: `cargo test -q insert`

Expected: PASS.

**Step 5: Commit**

```bash
git add .features/sinks.yaml .features/context.yaml docs/reference/output.md \
  .cursor/skills/drywet/references/rust-api.md \
  .claude/skills/drywet/references/rust-api.md \
  .agents/skills/drywet/references/rust-api.md
git commit -m "docs: map playback inserts onto sinks and context"
```

---

### [ ] Task 7: Full-suite regression

**Status:** `implementing`
**Resume:** Run `cargo test -q` and `./bin/feature-map validate && ./bin/feature-map check`. Skip commit if green.
**Commits:** —

**Files:** none unless a test fails.

**Step 1: Run the full suite**

Run: `cargo test -q`

Expected: PASS. No `DRYWET_LIVE_AUDIO`. Existing `tests/buffer_sink.rs`, `tests/pipewire_sink.rs`, `tests/render.rs`, `tests/engine.rs` stay green (empty chain).

**Step 2: Re-check maps**

Run: `./bin/feature-map validate && ./bin/feature-map check`

Expected: pass.

**Step 3: Commit only if Step 1 required a fix**

If green with no extra diff, skip commit and mark the task `done`.

If a fix landed:

```bash
git add -u
git commit -m "test(insert): keep empty-chain sink behavior"
```

---

## Verification (plan complete when Task 7 is `done`)

```bash
cargo test -q
cargo test -q insert
./bin/feature-map validate
./bin/feature-map check
```

Do not run live PipeWire tests. Do not add engine cmds. Do not ship crush or filter types.
