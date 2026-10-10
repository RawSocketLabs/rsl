//! The RX and TX DSP chains' configuration ports (`REG_CFG_PHY_0` and `REG_CFG_PHY_1`).
//!
//! Each port is one FPGA register that takes sub-register writes: bits 30:24 pick the
//! sub-register (libusdr's `fpga_phy_regs`), bits 23:0 carry its value. Through it the
//! driver resets the chain, loads the decimator's FIR microcode, switches DC correction and
//! sets the two NCOs.
//!
//! Source: `REG_CFG_PHY_*` in `device/generic_usdr/generic_regs.h` and `fpga_phy_regs`,
//! `_usdr_set_nco` and `usdr_set_samplerate_ex` in `device/m2_lm6_1/usdr_ctrl.c`;
//! `fgearbox_load_ucode` in `ipblks/fgearbox.c`.

use std::time::Duration;

use bnb::{bitfield, u7, u24};

use super::fir_tables;
use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

/// A DSP chain's configuration port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phy {
    /// The RX chain (`REG_CFG_PHY_0`).
    Rx = 56,

    /// The TX chain (`REG_CFG_PHY_1`).
    Tx = 57,
}

/// A sub-register behind a [`Phy`] port (libusdr's `fpga_phy_regs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubReg {
    /// Chain reset and NCO reset controls; libusdr does not document the bits
    /// (`CFG_REG_RESET`).
    Reset = 0,

    /// DC correction enable (`CFG_REG_DCCTRL`).
    DcControl = 1,

    /// IQ imbalance correction, I amplitude (`CFG_REG_IQIMB_0`).
    IqAmplitudeI = 2,

    /// IQ imbalance correction, Q amplitude (`CFG_REG_IQIMB_1`).
    IqAmplitudeQ = 3,

    /// IQ imbalance correction, I phase term (`CFG_REG_IQIMB_2`).
    IqPhaseI = 4,

    /// IQ imbalance correction, Q phase term (`CFG_REG_IQIMB_3`).
    IqPhaseQ = 5,

    /// NCO 0 frequency word, low half (`CFG_REG_NCO0_L`).
    Nco0Low = 8,

    /// NCO 0 frequency word, high half (`CFG_REG_NCO0_H`).
    Nco0High = 9,

    /// NCO 1 frequency word, low half (`CFG_REG_NCO1_L`).
    Nco1Low = 10,

    /// NCO 1 frequency word, high half (`CFG_REG_NCO1_H`).
    Nco1High = 11,

    /// FIR microcode, one byte per write (`CFG_REG_DSP_LD`).
    FirLoad = 64,
}

/// One write to a [`Phy`] port (`MAKE_PHY_WR_REG`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhyWrite {
    /// Which sub-register.
    #[bits(24..=30)]
    sub: u7,

    /// The value written.
    #[bits(0..=23)]
    data: u24,
}

/// One of a chain's two numerically controlled oscillators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nco {
    /// NCO 0.
    Nco0,

    /// NCO 1.
    Nco1,
}

/// The decimator's rate-change factor, with the FIR microcode for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decimation {
    /// No decimation: the FIR passes samples through.
    X1,

    /// Decimate by 2.
    X2,

    /// Decimate by 4.
    X4,

    /// Decimate by 8.
    X8,

    /// Decimate by 16.
    X16,

    /// Decimate by 32.
    X32,
}

impl Decimation {
    /// Every factor, smallest first.
    pub(crate) const ALL: [Self; 6] =
        [Self::X1, Self::X2, Self::X4, Self::X8, Self::X16, Self::X32];

    /// The factor as a number.
    pub(crate) const fn factor(self) -> u32 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::X8 => 8,
            Self::X16 => 16,
            Self::X32 => 32,
        }
    }

    /// The RX FIR microcode libusdr loads for this factor on the uSDR's 7-series FPGA.
    fn rx_fir(self) -> &'static [u8; 1280] {
        match self {
            Self::X1 => &fir_tables::BYPASS,
            Self::X2 => &fir_tables::DECIMATE_2,
            Self::X4 => &fir_tables::DECIMATE_4,
            Self::X8 => &fir_tables::DECIMATE_8,
            Self::X16 => &fir_tables::DECIMATE_16,
            Self::X32 => &fir_tables::DECIMATE_32,
        }
    }
}

impl Phy {
    /// Writes the reset sub-register. libusdr writes fixed values (15 then 9 before a FIR
    /// load, 1 or 7 then 0 around a front-end reset) without documenting the bits.
    pub(crate) fn reset(self, bus: &mut dyn Bus, bits: u32) -> Result<(), Error> {
        self.write(bus, SubReg::Reset, bits)
    }

    /// Switches the chain's DC correction.
    pub(crate) fn set_dc_correction(self, bus: &mut dyn Bus, on: bool) -> Result<(), Error> {
        self.write(bus, SubReg::DcControl, u32::from(on))
    }

    /// The IQ correction with no imbalance: libusdr's `usdr_calc_iqimb` for its default
    /// settings, amplitudes of 0.5 × 8388607 and no phase terms.
    pub(crate) const NO_IQ_IMBALANCE: [i32; 4] = [4_194_303, 4_194_303, 0, 0];

    /// Writes the IQ imbalance correction: I and Q amplitudes, then I and Q phase terms
    /// (`usdr_rxupdate_cal`). libusdr keeps each term's low 24 bits.
    pub(crate) fn set_iq_correction(self, bus: &mut dyn Bus, terms: [i32; 4]) -> Result<(), Error> {
        let subs = [
            SubReg::IqAmplitudeI,
            SubReg::IqAmplitudeQ,
            SubReg::IqPhaseI,
            SubReg::IqPhaseQ,
        ];
        for (sub, term) in subs.into_iter().zip(terms) {
            let [b0, b1, b2, _] = term.to_le_bytes();
            self.write(bus, sub, u32::from_le_bytes([b0, b1, b2, 0]))?;
        }
        Ok(())
    }

    /// Sets an NCO's frequency word, then pulses the NCO reset (`_usdr_set_nco`).
    pub(crate) fn set_nco(self, bus: &mut dyn Bus, nco: Nco, word: i32) -> Result<(), Error> {
        let (low, high) = match nco {
            Nco::Nco0 => (SubReg::Nco0Low, SubReg::Nco0High),
            Nco::Nco1 => (SubReg::Nco1Low, SubReg::Nco1High),
        };
        let [b0, b1, b2, b3] = word.to_le_bytes();
        self.write(bus, low, u32::from(u16::from_le_bytes([b0, b1])))?;
        self.write(bus, high, u32::from(u16::from_le_bytes([b2, b3])))?;
        self.reset(bus, 1 << 8)?;
        self.reset(bus, 0)
    }

    /// Loads the RX decimator's FIR microcode for `decimation`, a byte per write, 1 µs
    /// apart (`fgearbox_load_fir_ex` with `sleep_us` 1).
    pub(crate) fn load_rx_fir(bus: &mut dyn Bus, decimation: Decimation) -> Result<(), Error> {
        for &byte in decimation.rx_fir() {
            Self::Rx.write(bus, SubReg::FirLoad, byte.into())?;
            bus.sleep(Duration::from_micros(1));
        }
        Ok(())
    }

    /// Writes `data` to a sub-register.
    fn write(self, bus: &mut dyn Bus, sub: SubReg, data: u32) -> Result<(), Error> {
        let word = PhyWrite::new()
            .with_sub(u7::new(sub as u8))
            .with_data(u24::new(data));
        let reg = self as u32;
        bus.write_regs(reg, &[word.to_raw()]).writing(FPGA, reg)
    }
}
