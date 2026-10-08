//! Si5332 clock generator (source: `hw/si5332/si5332.c`, layouts from the generated
//! `def_si5332.h` and the enums in `si5332.c`).

use std::time::Duration;

use bnb::{BitEnum, bitfield, u2, u3};

use super::register::{I2cRegisters, IndexedRegister, Register, bitfield_register, enum_register};
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
enum Reg {
    /// Supply status (read for diagnostics only).
    VddOk = 0x05,
    /// System control; see [`SystemControl`].
    UsysCtrl = 0x06,
    /// System status; see [`SystemStatus`].
    UsysStat = 0x07,
    /// Device part number (`DEVICE_PN_BASE`).
    DevicePn = 0x0d,
    /// Device revision.
    DeviceRev = 0x0e,
    /// Device grade.
    DeviceGrade = 0x0f,
    /// Factory OPN ID, characters 1 and 0.
    FactoryOpnId10 = 0x10,
    /// Factory OPN ID, characters 3 and 2.
    FactoryOpnId32 = 0x11,
    /// Factory OPN ID, revision and character 4.
    FactoryOpnIdR4 = 0x12,
    /// Design ID, byte 0.
    DesignId0 = 0x13,
    /// Design ID, byte 1.
    DesignId1 = 0x14,
    /// Design ID, byte 2.
    DesignId2 = 0x15,
    /// PLL input mux; see [`ReferenceSelect`].
    ImuxSel = 0x24,
    /// Output 0 mux; see [`OutputMux`].
    Omux0Sel10 = 0x25,
    /// Output 1 mux.
    Omux1Sel10 = 0x26,
    /// Output 2 mux.
    Omux2Sel10 = 0x27,
    /// Output 3 mux.
    Omux3Sel10 = 0x28,
    /// Output 4 mux.
    Omux4Sel10 = 0x29,
    /// Output 5 mux.
    Omux5Sel10 = 0x2a,
    /// Integer divider 0A spread spectrum (`SS_MODE`, `SS_ENA`); libusdr disables it.
    Id0aSs = 0x3c,
    /// Integer divider 0B spread spectrum; libusdr disables it.
    Id0bSs = 0x48,
    /// Integer divider 1A spread spectrum; libusdr disables it.
    Id1aSs = 0x54,
    /// Integer divider 1B spread spectrum; libusdr disables it.
    Id1bSs = 0x60,
    /// Input buffer 2 mode; see [`InputBufferMode`].
    Clkin2ClkSel = 0x73,
    /// Input buffer 3 mode (no layout in libusdr; written as 0).
    Clkin3ClkSel = 0x74,
    /// Output 0 driver mode; see [`OutputMode`].
    Out0Mode = 0x7a,
    /// Output 0 divider.
    Out0Div = 0x7b,
    /// Output 0 skew.
    Out0Skew = 0x7c,
    /// Output 0 CMOS inversion.
    Out0CmosInvZ = 0x7d,
    /// Output 0 CMOS slew.
    Out0CmosSlew = 0x7e,
    /// Output 1 driver mode.
    Out1Mode = 0x7f,
    /// Output 1 divider.
    Out1Div = 0x80,
    /// Output 1 skew.
    Out1Skew = 0x81,
    /// Output 1 CMOS inversion.
    Out1CmosInvZ = 0x82,
    /// Output 1 CMOS slew.
    Out1CmosSlew = 0x83,
    /// Output 2 driver mode.
    Out2Mode = 0x89,
    /// Output 2 divider.
    Out2Div = 0x8a,
    /// Output 2 skew.
    Out2Skew = 0x8b,
    /// Output 2 CMOS inversion.
    Out2CmosInvZ = 0x8c,
    /// Output 2 CMOS slew.
    Out2CmosSlew = 0x8d,
    /// Output 3 driver mode: the external RX mixer LO.
    Out3Mode = 0x98,
    /// Output 3 divider.
    Out3Div = 0x99,
    /// Output 4 driver mode.
    Out4Mode = 0xa7,
    /// Output 4 divider.
    Out4Div = 0xa8,
    /// Output 5 driver mode.
    Out5Mode = 0xac,
    /// Output 5 divider.
    Out5Div = 0xad,
    /// Output 5 skew.
    Out5Skew = 0xae,
    /// Output 5 CMOS inversion.
    Out5CmosInvZ = 0xaf,
    /// Output 5 CMOS slew.
    Out5CmosSlew = 0xb0,
    /// Output enables for outputs 0..=3.
    Out3210Oe = 0xb6,
    /// Output enables for outputs 4 and 5.
    Out54Oe = 0xb7,
    /// Input power-downs; see [`InputPowerDown`].
    PdnInputs = 0xb9,
    /// Divider power-downs; see [`DividerPowerDown`].
    PdnDividers = 0xba,
    /// Output-mux power-downs; see [`OutputMuxPowerDown`].
    PdnOmux = 0xbb,
    /// Output 0..=3 power-downs; see [`Output3210PowerDown`].
    PdnOut3210 = 0xbc,
    /// Output 4 and 5 power-downs; see [`Output54PowerDown`].
    PdnOut54 = 0xbd,
    /// Crystal internal load-capacitor enable.
    XoscCintEna = 0xbf,
    /// Crystal XIN trim.
    XoscCtrimXin = 0xc0,
    /// Crystal XOUT trim.
    XoscCtrimXout = 0xc1,
}

