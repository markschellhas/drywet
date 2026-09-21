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
pub use sink::{BufferSink, PipeWireSink};
