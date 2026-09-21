//! Musician-facing Transport, time, Sampler / Synth / Drum, and an NDJSON
//! stdio host for GUIs that cannot link Rust.
//!
//! Standalone Rust apps (Ply, egui, …) construct a [`Context`] in-process
//! and trigger notes from the UI thread. Omarchy QML widgets spawn the
//! `drywet-engine` binary and speak one JSON object per line. See
//! `docs/gui.md` for both hookups.
//!
//! ```
//! use drywet::sink::Sink;
//! use drywet::{Context, Synth};
//!
//! let ctx = Context::new();
//! let mut synth = Synth::new(&ctx);
//! ctx.sink_mut().start_clock();
//! ctx.transport().start();
//! synth
//!     .trigger_attack_release(&ctx, "C4", "8n", None)
//!     .expect("live pad");
//! assert!(ctx.sink().accepted());
//! ```

/// Crate version string.
pub const VERSION: &str = "0.1.0";

pub mod context;
pub mod engine;
pub mod event;
pub mod instrument;
pub mod limits;
pub mod pitch;
pub mod sink;
mod sink_pipewire;
pub mod time;
pub mod transport;

pub use context::Context;
pub use engine::run;
pub use event::{Loop, Part, Sequence};
pub use instrument::{Drum, Sampler, Synth};
pub use sink::{BufferSink, PipeWireSink, Sink};