/// The nine identification registers, `DEVICE_PN_BASE..=DESIGN_ID2`, in order.
const ID_REGS: [Reg; 9] = [
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

/// The requested system state.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum SystemControl {
    /// Outputs held, registers writable.
    Ready = 0x01,
    /// Running.
    Active = 0x02,
    /// Any other value.
    #[catch_all]
    Other(u8),
}
enum_register!(SystemControl => Reg::UsysCtrl);

/// The reported system state.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum SystemStatus {
    /// Outputs held, registers writable.
    Ready = 0x01,
    /// Running.
    Active = 0x02,
    /// No input clock; cannot reach ACTIVE.
    NoInputClock = 0x89,
    /// Any other value, including while the state changes.
    #[catch_all]
    Other(u8),
}
enum_register!(SystemStatus => Reg::UsysStat);

/// The PLL reference input (`IMUX_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum ReferenceSelect {
    /// The crystal/oscillator input.
    Oscillator = 1,
    /// Input buffer 2.
    Input2 = 2,
    /// Any other value.
    #[catch_all]
    Other(u8),
}
enum_register!(ReferenceSelect => Reg::ImuxSel);

/// Input buffer 2's mode (`IMUX_INX_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum InputBufferMode {
    /// Buffer off.
    Disabled = 0,
    /// Differential input.
    Differential = 1,
    /// Any other value.
    #[catch_all]
    Other(u8),
}

enum_register!(InputBufferMode => Reg::Clkin2ClkSel);

/// The clock outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Output {
    /// Output 0.
    Out0,
    /// Output 1.
    Out1,
    /// Output 2.
    Out2,
    /// Output 3: the external RX mixer LO.
    Out3,
    /// Output 4.
    Out4,
    /// Output 5.
    Out5,
}

/// Output-mux select 0 (`OUMUXX_SEL0_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum MuxSel0 {
    /// PLL reference clock before the pre-scaler.
    PllRef,
    /// PLL reference clock after the pre-scaler.
    PllRefPrescaled,
    /// Input buffer 2.
    Input2,
    /// Input buffer 3.
    Input3,
}

/// Output-mux select 1 (`OUMUXX_SEL1_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
enum MuxSel1 {
    /// High-speed divider 0.
    HsDiv0,
    /// High-speed divider 1.
    HsDiv1,
    /// High-speed divider 2.
    HsDiv2,
    /// High-speed divider 3.
    HsDiv3,
    /// High-speed divider 4.
    HsDiv4,
    /// Integer divider 0.
    Id0,
    /// Integer divider 1.
    Id1,
    /// Select 0's choice (`omux1_sel0`).
    Sel0,
}

