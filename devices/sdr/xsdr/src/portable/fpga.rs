//! The gateware registers the LMS7 family's bring-up uses: the general-purpose output
//! (GPO) core and the hardware-ID input. Facts from the pinned libusdr's
//! `device/m2_lm7_1/xsdr_ctrl.{c,h}` and `device/generic_usdr/generic_regs.h`.

use rsl_lowlevel::FpgaAccess;

/// The chip name failed gateware accesses carry.
pub(crate) const NAME: &str = "FPGA";

/// The register every GPO write goes through: `dev_gpo_set` writes
/// `(bank & 0x7f) << 24 | data` to register 0.
const REG_GPO: u16 = 0;

/// General-purpose input word 3, the hardware ID (`IGPI_HWID` = 12, read at
/// `16 + bank / 4` by `dev_gpi_get32`).
pub(crate) const REG_HWID: u16 = 16 + 12 / 4;

/// A GPO bank (`enum IGPO_*` in `xsdr_ctrl.h`); each holds 8 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gpo {
    /// LMS7002M power control (`IGPO_LMS_PWR`): see [`LMS_LDO_ENABLE`],
    /// [`LMS_RESET_RELEASE`] and [`LMS_PULSE`].
    LmsPower = 0,

    /// Reference clock configuration (`IGPO_CLK_CFG`); see [`CLOCK_INTERNAL`].
    ClockConfig = 1,

    /// The LMS7002M's external LDOs (`IGPO_LDOLMS_EN`): 1 enables them.
    LmsLdo = 17,
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

/// [`Gpo::LmsPower`] bit 0: the LMS7002M's LDOs on (`IGPO_LMS_PWR_LDOEN`).
pub(crate) const LMS_LDO_ENABLE: u8 = 1 << 0;

/// [`Gpo::LmsPower`] bit 3: the LMS7002M out of reset (`IGPO_LMS_PWR_NRESET`, active low
/// on the pin; set here means running).
pub(crate) const LMS_RESET_RELEASE: u8 = 1 << 3;

/// [`Gpo::LmsPower`] bit 7, which `_xsdr_init_revx` sets and clears again before anything
/// else. The pinned headers name only bits 3:0, so its meaning is undocumented: kept as the
/// raw bit libusdr writes.
pub(crate) const LMS_PULSE: u8 = 1 << 7;

/// [`Gpo::ClockConfig`] value `_xsdr_init_revx` writes, commented "Enable internal clocking
/// by default". `xsdr_set_extref` reads the same bits as bit 0 = USB transport, bit 1 =
/// external reference; with no evidence which reading the gateware follows, the raw value
/// is kept.
pub(crate) const CLOCK_INTERNAL: u8 = 1;
