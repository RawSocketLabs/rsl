//! `control::ControlPort` over `rtlsdr_sim::Replay`: the driver's control transfers must
//! follow the recorded trace, and a recorded stall fails the transfer as the sim's port
//! fails a stalled one. Traces hold control transfers only, so the RX stream is refused and
//! sleeps are ignored.

use std::io;
use std::num::NonZeroUsize;
use std::time::Duration;

use rtlsdr::control::{BusError, ControlPort, Setup};
use rtlsdr_sim::{Divergence, Replay, ReplayError, Trace};

use crate::wire::control;

/// A trace being played back.
pub(crate) struct ReplayPort {
    /// The playback.
    replay: Replay,

    /// The first divergence, kept for the test once the driver gives up.
    divergence: Option<Divergence>,
}

impl ReplayPort {
    /// Plays `trace` back.
    pub(crate) fn new(trace: Trace) -> Self {
        Self {
            replay: Replay::new(trace),
            divergence: None,
        }
    }

    /// Ends the session: the first divergence, or the trace's unplayed remainder.
    pub(crate) fn finish(self) -> Result<(), Divergence> {
        self.divergence.map_or_else(|| self.replay.finish(), Err)
    }

    /// A recorded stall as the seam reports it; a divergence is also kept for the test.
    fn failed(&mut self, error: ReplayError) -> BusError {
        match error {
            ReplayError::Stall { .. } => BusError::Io(io::Error::other(error)),
            ReplayError::Diverged(divergence) => {
                let error = BusError::Other(Box::new(divergence.clone()));
                self.divergence.get_or_insert(divergence);
                error
            }
        }
    }
}

impl ControlPort for ReplayPort {
    fn control_in(
        &mut self,
        setup: Setup,
        data: &mut [u8],
        _timeout: Duration,
    ) -> Result<usize, BusError> {
        self.replay
            .control_in(control(setup, true), data)
            .map_err(|error| self.failed(error))
    }

    fn control_out(
        &mut self,
        setup: Setup,
        data: &[u8],
        _timeout: Duration,
    ) -> Result<(), BusError> {
        self.replay
            .control_out(control(setup, false), data)
            .map_err(|error| self.failed(error))
    }

    fn rx_start(&mut self, _transfer_len: usize, _depth: NonZeroUsize) -> Result<(), BusError> {
        Err(BusError::Unsupported(
            "traces record control transfers only",
        ))
    }

    fn rx_read(&mut self, _data: &mut [u8], _timeout: Duration) -> Result<usize, BusError> {
        Err(BusError::Unsupported(
            "traces record control transfers only",
        ))
    }

    fn rx_stop(&mut self) -> Result<(), BusError> {
        Err(BusError::Unsupported(
            "traces record control transfers only",
        ))
    }

    fn sleep(&mut self, _duration: Duration) {}
}