/// An output's mux (`OMUXn_SEL10`): which source feeds it.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OutputMux {
    /// Select 1.
    #[bits(4..=6)]
    sel1: MuxSel1,
    /// Select 0.
    #[bits(0..=1)]
    sel0: MuxSel0,
}

impl IndexedRegister for OutputMux {
    type Map = Reg;
    type Index = Output;

    fn addr(output: Output) -> Reg {
        match output {
            Output::Out0 => Reg::Omux0Sel10,
            Output::Out1 => Reg::Omux1Sel10,
            Output::Out2 => Reg::Omux2Sel10,
            Output::Out3 => Reg::Omux3Sel10,
            Output::Out4 => Reg::Omux4Sel10,
            Output::Out5 => Reg::Omux5Sel10,
        }
    }

    fn to_byte(self) -> u8 {
        self.to_raw()
    }
}

/// An output's driver mode (`OUTMODE_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum OutputMode {
    /// Off.
    Off = 0,
    /// CMOS on the positive pin only.
    CmosP = 1,
    /// CMOS on the negative pin only.
    CmosN = 2,
    /// Dual CMOS.
    CmosDual = 3,
    /// 2.5 V/3.3 V LVDS.
    Lvds25 = 4,
    /// 1.8 V LVDS.
    Lvds18 = 5,
    /// 2.5 V/3.3 V LVDS, fast.
    Lvds25Fast = 6,
    /// 1.8 V LVDS, fast.
    Lvds18Fast = 7,
    /// HCSL 50 Ω, external termination.
    Hcsl50External = 8,
    /// HCSL 50 Ω, internal termination.
    Hcsl50Internal = 9,
    /// HCSL 42.5 Ω, external termination.
    Hcsl42External = 10,
    /// HCSL 42.5 Ω, internal termination.
    Hcsl42Internal = 11,
    /// LVPECL.
    Lvpecl = 12,
    /// Any other value.
    #[catch_all]
    Other(u8),
}

impl IndexedRegister for OutputMode {
    type Map = Reg;
    type Index = Output;

    fn addr(output: Output) -> Reg {
        match output {
            Output::Out0 => Reg::Out0Mode,
            Output::Out1 => Reg::Out1Mode,
            Output::Out2 => Reg::Out2Mode,
            Output::Out3 => Reg::Out3Mode,
            Output::Out4 => Reg::Out4Mode,
            Output::Out5 => Reg::Out5Mode,
        }
    }

    fn to_byte(self) -> u8 {
        self.into()
    }
}

/// Input-side power-downs (`B9_*`).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InputPowerDown {
    /// PLL.
    #[bits(5..=5)]
    pll: bool,
    /// Pre-divider.
    #[bits(4..=4)]
    pdiv: bool,
    /// Input mux.
    #[bits(3..=3)]
    imux: bool,
    /// Input buffer 0.
    #[bits(1..=1)]
    ibuf0: bool,
    /// Crystal oscillator.
    #[bits(0..=0)]
    xosc: bool,
}
bitfield_register!(InputPowerDown => Reg::PdnInputs);

/// Divider power-downs (`BA_*`).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DividerPowerDown {
    /// Integer divider 1.
    #[bits(6..=6)]
    id1: bool,
    /// Integer divider 0.
    #[bits(5..=5)]
    id0: bool,
    /// High-speed divider 4.
    #[bits(4..=4)]
    hsdiv4: bool,
    /// High-speed divider 3.
    #[bits(3..=3)]
    hsdiv3: bool,
    /// High-speed divider 2.
    #[bits(2..=2)]
    hsdiv2: bool,
    /// High-speed divider 1.
    #[bits(1..=1)]
    hsdiv1: bool,
    /// High-speed divider 0.
    #[bits(0..=0)]
    hsdiv0: bool,
}
bitfield_register!(DividerPowerDown => Reg::PdnDividers);

