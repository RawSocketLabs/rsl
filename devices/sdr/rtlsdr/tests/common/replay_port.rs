//! `control::ControlPort` over `rtlsdr_sim::Replay`: the driver's control transfers must
//! follow the recorded trace. Traces hold control transfers only, so the RX stream is
//! refused and sleeps are ignored.

use std::num::NonZeroUsize;
use std::time::Duration;

use rtlsdr::control::{BusError, ControlPort, Setup};
use rtlsdr_sim::{Control, Divergence, Replay, Trace};

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

    /// Keeps a divergence and reports it to the driver as a transport failure.
    fn diverged(&mut self, divergence: Divergence) -> BusError {
        let error = BusError::Other(Box::new(divergence.clone()));
        self.divergence.get_or_insert(divergence);
        error
    }
}

/// The trace's form of a setup, its direction bit (USB 2.0 Table 9-2, bit 7) set for IN as
/// the usb port sets it.
fn control(setup: Setup, direction_in: bool) -> Control {
    Control {
        request_type: if direction_in {
            setup.request_type | 0x80
        } else {
            setup.request_type & !0x80
        },
        request: setup.request,
        value: setup.value,
        index: setup.index,
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
            .map_err(|divergence| self.diverged(divergence))
    }

    fn control_out(
        &mut self,
        setup: Setup,
        data: &[u8],
        _timeout: Duration,
    ) -> Result<(), BusError> {
        self.replay
            .control_out(control(setup, false), data)
            .map_err(|divergence| self.diverged(divergence))
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
