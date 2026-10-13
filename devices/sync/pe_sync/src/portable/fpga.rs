//! The gateware registers `PE_SYNC`'s open and close use: the general-purpose output (GPO)
//! banks and the status input. Facts and bit meanings from the comments in the pinned
//! libusdr's `device/pe_sync/pe_sync.c`; register numbers from `sync_const.h`.

use rsl_lowlevel::FpgaAccess;

/// The chip name failed gateware accesses carry.
pub(crate) const NAME: &str = "FPGA";

/// The register every GPO write goes through (`REG_WR_GPO`): `dev_gpo_set` writes
/// `(bank & 0x7f) << 24 | data` to register 0.
const REG_GPO: u16 = 0;

/// The status word: general-purpose input `IGPI_STAT` (16), read at register
/// `16 + 16 / 4` by `dev_gpi_get32`. libusdr only logs it; its meaning is not in the source.
pub(crate) const STATUS_READ: FpgaAccess = FpgaAccess::Read { reg: 16 + 16 / 4 };

/// A GPO bank (`enum dev_gpo`); each holds 8 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gpo {
    /// Reference inputs (`IGPO_IN_CTRL`); see [`INPUT_BUFFERS`], [`INPUT_GPS_PPS`] and
    /// [`INPUT_GPS_LDO`].
    Input = 0,

    /// LMX2820 #0 (`IGPO_SY0_CTRL`); see [`SYNTH_LDO`] and [`SYNTH_ENABLE`].
    Synth0 = 1,

    /// LMX2820 #1 (`IGPO_SY1_CTRL`), as [`Gpo::Synth0`].
    Synth1 = 2,

    /// The LMK05318B, the OCXO and the reference distribution buffers (`IGPO_GEN_CTRL`);
    /// see [`GENERATOR_ON`].
    Generator = 3,

    /// The distribution chips' supplies (`IGPO_DISTRIB_CTRL`); see [`DISTRIBUTION_ON`].
    Distribution = 4,

    /// The board LEDs (`IGPO_LED_CTRL`): bits 0, 2, 4 and 6 drive green LEDs 0 to 3, bits
    /// 1, 3, 5 and 7 the red ones; see [`LEDS_RED`] and [`LEDS_GREEN`].
    Leds = 6,
}

impl Gpo {
    /// Writes `data` to this bank.
    pub(crate) const fn write(self, data: u8) -> FpgaAccess {
        FpgaAccess::Write {
            reg: REG_GPO,
            value: (self as u32) << 24 | data as u32,
        }
    }
}

/// [`Gpo::Input`] bit 0: the 1PPS / 10 MHz input buffer and the 1PPS REF ADC on.
pub(crate) const INPUT_BUFFERS: u8 = 1 << 0;

/// [`Gpo::Input`] bit 2: 1PPS from the GPS rather than the external SMA.
pub(crate) const INPUT_GPS_PPS: u8 = 1 << 2;

/// [`Gpo::Input`] bit 3: the GPS LDO on.
pub(crate) const INPUT_GPS_LDO: u8 = 1 << 3;

/// [`Gpo::Synth0`] and [`Gpo::Synth1`] bit 0: the LMX2820's LDO on.
pub(crate) const SYNTH_LDO: u8 = 1 << 0;

/// [`Gpo::Synth0`] and [`Gpo::Synth1`] bit 1: the LMX2820's CE pin high. Bit 2, its MUTE
/// pin, stays low.
pub(crate) const SYNTH_ENABLE: u8 = 1 << 1;

/// [`Gpo::Generator`] as `pe_sync_power_on` sets it: bit 0 the LMK05318B's LDO, bit 1 its
/// PDN pin, bit 2 the OCXO and OCXO-DAC LDO, bit 3 the REFCLK distribution buffer, bit 4
/// the 1PPS distribution buffer, bit 5 the LMK05318B's GPIO0/SYNCN pin; its GPIO1 and
/// GPIO2 pins (bits 6, 7) stay low.
pub(crate) const GENERATOR_ON: u8 = 0b0011_1111;

/// [`Gpo::Distribution`] as `pe_sync_power_on` sets it: bit 0 the distribution logic's
/// global LDO, bits 2:1 the LMX1204 and LMX1214 LDOs, bit 3 the LMK1D1208I buffers, bit 4
/// the FCLK CMOS buffers' LDO. Bit 5, which would select the external LO, REFCLK and SYSREF
/// path over the internal one, stays clear.
pub(crate) const DISTRIBUTION_ON: u8 = 0b0001_1111;

/// [`Gpo::Leds`]: every LED off.
pub(crate) const LEDS_OFF: u8 = 0;

/// [`Gpo::Leds`]: the four red LEDs.
pub(crate) const LEDS_RED: u8 = 0b1010_1010;

/// [`Gpo::Leds`]: the four green LEDs.
pub(crate) const LEDS_GREEN: u8 = 0b0101_0101;

/// [`Gpo::Leds`]: every LED.
pub(crate) const LEDS_ALL: u8 = LEDS_RED | LEDS_GREEN;
