//! The gateware registers the `LSDr`'s bring-up uses: the status word open checks first and
//! the general-purpose output (GPO) core. Facts from the pinned libusdr's
//! `device/m2_lsdr/m2_da09_4_ad45_2.c`.

use rsl_lowlevel::FpgaAccess;

/// The chip name failed gateware accesses carry.
pub(crate) const NAME: &str = "FPGA";

/// Register 0 (`M2PCI_REG_STAT_CTRL`, the 8-bit GPO core's base): every GPO write goes
/// through it (`dev_gpo_set` writes `(bank & 0x7f) << 24 | data`), and open reads it once
/// to tell a live board from a dead link.
const REG_STATUS: u16 = 0;

/// Open's liveness read of [`REG_STATUS`]; libusdr reports "Device is dead!" when it reads
/// [`DEAD`].
pub(crate) const LIVENESS: FpgaAccess = FpgaAccess::Read { reg: REG_STATUS };

/// What a dead link reads back: all ones.
pub(crate) const DEAD: u32 = u32::MAX;

/// A GPO bank (`enum dev_gpo` in `m2_da09_4_ad45_2.c`); each holds 8 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gpo {
    /// The ADCs' reset lines and the receive FIFO resets (`IGPO_BANK_CFG_ADC`, commented
    /// `[7:4] -- FIFO reset;  [1:0] adc_reset`): see [`ADC_RESET`] and [`FIFO_RESET`].
    AdcConfig = 2,

    /// The receive PGA's latch enable (`IGPO_BANK_PGA_LE`): libusdr pulses it to
    /// [`PGA_LATCH`] and back to 0 after each gain write.
    PgaLatch = 3,

    /// The receive PGA's gain code (`IGPO_BANK_PGA_GAIN`).
    PgaGain = 4,
}

impl Gpo {
    /// Writes `data` to this bank.
    pub(crate) const fn write(self, data: u8) -> FpgaAccess {
        FpgaAccess::Write {
            reg: REG_STATUS,
            value: (self as u32) << 24 | data as u32,
        }
    }
}

/// [`Gpo::AdcConfig`] bits 1:0, one per ADC in libusdr's activation mask: set holds that
/// ADC in reset.
pub(crate) const ADC_RESET: u8 = 0x03;

/// [`Gpo::AdcConfig`] bits 7:4: the FIFO resets libusdr pulses while releasing the ADCs.
pub(crate) const FIFO_RESET: u8 = 0xf0;

/// [`Gpo::PgaLatch`]'s value while libusdr latches a new gain: all four latch bits.
pub(crate) const PGA_LATCH: u8 = 0x0f;
