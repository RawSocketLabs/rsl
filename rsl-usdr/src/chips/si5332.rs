//! Si5332 clock generator (source: `hw/si5332/si5332.c`).

use std::time::Duration;

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// Supply status register (read for diagnostics only).
const VDD_OK: u8 = 0x05;
/// System control: [`READY`] holds the outputs while the device is programmed.
const USYS_CTRL: u8 = 0x06;
/// System status.
const USYS_STAT: u8 = 0x07;
/// First of the nine part-number and design-ID registers.
const DEVICE_PN_BASE: u8 = 0x0d;
/// Last of the nine part-number and design-ID registers.
const DESIGN_ID2: u8 = 0x15;
/// Input mux select.
const IMUX_SEL: u8 = 0x24;
/// Input buffer 2 mode.
const CLKIN_2_CLK_SEL: u8 = 0x73;
/// Input buffer 3 mode.
const CLKIN_3_CLK_SEL: u8 = 0x74;
/// Output-mux selects for outputs 0..=5 (`OMUXn_SEL10`).
const OMUX_SEL10: [u8; 6] = [0x25, 0x26, 0x27, 0x28, 0x29, 0x2a];
/// Output 3 mode: the external RX mixer LO.
const OUT3_MODE: u8 = 0x98;
/// Output 3 divider.
const OUT3_DIV: u8 = 0x99;
/// Output 4 mode.
const OUT4_MODE: u8 = 0xa7;
/// Output 4 divider.
const OUT4_DIV: u8 = 0xa8;
/// Output enables for outputs 0..=3.
const OUT3210_OE: u8 = 0xb6;
/// Output enables for outputs 4 and 5.
const OUT54_OE: u8 = 0xb7;
/// Block power-downs: crystal oscillator and input buffers (`B9_*`).
const PDN_INPUTS: u8 = 0xb9;
/// Block power-downs: dividers (`BA_*`).
const PDN_DIVIDERS: u8 = 0xba;
/// Block power-downs: output muxes (`BB_*`).
const PDN_OMUX: u8 = 0xbb;
/// Block power-downs: outputs 0..=3.
const PDN_OUT3210: u8 = 0xbc;
/// Block power-downs: outputs 4 and 5 (`BD_*`).
const PDN_OUT54: u8 = 0xbd;
/// Crystal internal load-capacitor enable.
const XOSC_CINT_ENA: u8 = 0xbf;
/// Crystal XIN trim.
const XOSC_CTRIM_XIN: u8 = 0xc0;
/// Crystal XOUT trim.
const XOSC_CTRIM_XOUT: u8 = 0xc1;

/// System state: outputs held, registers writable.
const READY: u8 = 0x01;
/// System state: running.
const ACTIVE: u8 = 0x02;
/// System status: no input clock, cannot reach ACTIVE.
const NO_INPUT_CLOCK: u8 = 0x89;
/// Status polls before giving up, 10 µs apart (`si5532_get_state`).
const STATE_POLLS: usize = 100;

/// Output driver off.
const OUTMODE_OFF: u8 = 0;
/// Output driver: CMOS on the positive pin only.
const OUTMODE_CMOS_P: u8 = 1;
/// Output driver: LVPECL.
const OUTMODE_LVPECL: u8 = 12;
/// Output driver slew setting libusdr uses for every enabled output.
const CMOS_SLEW: u8 = 3;
/// Output mux: select 0 = PLL reference before the pre-scaler (0), select 1 =
/// `omux1_sel0` (7 << 4) (`MAKE_OUMUXX`).
const OMUX_PLL_REF_THEN_OMUXS0: u8 = 7 << 4;

/// `B9_XOSC_DIS`: power down the crystal oscillator.
const B9_XOSC_DIS: u8 = 1 << 0;
/// `B9_IBUF0_DIS`: power down input buffer 0.
const B9_IBUF0_DIS: u8 = 1 << 1;
/// `BA_HSDIV1_DIS`.
const BA_HSDIV1_DIS: u8 = 1 << 1;
/// `BA_HSDIV2_DIS`.
const BA_HSDIV2_DIS: u8 = 1 << 2;
/// `BA_HSDIV4_DIS`.
const BA_HSDIV4_DIS: u8 = 1 << 4;
/// `BA_ID0_DIS`.
const BA_ID0_DIS: u8 = 1 << 5;
/// `BA_ID1_DIS`.
const BA_ID1_DIS: u8 = 1 << 6;
/// `BB_OMUX4_DIS`.
const BB_OMUX4_DIS: u8 = 1 << 4;
/// `BB_OMUX5_DIS`.
const BB_OMUX5_DIS: u8 = 1 << 5;
/// `BD_OUT4_DIS`.
const BD_OUT4_DIS: u8 = 1 << 1;
/// `BD_OUT5_DIS`.
const BD_OUT5_DIS: u8 = 1 << 2;

/// Where the PLL reference comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    /// The crystal/oscillator input (`IMUX_XOSC`).
    Oscillator,
    /// Input buffer 2, differential (`IMUX_IN_2`).
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

/// A Si5332 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Si5332 {
    /// Where the clock generator answers.
    dev: I2cAddr,
}

