//! Where a board's chips answer: the addresses every `LSDr` sequence takes.

use hal::{I2cAddr, SpiAddr};

/// The chips' buses and addresses (the board's `enum BUSIDX_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Wiring {
    /// The TPS6594 PMIC, whose LDO4 supplies the ADCs.
    pub(crate) pmic: I2cAddr,

    /// The Si549 sample-clock oscillator.
    pub(crate) oscillator: I2cAddr,

    /// The two ADC sockets, probed in this order.
    pub(crate) adcs: [SpiAddr; 2],

    /// The LMK04832 clock generator's socket.
    pub(crate) clock_generator: SpiAddr,
}
