//! The register address map.
//!
//! The Si5332 has one flat 8-bit address space, so one `Reg` names every address the driver
//! touches. The value types live in the module for their stage, or in [`state`](super::state)
//! for the state machine.

use bnb::BitEnum;

/// Register addresses, with libusdr's C names (`si5332.c`) in the docs.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Supply status flags (`VDDO_OK`, `VDD_XTAL_OK`), read for diagnostics only. `VDD_OK`,
    /// 0x05.
    SupplyStatus = 0x05,

    /// See [`RequestedState`](super::state::RequestedState). `USYS_CTRL`, 0x06.
    RequestedState = 0x06,

    /// See [`CurrentState`](super::state::CurrentState). `USYS_STAT`, 0x07.
    CurrentState = 0x07,

    /// Device part number. `DEVICE_PN_BASE`, 0x0D.
    DevicePn = 0x0d,

    /// Device revision. `DEVICE_REV`, 0x0E.
    DeviceRev = 0x0e,

    /// Device grade. `DEVICE_GRADE`, 0x0F.
    DeviceGrade = 0x0f,

    /// Orderable part number, characters 1 and 0. `FACTORY_OPN_ID10`, 0x10.
    FactoryOpnId10 = 0x10,

    /// Orderable part number, characters 3 and 2. `FACTORY_OPN_ID32`, 0x11.
    FactoryOpnId32 = 0x11,

    /// Orderable part number, revision and character 4. `FACTORY_OPN_IDR4`, 0x12.
    FactoryOpnIdR4 = 0x12,

    /// Design ID, byte 0. `DESIGN_ID0`, 0x13.
    DesignId0 = 0x13,

    /// Design ID, byte 1. `DESIGN_ID1`, 0x14.
    DesignId1 = 0x14,

    /// Design ID, byte 2. `DESIGN_ID2`, 0x15.
    DesignId2 = 0x15,

    /// See [`PllReference`](super::input::PllReference). `IMUX_SEL`, 0x24.
    PllReference = 0x24,

    /// See [`OutputSource`](super::output::OutputSource), output 0. `OMUX0_SEL10`, 0x25.
    Output0Source = 0x25,

    /// Output 1 source. `OMUX1_SEL10`, 0x26.
    Output1Source = 0x26,

    /// Output 2 source. `OMUX2_SEL10`, 0x27.
    Output2Source = 0x27,

    /// Output 3 source. `OMUX3_SEL10`, 0x28.
    Output3Source = 0x28,

    /// Output 4 source. `OMUX4_SEL10`, 0x29.
    Output4Source = 0x29,

    /// Output 5 source. `OMUX5_SEL10`, 0x2A.
    Output5Source = 0x2a,

    /// See [`SpreadSpectrum`](super::divider::SpreadSpectrum), divider 0 bank A. `ID0A_SS`, 0x3C.
    Id0aSpreadSpectrum = 0x3c,

    /// Divider 0 bank B spread spectrum. `ID0B_SS`, 0x48.
    Id0bSpreadSpectrum = 0x48,

    /// Divider 1 bank A spread spectrum. `ID1A_SS`, 0x54.
    Id1aSpreadSpectrum = 0x54,

    /// Divider 1 bank B spread spectrum. `ID1B_SS`, 0x60.
    Id1bSpreadSpectrum = 0x60,

    /// See [`InputMode`](super::input::InputMode). `CLKIN_2_CLK_SEL`, 0x73.
    Input2Mode = 0x73,

    /// Input buffer 3's mode; libusdr's map has no layout, and it writes 0.
    /// `CLKIN_3_CLK_SEL`, 0x74.
    Input3Mode = 0x74,

    /// See [`DriverMode`](super::output::DriverMode), output 0. `OUT0_MODE`, 0x7A.
    Output0Mode = 0x7a,

    /// See [`Divider`](super::output::Divider), output 0. `OUT0_DIV`, 0x7B.
    Output0Divider = 0x7b,

    /// See [`Skew`](super::output::Skew), output 0. `OUT0_SKEW`, 0x7C.
    Output0Skew = 0x7c,

    /// See [`Polarity`](super::output::Polarity), output 0. `OUT0_CMOS_INV_Z`, 0x7D.
    Output0Polarity = 0x7d,

    /// See [`CmosDrive`](super::output::CmosDrive), output 0. `OUT0_CMOS_SLEW`, 0x7E.
    Output0Drive = 0x7e,

    /// Output 1 driver mode. `OUT1_MODE`, 0x7F.
    Output1Mode = 0x7f,

    /// Output 1 divider. `OUT1_DIV`, 0x80.
    Output1Divider = 0x80,

    /// Output 1 skew. `OUT1_SKEW`, 0x81.
    Output1Skew = 0x81,

    /// Output 1 polarity. `OUT1_CMOS_INV_Z`, 0x82.
    Output1Polarity = 0x82,

    /// Output 1 CMOS drive. `OUT1_CMOS_SLEW`, 0x83.
    Output1Drive = 0x83,

    /// Output 2 driver mode. `OUT2_MODE`, 0x89.
    Output2Mode = 0x89,

    /// Output 2 divider. `OUT2_DIV`, 0x8A.
    Output2Divider = 0x8a,

    /// Output 2 skew. `OUT2_SKEW`, 0x8B.
    Output2Skew = 0x8b,

    /// Output 2 polarity. `OUT2_CMOS_INV_Z`, 0x8C.
    Output2Polarity = 0x8c,

    /// Output 2 CMOS drive. `OUT2_CMOS_SLEW`, 0x8D.
    Output2Drive = 0x8d,

    /// Output 3 driver mode. `OUT3_MODE`, 0x98.
    Output3Mode = 0x98,

    /// Output 3 divider. `OUT3_DIV`, 0x99.
    Output3Divider = 0x99,

    /// Output 3 skew. `OUT3_SKEW`, 0x9A.
    Output3Skew = 0x9a,

    /// Output 3 polarity. `OUT3_CMOS_INV_Z`, 0x9B.
    Output3Polarity = 0x9b,

    /// Output 3 CMOS drive. `OUT3_CMOS_SLEW`, 0x9C.
    Output3Drive = 0x9c,

    /// Output 4 driver mode. `OUT4_MODE`, 0xA7.
    Output4Mode = 0xa7,

    /// Output 4 divider. `OUT4_DIV`, 0xA8.
    Output4Divider = 0xa8,

    /// Output 4 skew. `OUT4_SKEW`, 0xA9.
    Output4Skew = 0xa9,

    /// Output 4 polarity. `OUT4_CMOS_INV_Z`, 0xAA.
    Output4Polarity = 0xaa,

    /// Output 4 CMOS drive. `OUT4_CMOS_SLEW`, 0xAB.
    Output4Drive = 0xab,

    /// Output 5 driver mode. `OUT5_MODE`, 0xAC.
    Output5Mode = 0xac,

    /// Output 5 divider. `OUT5_DIV`, 0xAD.
    Output5Divider = 0xad,

    /// Output 5 skew. `OUT5_SKEW`, 0xAE.
    Output5Skew = 0xae,

    /// Output 5 polarity. `OUT5_CMOS_INV_Z`, 0xAF.
    Output5Polarity = 0xaf,

    /// Output 5 CMOS drive. `OUT5_CMOS_SLEW`, 0xB0.
    Output5Drive = 0xb0,

    /// Output enables for outputs 0..=3 (bits 0, 1, 3, 6). libusdr writes 0xFF, which also
    /// sets the undefined bits, so it stays a raw byte. `OUT3210_OE`, 0xB6.
    OutputEnables0to3 = 0xb6,

    /// Output enables for outputs 4 and 5 (bits 1, 2); written as 0xFF like
    /// [`Reg::OutputEnables0to3`]. `OUT54_OE`, 0xB7.
    OutputEnables4and5 = 0xb7,

    /// See [`InputPowerDown`](super::input::InputPowerDown). 0xB9.
    InputPowerDown = 0xb9,

    /// See [`DividerPowerDown`](super::divider::DividerPowerDown). 0xBA.
    DividerPowerDown = 0xba,

    /// See [`SourcePowerDown`](super::output::SourcePowerDown). 0xBB.
    SourcePowerDown = 0xbb,

    /// See [`Output0to3PowerDown`](super::output::Output0to3PowerDown). 0xBC.
    Output0to3PowerDown = 0xbc,

    /// See [`Output4and5PowerDown`](super::output::Output4and5PowerDown). 0xBD.
    Output4and5PowerDown = 0xbd,

    /// See [`CrystalLoad`](super::input::CrystalLoad). `XOSC_CINT_ENA`, 0xBF.
    CrystalLoad = 0xbf,

    /// See [`CrystalTrim`](super::input::CrystalTrim), pin XA. `XOSC_CTRIM_XA`, 0xC0.
    CrystalTrimXa = 0xc0,

    /// Crystal trim, pin XB. `XOSC_CTRIM_XB`, 0xC1.
    CrystalTrimXb = 0xc1,
}

/// The nine identification registers, in address order.
pub(super) const ID_REGS: [Reg; 9] = [
    Reg::DevicePn,
    Reg::DeviceRev,
    Reg::DeviceGrade,
    Reg::FactoryOpnId10,
    Reg::FactoryOpnId32,
    Reg::FactoryOpnIdR4,
    Reg::DesignId0,
    Reg::DesignId1,
    Reg::DesignId2,
];
