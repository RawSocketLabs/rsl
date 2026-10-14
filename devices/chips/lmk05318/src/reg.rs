//! The register address map.
//!
//! The LMK05318 has one flat 16-bit address space of byte registers, so one `Reg` names
//! every address the driver touches. The value types live in the module for their block.
//! Multi-byte numbers (the YAML's address ranges, such as `PLL1_NDIV` at `0x6C:0x6D`) are
//! sent most significant byte first and written as bytes.
//!
//! `hal`'s shared `Register` traits take 8-bit addresses, so this chip's value
//! types implement the 16-bit [`Lmk05318Register`] and [`IndexedLmk05318Register`] instead.

/// Register addresses, with the YAML's names (`hw/lmk05318/lmk05318.yaml`) in the docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Reg {
    /// The first identification byte; libusdr reads four from here and requires
    /// `10 0B 35 42`. Undocumented in the vendored YAML. 0x0000.
    Id = 0x0000,

    /// See [`DeviceControl`](super::control::DeviceControl). `DEV_CTL`, 0x0C.
    DeviceControl = 0x000c,

    /// See [`LossFlags`](super::control::LossFlags). `INT_FLAG0`, 0x13.
    LossFlags = 0x0013,

    /// See [`DpllFlags`](super::control::DpllFlags). `INT_FLAG1`, 0x14.
    DpllFlags = 0x0014,

    /// See [`InterruptControl`](super::control::InterruptControl). `INTCTL`, 0x15.
    InterruptControl = 0x0015,

    /// See [`MuteLevels`](super::output::MuteLevels), outputs 0 to 3. `MUTELVL1`, 0x17.
    MuteLevels0To3 = 0x0017,

    /// See [`MuteLevels`](super::output::MuteLevels), outputs 4 to 7. `MUTELVL2`, 0x18.
    MuteLevels4To7 = 0x0018,

    /// See [`OutputMute`](super::output::OutputMute). `OUT_MUTE`, 0x19.
    OutputMute = 0x0019,

    /// See [`LockMute`](super::output::LockMute). `DPLL_MUTE`, 0x1D.
    LockMute = 0x001d,

    /// See [`Apll1DenominatorMode`](super::apll::Apll1DenominatorMode).
    /// `SPARE_NVMBASE2_BY2`, 0x27.
    Apll1DenominatorMode = 0x0027,

    /// See [`Apll2DenominatorMode`](super::apll::Apll2DenominatorMode).
    /// `SPARE_NVMBASE2_BY1`, 0x28.
    Apll2DenominatorMode = 0x0028,

    /// See [`XoControl`](super::xo::XoControl). `XO_CLKCTL1`, 0x2A.
    XoControl = 0x002a,

    /// See [`XoInputType`](super::xo::XoInputType). `XO_CLKCTL2`, 0x2B.
    XoInputType = 0x002b,

    /// See [`XoDivider`](super::xo::XoDivider). `XO_CONFIG`, 0x2C.
    XoDivider = 0x002c,

    /// See [`PllClocks`](super::apll::PllClocks). `PLL_CLK_CFG`, 0x2F.
    PllClocks = 0x002f,

    /// See [`OutputControl`](super::output::OutputControl), outputs 0 and 1. `OUTCTL_0`,
    /// 0x33.
    Output0Control = 0x0033,

    /// See [`OutputFormatOnly`](super::output::OutputFormatOnly). `OUTCTL_1`, 0x34.
    Output1Control = 0x0034,

    /// See [`OutputDivider`](super::output::OutputDivider), outputs 0 and 1.
    /// `OUTDIV_0_1`, 0x35.
    Output01Divider = 0x0035,

    /// See [`OutputControl`](super::output::OutputControl), outputs 2 and 3. `OUTCTL_2`,
    /// 0x36.
    Output2Control = 0x0036,

    /// See [`OutputFormatOnly`](super::output::OutputFormatOnly). `OUTCTL_3`, 0x37.
    Output3Control = 0x0037,

    /// See [`OutputDivider`](super::output::OutputDivider), outputs 2 and 3.
    /// `OUTDIV_2_3`, 0x38.
    Output23Divider = 0x0038,

    /// See [`OutputControl`](super::output::OutputControl). `OUTCTL_4`, 0x39.
    Output4Control = 0x0039,

    /// See [`OutputDivider`](super::output::OutputDivider). `OUTDIV_4`, 0x3A.
    Output4Divider = 0x003a,

    /// See [`OutputControl`](super::output::OutputControl). `OUTCTL_5`, 0x3B.
    Output5Control = 0x003b,

    /// See [`OutputDivider`](super::output::OutputDivider). `OUTDIV_5`, 0x3C.
    Output5Divider = 0x003c,

    /// See [`OutputControl`](super::output::OutputControl). `OUTCTL_6`, 0x3D.
    Output6Control = 0x003d,

    /// See [`OutputDivider`](super::output::OutputDivider). `OUTDIV_6`, 0x3E.
    Output6Divider = 0x003e,

    /// See [`OutputControl`](super::output::OutputControl). `OUTCTL_7`, 0x3F.
    Output7Control = 0x003f,

    /// Output 7's second divider stage less one, bits 23:16 (`OUT7_STG2_DIV`, 24 bits;
    /// the total divide is both stages' product). `OUTDIV_7_STG2`, 0x40.
    Output7Stage2High = 0x0040,

    /// Bits 15:8 of [`Reg::Output7Stage2High`]'s value. 0x41.
    Output7Stage2Middle = 0x0041,

    /// Bits 7:0 of [`Reg::Output7Stage2High`]'s value. 0x42.
    Output7Stage2Low = 0x0042,

    /// See [`OutputDivider`](super::output::OutputDivider): output 7's first stage.
    /// `OUTDIV_7`, 0x43.
    Output7Divider = 0x0043,

    /// See [`PostDividerSync`](super::output::PostDividerSync). `OUTSYNCCTL`, 0x46.
    PostDividerSync = 0x0046,

    /// See [`OutputSync`](super::output::OutputSync). `OUTSYNCEN`, 0x47.
    OutputSync = 0x0047,

    /// See [`Pll1Control`](super::apll::Pll1Control). `PLL1_CTRL0`, 0x4A.
    Pll1Control = 0x004a,

    /// See [`Pll1Calibration`](super::apll::Pll1Calibration). `PLL1_CALCTRL0`, 0x4F.
    Pll1Calibration = 0x004f,

    /// See [`BawLock`](super::apll::BawLock). `BAW_LOCKDET_PPM_MAX_BY1`, 0x50.
    BawLock = 0x0050,

    /// BAW VCO lock detection, low byte (`BAW_LOCK_DET_2`); a tuning constant per XO.
    /// `BAW_LOCKDET_PPM_MAX_BY0`, 0x51.
    BawLockPpmMaxLow = 0x0051,

    /// BAW lock-detect counter start, bits 31:24; a tuning constant per XO.
    /// `BAW_LOCKDET_CNTSTRT`, 0x52 (0x52:0x55).
    BawLockCountStart3 = 0x0052,

    /// Bits 23:16 of [`Reg::BawLockCountStart3`]'s value. 0x53.
    BawLockCountStart2 = 0x0053,

    /// Bits 15:8 of [`Reg::BawLockCountStart3`]'s value. 0x54.
    BawLockCountStart1 = 0x0054,

    /// Bits 7:0 of [`Reg::BawLockCountStart3`]'s value. 0x55.
    BawLockCountStart0 = 0x0055,

    /// BAW lock-detect VCO counter start, bits 31:24; a tuning constant per XO.
    /// `BAW_LOCKDET_VCO_CNTSTRT`, 0x56 (0x56:0x59).
    BawLockVcoCountStart3 = 0x0056,

    /// Bits 23:16 of [`Reg::BawLockVcoCountStart3`]'s value. 0x57.
    BawLockVcoCountStart2 = 0x0057,

    /// Bits 15:8 of [`Reg::BawLockVcoCountStart3`]'s value. 0x58.
    BawLockVcoCountStart1 = 0x0058,

    /// Bits 7:0 of [`Reg::BawLockVcoCountStart3`]'s value. 0x59.
    BawLockVcoCountStart0 = 0x0059,

    /// BAW unlock-detect threshold, high byte; a tuning constant per XO.
    /// `BAW_UNLOCKDET_PPM_MAX`, 0x5A (0x5A:0x5B).
    BawUnlockPpmMaxHigh = 0x005a,

    /// Low byte of [`Reg::BawUnlockPpmMaxHigh`]'s value. 0x5B.
    BawUnlockPpmMaxLow = 0x005b,

    /// BAW unlock-detect counter start, bits 31:24; a tuning constant per XO.
    /// `BAW_UNLOCKDET_CNTSTRT`, 0x5C (0x5C:0x5F).
    BawUnlockCountStart3 = 0x005c,

    /// Bits 23:16 of [`Reg::BawUnlockCountStart3`]'s value. 0x5D.
    BawUnlockCountStart2 = 0x005d,

    /// Bits 15:8 of [`Reg::BawUnlockCountStart3`]'s value. 0x5E.
    BawUnlockCountStart1 = 0x005e,

    /// Bits 7:0 of [`Reg::BawUnlockCountStart3`]'s value. 0x5F.
    BawUnlockCountStart0 = 0x005f,

    /// BAW unlock-detect VCO counter start, bits 31:24; a tuning constant per XO.
    /// `BAW_UNLOCKDET_VCO_CNTSTRT`, 0x60 (0x60:0x63).
    BawUnlockVcoCountStart3 = 0x0060,

    /// Bits 23:16 of [`Reg::BawUnlockVcoCountStart3`]'s value. 0x61.
    BawUnlockVcoCountStart2 = 0x0061,

    /// Bits 15:8 of [`Reg::BawUnlockVcoCountStart3`]'s value. 0x62.
    BawUnlockVcoCountStart1 = 0x0062,

    /// Bits 7:0 of [`Reg::BawUnlockVcoCountStart3`]'s value. 0x63.
    BawUnlockVcoCountStart0 = 0x0063,

    /// See [`Pll2Control`](super::apll::Pll2Control). `PLL2_CTRL0`, 0x64.
    Pll2Control = 0x0064,

    /// See [`Pll2PostDividers`](super::apll::Pll2PostDividers). `PLL2_CTRL2`, 0x66.
    Pll2PostDividers = 0x0066,

    /// APLL1's feedback divider integer part, high byte. `PLL1_NDIV`, 0x6C (0x6C:0x6D).
    Pll1IntegerHigh = 0x006c,

    /// Low byte of [`Reg::Pll1IntegerHigh`]'s value. 0x6D.
    Pll1IntegerLow = 0x006d,

    /// With a programmed APLL1 denominator, the 24-bit numerator's bits 15:8 (the 40-bit
    /// numerator's bits 39:32 otherwise). `PLL1_NUM`, 0x6E (R110; 0x6E:0x72).
    Pll1NumeratorMiddle = 0x006e,

    /// With a programmed denominator, the numerator's bits 7:0. 0x6F (R111).
    Pll1NumeratorLow = 0x006f,

    /// With a programmed denominator, the 24-bit denominator's bits 23:16. 0x70 (R112).
    Pll1DenominatorHigh = 0x0070,

    /// With a programmed denominator, the denominator's bits 15:8. 0x71 (R113).
    Pll1DenominatorMiddle = 0x0071,

    /// With a programmed denominator, the denominator's bits 7:0. 0x72 (R114).
    Pll1DenominatorLow = 0x0072,

    /// See [`Pll1Modulator`](super::apll::Pll1Modulator). `PLL1_MASHCTRL`, 0x73.
    Pll1Modulator = 0x0073,

    /// See [`Pll1Mode`](super::apll::Pll1Mode). `PLL1_MODE`, 0x74.
    Pll1Mode = 0x0074,

    /// APLL2's feedback divider integer part, high byte. `PLL2_NDIV`, 0x86 (0x86:0x87).
    Pll2IntegerHigh = 0x0086,

    /// Low byte of [`Reg::Pll2IntegerHigh`]'s value. 0x87.
    Pll2IntegerLow = 0x0087,

    /// APLL2's 24-bit feedback numerator, bits 23:16. `PLL2_NUM`, 0x88 (0x88:0x8A).
    Pll2NumeratorHigh = 0x0088,

    /// Bits 15:8 of [`Reg::Pll2NumeratorHigh`]'s value. 0x89.
    Pll2NumeratorMiddle = 0x0089,

    /// Bits 7:0 of [`Reg::Pll2NumeratorHigh`]'s value. 0x8A.
    Pll2NumeratorLow = 0x008a,

    /// See [`Pll2Modulator`](super::apll::Pll2Modulator). `PLL2_MASHCTRL`, 0x8B.
    Pll2Modulator = 0x008b,

    /// See [`DpllControl`](super::control::DpllControl). `DPLL_GEN_CTL`, 0xFC.
    DpllControl = 0x00fc,

    /// APLL2's programmed 24-bit feedback denominator, bits 23:16. `PLL2_DEN`, 0x14D
    /// (R333; 0x14D:0x14F).
    Pll2DenominatorHigh = 0x014d,

    /// Bits 15:8 of [`Reg::Pll2DenominatorHigh`]'s value. 0x14E.
    Pll2DenominatorMiddle = 0x014e,

    /// Bits 7:0 of [`Reg::Pll2DenominatorHigh`]'s value. 0x14F.
    Pll2DenominatorLow = 0x014f,

    /// With a programmed APLL1 denominator, the 24-bit numerator's bits 23:16.
    /// `PLL1_24B_NUM_23_16`, 0x153 (R339).
    Pll1NumeratorHigh = 0x0153,

    /// See [`VcoStatus`](super::apll::VcoStatus), APLL1. `PLL1_CALSTAT1`, 0x165.
    Pll1Status = 0x0165,

    /// See [`VcoStatus`](super::apll::VcoStatus), APLL2. `PLL2_CALSTAT1`, 0x16F.
    Pll2Status = 0x016f,
}

impl From<Reg> for u16 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

impl Reg {
    /// The highest address libusdr's staging map holds (`LMK05318_REGADDR_MAX`, 0x19B).
    pub(super) const LAST_STAGED: u16 = 0x019b;
}

/// A byte register at one fixed address; the 16-bit counterpart of `hal`'s
/// `Register`.
pub trait Lmk05318Register: Copy + From<u8> + Into<u8> {
    /// Where this register lives.
    const ADDR: Reg;
}

/// A byte register the chip repeats; the 16-bit counterpart of `hal`'s
/// `IndexedRegister`.
pub trait IndexedLmk05318Register: Copy + From<u8> + Into<u8> {
    /// What selects one copy.
    type Index: Copy;
    /// Where the copy for `index` lives.
    fn addr(index: Self::Index) -> Reg;
}
