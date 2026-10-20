//! The seam's setup as `rtlsdr-sim` records it, shared by the test ports.

use rtlsdr::control::Setup;
use rtlsdr_sim::Control;

/// `setup` as the simulator and traces see it, its direction bit (USB 2.0 Table 9-2, bit 7)
/// set for IN and cleared for OUT, as the usb port sends it.
pub(crate) fn control(setup: Setup, direction_in: bool) -> Control {
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
