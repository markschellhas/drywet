/// Arrangement clock owned by [`crate::Context`].
///
/// This is a constructable stub: the transport exists and reports
/// [`TransportState::Stopped`]. Start, stop, pause, and bpm land in a later task.
#[derive(Debug)]
pub struct Transport {
    state: TransportState,
}

/// Playhead lifecycle. Only [`Stopped`](TransportState::Stopped) is reachable
/// until start/stop are implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    /// Clock is idle; playhead has not started.
    Stopped,
}

impl Transport {
    /// Stopped transport; not attached to a live clock yet.
    pub fn new() -> Self {
        Self {
            state: TransportState::Stopped,
        }
    }

    /// Current lifecycle state.
    pub fn state(&self) -> TransportState {
        self.state
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}
