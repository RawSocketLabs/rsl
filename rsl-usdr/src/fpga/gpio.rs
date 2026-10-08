//! General-purpose outputs (one 8-bit latch per bank) and inputs (byte-addressed words).
//!
//! Source: `device/generic_usdr/generic_regs.h` and `dev_gpo_set`/`dev_gpi_get32` in
//! `device/m2_lm6_1/usdr_ctrl.c`.

use bnb::{bitfield, u7};

use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

/// FPGA register latching one GPO bank per write.
const REG_GPO: u32 = 0;
/// First FPGA register of the general-purpose inputs.
const REG_GPI_BASE: u32 = 16;

/// A write to [`REG_GPO`]. Bit 31 set would load the I2C address LUT instead.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GpoWrite {
    /// Which output bank to latch.
    #[bits(24..=30)]
    bank: u7,
    /// The bank's new value.
    #[bits(0..=7)]
    data: u8,
}

/// The board's general-purpose output banks (`IGPO_*` in `usdr_ctrl.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gpo {
    /// LMS6002D reset, active low.
    LmsReset = 0,
    /// External RX mixer enable.
    RxMixerEnable = 1,
    /// TX antenna switch.
    TxSwitch = 2,
    /// RX antenna switch.
    RxSwitch = 3,
    /// RF booster power.
    Booster = 7,
    /// Status LED.
    Led = 8,
    /// FPGA DC correction.
    DcCorrection = 9,
    /// On-board oscillator enable (revision 3).
    EnableOscillator = 17,
}

impl Gpo {
    /// Latches `value` into this bank.
    pub(crate) fn set(self, bus: &mut dyn Bus, value: u8) -> Result<(), Error> {
        let write = GpoWrite::new()
            .with_bank(u7::new(self as u8))
            .with_data(value);
        bus.write_regs(REG_GPO, &[write.to_raw()])
            .during("GPO write")
    }
}

/// General-purpose input words, by byte offset (`IGPI_*` in `generic_regs.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gpi {
    /// Xilinx `USR_ACCESS2`: the gateware build ID.
    UsrAccess2 = 0,
    /// Hardware ID; see [`Hwid`].
    Hwid = 12,
}

impl Gpi {
    /// Reads the 32-bit input word holding this input.
    pub(crate) fn read(self, bus: &mut dyn Bus) -> Result<u32, Error> {
        let mut word = [0];
        bus.read_regs(REG_GPI_BASE + self as u32 / 4, &mut word)
            .during("GPI read")?;
        Ok(word[0])
    }
}

/// The hardware ID word.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hwid {
    /// The board has an RX chain.
    #[bits(25..=25)]
    has_rx: bool,
    /// The board has a TX chain.
    #[bits(24..=24)]
    has_tx: bool,
    /// Board revision.
    #[bits(8..=15)]
    revision: u8,
}
