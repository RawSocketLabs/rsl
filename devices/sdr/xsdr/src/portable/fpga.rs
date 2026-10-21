//! The gateware registers the LMS7 family's bring-up uses: the general-purpose output
//! (GPO) core and the hardware-ID input. Facts from the pinned libusdr's
//! `device/m2_lm7_1/xsdr_ctrl.{c,h}` and `device/generic_usdr/generic_regs.h`.

use hal::FpgaAccess;

/// The chip name failed gateware accesses carry.
pub(crate) const NAME: &str = "FPGA";

/// The register every GPO write goes through: `dev_gpo_set` writes
/// `(bank & 0x7f) << 24 | data` to register 0.
const REG_GPO: u32 = 0;

/// General-purpose input word 3, the hardware ID (`IGPI_HWID` = 12, read at
/// `16 + bank / 4` by `dev_gpi_get32`).
pub(crate) const REG_HWID: u32 = 16 + 12 / 4;

/// A GPO bank (`enum IGPO_*` in `xsdr_ctrl.h`); each holds 8 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gpo {
    /// LMS7002M power control (`IGPO_LMS_PWR`): see [`LMS_LDO_ENABLE`],
    /// [`LMS_RESET_RELEASE`] and [`LMS_PULSE`].
    LmsPower = 0,

    /// Reference clock configuration (`IGPO_CLK_CFG`); see [`CLOCK_INTERNAL`].
    ClockConfig = 1,

    /// The TX antenna switch (`IGPO_TXSW`): the TX band's switch position.
    TxSwitch = 2,

    /// The RX antenna switch (`IGPO_RXSW`): the RX band's switch position.
    RxSwitch = 3,

    /// The external GPIOs' I/O voltage (`IGPO_IOVCCSEL`): the XTRX's open writes 1, "Set
    /// external GPIOs to 3.3V".
    IoVoltage = 8,

    /// The LMS7002M's external LDOs (`IGPO_LDOLMS_EN`): 1 enables them.
    LmsLdo = 17,

    /// The board LED (`IGPO_LED`): `xsdr_dtor` writes 0.
    Led = 18,

    /// The TX DSP chain's resets (`IGPO_DSPCHAIN_TX_RST`): bit 2 the extended front end's
    /// (`xsdr_reset_extfe`).
    #[allow(
        dead_code,
        reason = "the TX stream uses it; the no_std fixture compiles the tree without it"
    )]
    TxChainReset = 23,
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

/// The RX PHY's configuration port (`REG_CFG_PHY_0`, `generic_regs.h`): `xsdr_phy_rx_reg`
/// writes `write << 31 | bank << 24 | address << 16 | value` here, and a read of it returns
/// what the last request selected.
pub(crate) const REG_CFG_PHY_0: u32 = 56;

/// An RX PHY bank (`PHY_REG_RXBANK_*`, `xsdr_ctrl.c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RxPhyBank {
    /// Control (`PHY_REG_RXBANK_CTRL`): address 0 is the deserializer.
    Control = 0,

    /// The LFSR checkers (`PHY_REG_RXBANK_LFSRCHK`).
    LfsrCheck = 3,

    /// The A/B and I/Q alignment checker (`PHY_REG_RXBANK_IQABCHK`).
    IqAbCheck = 4,

    /// The I/Q capture (`PHY_REG_RXBANK_CAPTURE`): address 0 starts and stops it, a read
    /// request selects a sample (bit 7 channel B).
    #[allow(
        dead_code,
        reason = "the host-side quality check uses it; the no_std fixture compiles the tree without it"
    )]
    Capture = 5,
}

impl RxPhyBank {
    /// `xsdr_phy_rx_reg(d, write, bank, address, value)`'s register write.
    pub(crate) const fn access(self, write: bool, address: u8, value: u16) -> FpgaAccess {
        FpgaAccess::Write {
            reg: REG_CFG_PHY_0,
            value: if write { 1 << 31 } else { 0 }
                | (self as u32) << 24
                | (address as u32) << 16
                | value as u32,
        }
    }
}

/// The delay `_xsdr_calibrate_lml` writes without a TX MMCM and no override: 3.
pub(crate) const TX_DEFAULT_DELAY: FpgaAccess = TxPhy::Delay.write(3);

/// [`Gpo::LmsPower`] bit 1: the LMS7002M's RX enable pin (`IGPO_LMS_PWR_RXEN`).
pub(crate) const LMS_RX_ENABLE: u8 = 1 << 1;

/// [`Gpo::LmsPower`] bit 2: the LMS7002M's TX enable pin (`IGPO_LMS_PWR_TXEN`).
pub(crate) const LMS_TX_ENABLE: u8 = 1 << 2;

/// The TX PHY's configuration port (`REG_CFG_PHY_1`, `generic_regs.h`): `xsdr_phy_tx_reg`
/// writes `register << 24 | value & 0xFFFFFF` here; a read returns the port's status (the
/// TX MMCM's DRP data in bits 31:16, its lock in bit 8, a busy DRP in bit 9).
pub(crate) const REG_CFG_PHY_1: u32 = 57;

/// A TX PHY register (`PHY_REG_*`, `xsdr_ctrl.c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TxPhy {
    /// The TX serializer's control (`PHY_REG_PORT_CTRL`): bit 0 runs it, bit 4 SISO DDR.
    PortControl = 0,

    /// The TX MMCM's dynamic reconfiguration port (`PHY_REG_MMCM_DRP`).
    MmcmDrp = 1,

    /// The TX MMCM's control (`PHY_REG_MMCM_CTRL`): bit 0 reset, bit 2 TX master; 2
    /// powers it down.
    MmcmControl = 2,

    /// The TX clock delay without a TX MMCM (`PHY_REG_DLY_VALUE`).
    Delay = 4,

    /// Which lanes carry I and Q (`PHY_REG_PORT_IQSEL`).
    IqSelect = 5,

    /// The LFSR generator (`PHY_REG_LFSR_GEN`): 1 runs it.
    LfsrGenerator = 6,

    /// The generator's pattern: 0 the LFSR, 1 the A/B-I/Q table (`PHY_REG_IQAB_GEN`).
    IqAbGenerator = 7,

    /// Pushes a word into channel A's A/B-I/Q table (`PHY_REG_IQAB_PSHA`).
    IqAbPushA = 8,

    /// Channel B's (`PHY_REG_IQAB_PSHB`).
    IqAbPushB = 9,
}

impl TxPhy {
    /// `xsdr_phy_tx_reg(d, self, value)`'s port word.
    pub(crate) const fn word(self, value: u32) -> u32 {
        (self as u32) << 24 | value & 0x00ff_ffff
    }

    /// `xsdr_phy_tx_reg(d, self, value)`.
    pub(crate) const fn write(self, value: u32) -> FpgaAccess {
        FpgaAccess::Write {
            reg: REG_CFG_PHY_1,
            value: self.word(value),
        }
    }
}

/// `_xsdr_mmcm_pd`: the TX MMCM powered down (control 2, commented nowhere; the function
/// name says it powers the TX MMCM down).
pub(crate) const TX_MMCM_POWER_DOWN: FpgaAccess = TxPhy::MmcmControl.write(2);

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
