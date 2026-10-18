//! The LO-leakage and IQ-imbalance calibration on [`NativeUsb`]: the blocking board's own
//! [`Calibrate`] machine (`board/calibration.rs`), run one action at a time on the
//! handle's RF state and corrections, which it takes back when the machine ends.

use std::time::Instant;

use super::{NativeEvent, NativeUsb, SequenceOperation};
use crate::board::{Calibrate, Calibration};
use crate::error::Error;

impl NativeUsb {
    /// Starts the calibrations `request` asks for, as
    /// [`Device::calibrate`](crate::Device::calibrate) does: the same machine, its waits
    /// (the chains' wake-ups, the DC estimator's polls) caller deadlines. The corrections
    /// found stay applied and kept for the stream group's create. [`poll`](Self::poll)
    /// reports [`NativeEvent::Calibrated`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotPowered`]
    /// before a power-up; [`Error::AlreadyStreaming`] while a stream group exists;
    /// [`Error::NoSampleRate`] before any sample rate; [`Error::Bus`] if the handle is
    /// closed.
    pub fn start_calibrate(&mut self, request: Calibration, now: Instant) -> Result<(), Error> {
        let radio = self.refuse_radio_operation()?;
        if self.group.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let calibrate = Calibrate::new(radio, self.session.board.corrections, request)?;
        self.start_sequence(SequenceOperation::Calibrate(calibrate), now)
    }

    /// Takes back what a finished calibration changed; its event, with the run's failure
    /// first, else the machine's report or fault.
    pub(super) fn finish_calibration(
        &mut self,
        calibrate: &Calibrate,
        result: Result<(), Error>,
    ) -> NativeEvent {
        let (radio, corrections, outcome) = calibrate.finish();
        self.session.board.radio = Some(radio);
        self.session.board.corrections = corrections;
        NativeEvent::Calibrated(result.and(outcome))
    }
}
