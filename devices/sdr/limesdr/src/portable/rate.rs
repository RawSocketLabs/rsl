//! Setting the LimeSDR-mini's sample rate: libusdr's `limesdr_set_samplerate`
//! (`device/u3_limesdr/limesdr_ctrl.c`). The LMS7002M's rate (`lms7002m_samplerate`, in
//! `lms7002m`'s control code) in SISO DDR both ways, the converters as fast as the
//! processors allow, RX on `LimeLight` port 2; then both `LimeLight` clocks direct.

use hal::{Sequence, SequenceAction, SequenceFailure};
use lms7002m::ctrl::{Control, ControlError, Operation, RateFlags, RateRequest};

use super::fpga::DIRECT_CLOCKING;
use super::modify::Modify;

/// The control code's request for `rx` and `tx` samples per second (0 for none): libusdr's
/// `XSDR_SR_MAXCONVRATE | XSDR_LML_SISO_DDR_RX | XSDR_LML_SISO_DDR_TX`, `rx_port_1`
/// false ("LML2 - RX"), no board decimation or interpolation, no converter floors.
const fn request(rx: u32, tx: u32) -> RateRequest {
    RateRequest {
        rx,
        tx,
        adc: 0,
        dac: 0,
        flags: RateFlags {
            max_conversion: true,
            extended_cgen: false,
            rx_siso_ddr: true,
            tx_siso_ddr: true,
            rx_external_fifo_clock: false,
            tx_external_fifo_clock: false,
        },
        rx_port1: false,
        rx_decimation: 1,
        tx_interpolation: 1,
    }
}

/// `limesdr_set_samplerate`: the control code's rate, then, if it was set, direct
/// clocking for `LimeLight` clocks 0 and 1, each a read-modify-write.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SetSampleRate {
    /// The control code's rate, holding the state.
    rate: Operation,
    /// The direct-clocking writes, clock 0 then 1.
    clocks: [Modify; 2],
    /// A direct-clocking access failed, which ends the rate.
    failed: bool,
}

impl SetSampleRate {
    /// Starts setting `rx` and `tx` samples per second on `control`.
    pub(crate) fn new(control: Control, rx: u32, tx: u32) -> Self {
        Self {
            rate: control.sample_rate(request(rx, tx)),
            clocks: [0, 1].map(|clock| Modify::new(DIRECT_CLOCKING, 0, 1 << clock)),
            failed: false,
        }
    }

    /// The control state, as the rate has left it so far.
    pub(crate) const fn control(&self) -> &Control {
        self.rate.control()
    }

    /// The control code's refusal, once it refused; `None` otherwise, a bus failure
    /// included (the executor reports that).
    pub(crate) fn refusal(&self) -> Option<ControlError> {
        self.rate.outcome().and_then(Result::err)
    }

    /// The direct-clocking write with an action pending, once the rate is set and while
    /// none failed.
    fn clock(&mut self) -> Option<&mut Modify> {
        if self.failed || self.rate.outcome() != Some(Ok(())) {
            return None;
        }
        self.clocks.iter_mut().find(|clock| clock.next().is_some())
    }
}

impl Sequence for SetSampleRate {
    fn next(&self) -> Option<SequenceAction> {
        if self.rate.outcome() != Some(Ok(())) {
            return self.rate.next();
        }
        if self.failed {
            return None;
        }
        self.clocks.iter().find_map(Sequence::next)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.rate.next().is_some() {
            return self.rate.complete(result);
        }
        let Some(clock) = self.clock() else {
            return Ok(());
        };
        let completed = clock.complete(result);
        self.failed = completed.is_err();
        completed
    }
}