impl Si5332 {
    /// The clock generator at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
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
        let mut id = [0; (DESIGN_ID2 - DEVICE_PN_BASE + 1) as usize];
        for (reg, byte) in (DEVICE_PN_BASE..=DESIGN_ID2).zip(&mut id) {
            *byte = self.read(bus, reg)?;
        }
        self.read(bus, VDD_OK)?;
        if id[..6].iter().all(|&byte| byte == 0xff) {
            return Err(Error::ChipMissing("Si5332"));
        }

        self.wait_state(bus, false)?;
        for (reg, value) in Self::power_up_plan(div, reference, lvpecl) {
            self.write(bus, reg, value)?;
        }
        // libusdr reads FPGA register 0xC here and ignores the result; kept so traces line up.
        let _ = bus.read_regs(0xc, &mut [0]);
        self.wait_state(bus, true)
    }

    /// The register writes of `si5332_init`, in order.
    fn power_up_plan(div: u8, reference: Reference, lvpecl: LvpeclOutput) -> [(u8, u8); 49] {
        let (imux, in2_mode, inputs_off) = match reference {
            Reference::Oscillator => (1, 0, B9_IBUF0_DIS),
            Reference::Input2 => (2, 1, B9_XOSC_DIS),
        };
        let (out0_mode, out1_mode) = match lvpecl {
            LvpeclOutput::Out0 => (OUTMODE_LVPECL, OUTMODE_CMOS_P),
            LvpeclOutput::Out1 => (OUTMODE_CMOS_P, OUTMODE_LVPECL),
        };
        let [omux0, omux1, omux2, omux3, omux4, omux5] = OMUX_SEL10;
        #[rustfmt::skip]
        let plan = [
            (USYS_CTRL, READY),
            (IMUX_SEL, imux),
            (CLKIN_2_CLK_SEL, in2_mode),
            (CLKIN_3_CLK_SEL, 0),
            // Undocumented registers libusdr clears.
            (0x3c, 0), (0x48, 0), (0x54, 0), (0x60, 0),
            (omux0, OMUX_PLL_REF_THEN_OMUXS0), (omux1, OMUX_PLL_REF_THEN_OMUXS0),
            (omux2, OMUX_PLL_REF_THEN_OMUXS0), (omux3, OMUX_PLL_REF_THEN_OMUXS0),
            (omux4, OMUX_PLL_REF_THEN_OMUXS0), (omux5, OMUX_PLL_REF_THEN_OMUXS0),
            // OUTn_MODE, OUTn_DIV, OUTn_SKEW, OUTn_CMOS_INV_Z, OUTn_CMOS_SLEW.
            (0x7a, out0_mode), (0x7b, div), (0x7c, 0), (0x7d, 0), (0x7e, CMOS_SLEW),
            (0x7f, out1_mode), (0x80, 1), (0x81, 0), (0x82, 0), (0x83, CMOS_SLEW),
            (0x89, OUTMODE_CMOS_P), (0x8a, 1), (0x8b, 0), (0x8c, 0), (0x8d, CMOS_SLEW),
            (OUT3_DIV, 0),
            (OUT4_DIV, 0),
            (0xac, OUTMODE_OFF), (0xad, 1), (0xae, 0), (0xaf, 0), (0xb0, CMOS_SLEW),
            (OUT3_MODE, OUTMODE_OFF),
            (OUT4_MODE, OUTMODE_OFF),
            (OUT3210_OE, 0xff),
            (OUT54_OE, 0xff),
            (PDN_INPUTS, inputs_off),
            (PDN_DIVIDERS, BA_HSDIV1_DIS | BA_HSDIV2_DIS | BA_HSDIV4_DIS | BA_ID0_DIS | BA_ID1_DIS),
            (PDN_OMUX, BB_OMUX4_DIS | BB_OMUX5_DIS),
            (PDN_OUT3210, 0),
            (PDN_OUT54, BD_OUT4_DIS | BD_OUT5_DIS),
            (XOSC_CINT_ENA, 0),
            (XOSC_CTRIM_XIN, 0),
            (XOSC_CTRIM_XOUT, 0),
            (USYS_CTRL, ACTIVE),
        ];
        plan
    }

    /// Polls the system status until it settles (`si5532_get_state`).
    ///
    /// Like libusdr, a status that never settles is not an error, and a missing input
    /// clock is one only once programming is done (`after_init`).
    fn wait_state(self, bus: &mut dyn Bus, after_init: bool) -> Result<(), Error> {
        let mut state = 0;
        for _ in 0..STATE_POLLS {
            bus.sleep(Duration::from_micros(10));
            state = self.read(bus, USYS_STAT)?;
            if matches!(state, READY | ACTIVE | NO_INPUT_CLOCK) {
                break;
            }
        }
        match state {
            NO_INPUT_CLOCK if after_init => Err(Error::ClockInputMissing),
            _ => Ok(()),
        }
    }

    /// Writes one register.
    fn write(self, bus: &mut dyn Bus, reg: u8, value: u8) -> Result<(), Error> {
        bus.i2c(self.dev, &[reg, value], &mut [])
            .during("Si5332 write")
    }

    /// Reads one register.
    fn read(self, bus: &mut dyn Bus, reg: u8) -> Result<u8, Error> {
        let mut value = [0];
        bus.i2c(self.dev, &[reg], &mut value)
            .during("Si5332 read")?;
        Ok(value[0])
    }
}
