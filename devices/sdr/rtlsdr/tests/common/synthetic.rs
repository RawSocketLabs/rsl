//! SYNTHETIC wiring and session for exercising the seam, the simulator and the replay
//! harness. **Not RTL2832U values**: the request codes and gate location are invented
//! for the harness (the real ones are `RTL-GAP-04` to `RTL-GAP-06`). The only real facts
//! used are the R820T2's, through `rtlsdr::r820t2`, and `bmRequestType`'s vendor-type
//! encoding from USB 2.0 §9.3.1 (Table 9-2): type bits 6:5 = 2.

use std::time::Duration;

use rtlsdr::control::{BusError, ControlPort, Setup};
use rtlsdr::r820t2::{Registers, field};
use rtlsdr_sim::{GateBit, Wiring};

/// SYNTHETIC: the request the simulator routes to its register store.
pub(crate) const STORE_REQUEST: u8 = 0xE1;

/// SYNTHETIC: the request the simulator routes to the tuner's I2C port.
pub(crate) const TUNER_REQUEST: u8 = 0xE2;

/// SYNTHETIC: where the repeater gate sits in the register store.
pub(crate) const GATE: GateBit = GateBit {
    value: 0x00E3,
    index: 0x00E4,
    bit: 3,
};

/// USB 2.0 Table 9-2: a vendor request to the device (direction set by the port).
const VENDOR_DEVICE: u8 = 0b0100_0000;

/// A control timeout; nothing simulated waits on it.
pub(crate) const TIMEOUT: Duration = Duration::from_millis(100);

/// The SYNTHETIC wiring.
pub(crate) fn wiring() -> Wiring {
    Wiring {
        register_request: STORE_REQUEST,
        tuner_request: TUNER_REQUEST,
        gate: GATE,
    }
}

/// A setup for `request` at (`value`, `index`).
pub(crate) fn setup(request: u8, value: u16, index: u16) -> Setup {
    Setup {
        request_type: VENDOR_DEVICE,
        request,
        value,
        index,
    }
}

/// The repeater gate's register-store write: `open` sets the gate bit.
fn gate(port: &mut dyn ControlPort, open: bool) -> Result<(), BusError> {
    let byte = u8::from(open) << GATE.bit;
    port.control_out(
        setup(STORE_REQUEST, GATE.value, GATE.index),
        &[byte],
        TIMEOUT,
    )
}

/// The SYNTHETIC session: opens the repeater, page-writes a gain setting (R5-R7) built by
/// `rtlsdr`'s R820T2 code, reads R0-R7 back, closes the repeater, and returns what the read
/// decoded to.
pub(crate) fn session(port: &mut dyn ControlPort) -> Result<Registers, BusError> {
    let mut wanted = Registers::default();
    wanted.set(field::LNA_GAIN, 0xA).expect("fits");
    wanted.set(field::MIX_GAIN, 0x5).expect("fits");
    let write = wanted.page_write(0x05, 3).expect("R5-R7 are writable");

    gate(port, true)?;
    port.control_out(setup(TUNER_REQUEST, 0, 0), write.as_bytes(), TIMEOUT)?;
    let mut read = [0; 8];
    let len = port.control_in(setup(TUNER_REQUEST, 0, 0), &mut read, TIMEOUT)?;
    gate(port, false)?;
    Ok(Registers::from_read(&read[..len]).expect("eight registers fit"))
}