/// Output-mux power-downs (`BB_*`).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OutputMuxPowerDown {
    /// Output mux 5.
    #[bits(5..=5)]
    omux5: bool,
    /// Output mux 4.
    #[bits(4..=4)]
    omux4: bool,
    /// Output mux 3.
    #[bits(3..=3)]
    omux3: bool,
    /// Output mux 2.
    #[bits(2..=2)]
    omux2: bool,
    /// Output mux 1.
    #[bits(1..=1)]
    omux1: bool,
    /// Output mux 0.
    #[bits(0..=0)]
    omux0: bool,
}
bitfield_register!(OutputMuxPowerDown => Reg::PdnOmux);

/// Output 0..=3 power-downs (`BC_*`).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Output3210PowerDown {
    /// Output 3.
    #[bits(6..=6)]
    out3: bool,
    /// Output 2.
    #[bits(3..=3)]
    out2: bool,
    /// Output 1.
    #[bits(1..=1)]
    out1: bool,
    /// Output 0.
    #[bits(0..=0)]
    out0: bool,
}
bitfield_register!(Output3210PowerDown => Reg::PdnOut3210);

/// Output 4 and 5 power-downs (`BD_*`).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Output54PowerDown {
    /// Output 5.
    #[bits(2..=2)]
    out5: bool,
    /// Output 4.
    #[bits(1..=1)]
    out4: bool,
}
bitfield_register!(Output54PowerDown => Reg::PdnOut54);

/// Status polls before giving up, 10 µs apart (`si5532_get_state`).
const STATE_POLLS: usize = 100;
/// CMOS slew setting libusdr uses for every enabled output.
const CMOS_SLEW: u8 = 3;

/// Where the PLL reference comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    /// The crystal/oscillator input.
    Oscillator,
    /// Input buffer 2, differential.
    Input2,
}

/// Which of outputs 0 and 1 drives LVPECL; the other drives CMOS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LvpeclOutput {
    /// Output 0 is LVPECL.
    Out0,
    /// Output 1 is LVPECL.
    Out1,
}

/// One opaque or typed register write: address and byte.
type RegWrite = (Reg, u8);

