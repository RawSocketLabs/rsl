//! Closing an `LSDr`: `usdr_device_m2_d09_4_ad45_2_destroy` in
//! `device/m2_lsdr/m2_da09_4_ad45_2.c`.
//!
//! The Si549's output is gated, the ADC supply (LDO4) switched off, then both ADC sockets
//! told to power their channels and themselves down. libusdr ignores every result there,
//! so each write is a [`Single`] of its own, attempted whatever the others did. Both ADC
//! writes come after their supply is cut, an order in which real hardware may lose them;
//! the order is libusdr's.

use hal::{Register, RegisterAccess, SequenceAction};

use super::ads42lbx9::{self, PowerControl};
use super::si549::{self, OutputControl};
use super::single::Single;
use super::tps6594::{self, Ldo4Control};
use super::wiring::Wiring;

impl Wiring {
    /// The close's writes on this board, in libusdr's order, each its own sequence.
    pub(crate) fn close(self) -> [Single; 4] {
        let [adc0, adc1] = self.adcs;
        [
            Single::new(
                SequenceAction::Access(RegisterAccess::write(
                    self.oscillator,
                    OutputControl::new().with_enabled(false).entry(),
                )),
                si549::NAME,
            ),
            Single::new(
                SequenceAction::Access(RegisterAccess::write(
                    self.pmic,
                    Ldo4Control::switched(false).entry(),
                )),
                tps6594::NAME,
            ),
            Single::new(
                SequenceAction::Fpga(ads42lbx9::write(adc0, PowerControl::OFF)),
                ads42lbx9::NAME,
            ),
            Single::new(
                SequenceAction::Fpga(ads42lbx9::write(adc1, PowerControl::OFF)),
                ads42lbx9::NAME,
            ),
        ]
    }
}
