//! The gateware registers the `LSDr`'s bring-up uses: the status word open checks first and
//! the general-purpose output (GPO) core. Facts from the pinned libusdr's
//! `device/m2_lsdr/m2_da09_4_ad45_2.c`.

use hal::FpgaAccess;

/// The chip name failed gateware accesses carry.
pub(crate) const NAME: &str = "FPGA";

/// Register 0 (`M2PCI_REG_STAT_CTRL`, the 8-bit GPO core's base): every GPO write goes
/// through it (`dev_gpo_set` writes `(bank & 0x7f) << 24 | data`), and open reads it once
/// to tell a live board from a dead link.
const REG_STATUS: u32 = 0;

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

    /// Which ADC channels the DSP and PHY settings that follow reach
    /// (`IGPO_BANK_ADC_CHMSK`, commented `[5:4] -- rb_selector; [3:0] -- set_mask`):
    /// libusdr writes [`ALL_CHANNELS`] before each such setting.
    ChannelMask = 5,

    /// The LVDS PHY's control (`IGPO_BANK_ADC_PHY_CTL_0`): see [`PHY_CLOCK_LOAD`].
    PhyControl = 6,

    /// The DSP's second mixer (`IGPO_BANK_ADC_DSP_MIX2`): 3 after a rate change, 0 when
    /// the NCO is set.
    DspMix = 9,

    /// The NCO's frequency word, least significant byte (`IGPO_BANK_ADC_NCO_0`); bytes 1
    /// to 3 follow in banks 11 to 13.
    Nco0 = 10,

    /// The NCO word's second byte (`IGPO_BANK_ADC_NCO_1`).
    Nco1 = 11,

    /// The NCO word's third byte (`IGPO_BANK_ADC_NCO_2`).
    Nco2 = 12,

    /// The NCO word's most significant byte (`IGPO_BANK_ADC_NCO_3`).
    Nco3 = 13,

    /// The DSP chain's reset (`IGPO_BANK_ADC_DSPCHAIN_RST`): libusdr pulses it to 1 and
    /// back to 0.
    DspReset = 14,

    /// The FIR gearbox's microcode port (`IGPO_BANK_ADC_DSPCHAIN_PRG`), a byte per write
    /// (`fgearbox_load_fir`).
    FirProgram = 15,
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

/// [`Gpo::ChannelMask`] selecting all four ADC channels, readback selector 0.
pub(crate) const ALL_CHANNELS: u8 = 0x0f;

/// [`Gpo::PhyControl`]'s clock-delay load (`1 << PHY_LVDS_CLK_LD_OFF`), with the delay tap
/// in bits 4:0.
pub(crate) const PHY_CLOCK_LOAD: u8 = 0x80;

/// The general-purpose input word holding the LMK04832's status (`dev_gpi_get32` of
/// `IGPI_BANK_LMK_STAT`, bank 7: register `16 + 7 / 4`); libusdr reads and logs it.
pub(crate) const CLOCK_STATUS: FpgaAccess = FpgaAccess::Read { reg: 17 };