/// A Si5332 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Si5332 {
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Si5332 {
    /// The clock generator at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "Si5332 read", "Si5332 write"),
        }
    }

    /// Identifies the chip and programs its power-up output plan (`si5332_init`), with
    /// output 0 divided by `div`.
    pub(crate) fn init(
        self,
        bus: &mut dyn Bus,
        div: u8,
        reference: Reference,
        lvpecl: LvpeclOutput,
    ) -> Result<(), Error> {
        let mut id = [0; ID_REGS.len()];
        for (reg, byte) in ID_REGS.into_iter().zip(&mut id) {
            *byte = self.regs.read_raw(bus, reg)?;
        }
        self.regs.read_raw(bus, Reg::VddOk)?;
        if id[..6].iter().all(|&byte| byte == 0xff) {
            return Err(Error::ChipMissing("Si5332"));
        }

        self.wait_state(bus, false)?;
        for (reg, value) in Self::power_up_plan(div, reference, lvpecl) {
            self.regs.write_raw(bus, reg, value)?;
        }
        // libusdr reads FPGA register 0xC here and ignores the result; kept so traces line up.
        let _ = bus.read_regs(0xc, &mut [0]);
        self.wait_state(bus, true)
    }

    /// The register writes of `si5332_init`, in order.
    fn power_up_plan(div: u8, reference: Reference, lvpecl: LvpeclOutput) -> [RegWrite; 49] {
        let (imux, in2_mode, inputs_off) = match reference {
            Reference::Oscillator => (
                ReferenceSelect::Oscillator,
                InputBufferMode::Disabled,
                InputPowerDown::new().with_ibuf0(true),
            ),
            Reference::Input2 => (
                ReferenceSelect::Input2,
                InputBufferMode::Differential,
                InputPowerDown::new().with_xosc(true),
            ),
        };
        let (out0_mode, out1_mode) = match lvpecl {
            LvpeclOutput::Out0 => (OutputMode::Lvpecl, OutputMode::CmosP),
            LvpeclOutput::Out1 => (OutputMode::CmosP, OutputMode::Lvpecl),
        };
        let pll_ref = OutputMux::new()
            .with_sel1(MuxSel1::Sel0)
            .with_sel0(MuxSel0::PllRef);
        let dividers_off = DividerPowerDown::new()
            .with_hsdiv1(true)
            .with_hsdiv2(true)
            .with_hsdiv4(true)
            .with_id0(true)
            .with_id1(true);

        #[rustfmt::skip]
        let plan = [
            SystemControl::Ready.entry(),
            imux.entry(),
            in2_mode.entry(),
            (Reg::Clkin3ClkSel, 0),
            (Reg::Id0aSs, 0),
            (Reg::Id0bSs, 0),
            (Reg::Id1aSs, 0),
            (Reg::Id1bSs, 0),
            pll_ref.entry_at(Output::Out0),
            pll_ref.entry_at(Output::Out1),
            pll_ref.entry_at(Output::Out2),
            pll_ref.entry_at(Output::Out3),
            pll_ref.entry_at(Output::Out4),
            pll_ref.entry_at(Output::Out5),
            out0_mode.entry_at(Output::Out0), (Reg::Out0Div, div), (Reg::Out0Skew, 0), (Reg::Out0CmosInvZ, 0), (Reg::Out0CmosSlew, CMOS_SLEW),
            out1_mode.entry_at(Output::Out1), (Reg::Out1Div, 1), (Reg::Out1Skew, 0), (Reg::Out1CmosInvZ, 0), (Reg::Out1CmosSlew, CMOS_SLEW),
            OutputMode::CmosP.entry_at(Output::Out2), (Reg::Out2Div, 1), (Reg::Out2Skew, 0), (Reg::Out2CmosInvZ, 0), (Reg::Out2CmosSlew, CMOS_SLEW),
            (Reg::Out3Div, 0),
            (Reg::Out4Div, 0),
            OutputMode::Off.entry_at(Output::Out5), (Reg::Out5Div, 1), (Reg::Out5Skew, 0), (Reg::Out5CmosInvZ, 0), (Reg::Out5CmosSlew, CMOS_SLEW),
            OutputMode::Off.entry_at(Output::Out3),
            OutputMode::Off.entry_at(Output::Out4),
            (Reg::Out3210Oe, 0xff),
            (Reg::Out54Oe, 0xff),
            inputs_off.entry(),
            dividers_off.entry(),
            OutputMuxPowerDown::new().with_omux4(true).with_omux5(true).entry(),
            Output3210PowerDown::new().entry(),
            Output54PowerDown::new().with_out4(true).with_out5(true).entry(),
            (Reg::XoscCintEna, 0),
            (Reg::XoscCtrimXin, 0),
            (Reg::XoscCtrimXout, 0),
            SystemControl::Active.entry(),
        ];
        plan
    }

    /// Polls the system status until it settles (`si5532_get_state`).
    ///
    /// Like libusdr, a status that never settles is not an error, and a missing input
    /// clock is one only once programming is done (`after_init`).
    fn wait_state(self, bus: &mut dyn Bus, after_init: bool) -> Result<(), Error> {
        let mut status = SystemStatus::Other(0);
        for _ in 0..STATE_POLLS {
            bus.sleep(Duration::from_micros(10));
            status = self.regs.read(bus)?;
            if !matches!(status, SystemStatus::Other(_)) {
                break;
            }
        }
        match status {
            SystemStatus::NoInputClock if after_init => Err(Error::ClockInputMissing),
            _ => Ok(()),
        }
    }
}
