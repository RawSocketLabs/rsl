//! General-purpose outputs (one 8-bit latch per bank) and inputs (byte-addressed words).
//!
//! The FPGA drives the board's control lines: resets, RF switches, enables, the LED.
//! Each is a GPO bank the host sets with one register write. The GPI words report what
//! the gateware knows about itself and the board, including the revision the driver
//! branches on.
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
    /// LMS6002D reset, active low: 0 holds the chip in reset, 1 releases it. Held low from
    /// identification until the clocks and rails are up, and again at power-down.
    LmsReset = 0,

    /// Board mixer enable: 1 powers the mixer that lifts signals below 230 MHz into the LMS6002D's
    /// range, by adding Si5332 output 3's frequency, and feeds LNA3. libusdr drives it from bit 1
    /// of an RX path's switch setting.
    RxMixerEnable = 1,

    /// TX antenna switch: picks the balun for PA1 (1) or PA2 (0).
    TxSwitch = 2,

    /// RX antenna switch: picks the balun for LNA1 (0) or LNA2 (1); libusdr also sets 1 on
    /// its LNA3 path.
    RxSwitch = 3,

    /// RF booster power, on just before the LMS6002D leaves reset and off at power-down.
    /// libusdr does not document what the line switches.
    Booster = 7,

    /// Status LED: lit while the board is powered up.
    Led = 8,

    /// DC correction enable (`IGPO_DCCORR`). A zero-IF receiver such as the LMS6002D leaves a spike
    /// at 0 Hz that has to be removed. libusdr sets this line once, at the end of power-up, and
    /// never clears it; its RX DC-offset correction is switched separately, through a PHY register
    /// (`usdr_set_rxdccorr`, and on every sample-rate change). What this line alone gates is not
    /// documented.
    DcCorrection = 9,

    /// On-board 26 MHz oscillator enable, revision 3 only: the Si5332's reference there.
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
    /// Xilinx `USR_ACCESS2`: the gateware build ID, stamped into the bitstream when it is
    /// built. libusdr only logs it.
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
