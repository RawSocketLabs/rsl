//! The RX and TX DSP chains' configuration ports (`REG_CFG_PHY_0` and `REG_CFG_PHY_1`).
//!
//! Each port is one FPGA register that takes sub-register writes: bits 30:24 pick the
//! sub-register (libusdr's `fpga_phy_regs`), bits 23:0 carry its value. Through it the
//! driver resets the chain, loads the decimator's FIR microcode, switches RX DC correction,
//! sets the TX DC offset, the IQ imbalance correction and the two NCOs, and drives the TX
//! chain's constant generator.
//!
//! Source: `REG_CFG_PHY_*` in `device/generic_usdr/generic_regs.h` and `fpga_phy_regs`,
//! `_usdr_set_nco`, `usdr_set_samplerate_ex`, `usdr_tx_dccorr`, `usdr_calc_iqimb` and
//! `usdr_tx_gen_set` in `device/m2_lm6_1/usdr_ctrl.c`;
//! `fgearbox_load_ucode` in `ipblks/fgearbox.c`.

use std::f64::consts::FRAC_PI_4;
use std::time::Duration;

use bnb::{BitEnum, bitfield, u7, u12, u24};
use num_complex::Complex;

use super::fir_tables;
use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;
use crate::portable::stream::EngineSequence;

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

    /// DC correction (`CFG_REG_DCCTRL`): on the RX chain, the automatic correction's enable
    /// (`usdr_set_rxdccorr`); on the TX chain, the DC offset, Q in bits 23:12 and I in 11:0
    /// (`usdr_tx_dccorr`).
    DcControl = 1,

    /// IQ imbalance correction, I amplitude (`CFG_REG_IQIMB_0`).
    IqAmplitudeI = 2,

    /// IQ imbalance correction, Q amplitude (`CFG_REG_IQIMB_1`).
    IqAmplitudeQ = 3,

    /// IQ imbalance correction, I phase term (`CFG_REG_IQIMB_2`).
    IqPhaseI = 4,

    /// IQ imbalance correction, Q phase term (`CFG_REG_IQIMB_3`).
    IqPhaseQ = 5,

    /// The TX chain's constant generator, which replaces the samples with a fixed I/Q pair
    /// (`CFG_REG_DC_GEN`); see [`GeneratorWrite`].
    Generator = 7,

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

/// The value of one write to [`SubReg::Generator`] (`MAKE_PHY_WR_SUB_REG`): which of the
/// generator's own registers, and its 16-bit value.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GeneratorWrite {
    /// The generator register.
    #[bits(16..=23)]
    reg: GeneratorReg,

    /// The value written.
    #[bits(0..=15)]
    value: u16,
}

/// The constant generator's registers (libusdr's `CFG_REG_DC_GEN_*` in `usdr_ctrl.c`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
enum GeneratorReg {
    /// 1 replaces the TX samples with the constants, 0 passes them (`CFG_REG_DC_GEN_CTRL`).
    Control = 0,

    /// Channel A's I constant (`CFG_REG_DC_GEN_AI`).
    ChannelAI = 4,

    /// Channel A's Q constant (`CFG_REG_DC_GEN_AQ`).
    ChannelAQ = 5,

    /// Channel B's I constant (`CFG_REG_DC_GEN_BI`).
    ChannelBI = 6,

    /// Channel B's Q constant (`CFG_REG_DC_GEN_BQ`).
    ChannelBQ = 7,
}

/// One of a chain's two numerically controlled oscillators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nco {
    /// NCO 0.
    Nco0,

    /// NCO 1.
    Nco1,
}

/// An IQ imbalance correction as libusdr keeps it for a chain (`struct imb_data`), the
/// terms its calibration searches; the default is none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IqImbalance {
    /// Amplitude correction, ±262144 full scale (`ampl`, `IMB_AMPL_MAX`): positive scales
    /// I down by that fraction, negative scales Q down.
    pub amplitude: i32,

    /// Phase correction, 45000 turning each channel by π/4 (`pahse`, `IMB_PHASE_MAX`).
    /// libusdr does not clamp either field.
    pub phase: i32,
}

impl IqImbalance {
    /// The amplitude full scale (`IMB_AMPL_MAX`).
    const AMPLITUDE_FULL_SCALE: f64 = 262_144.0;

    /// The phase that rotates by π/4 (`IMB_PHASE_MAX`).
    const PHASE_FULL_SCALE: f64 = 45_000.0;

    /// The RX chain's headroom scale: two channels, no 3 dB boost (`AMP_COMP_2CH_0DB`,
    /// `usdr_ctor`'s setting for RX).
    const RX_SCALE: f32 = 0.5;

    /// The TX chain's headroom scale, 0.35355: two channels with a 3 dB boost
    /// (`AMP_COMP_2CH_3DB`), libusdr's zeroed default, which `usdr_ctor` leaves for TX.
    /// libusdr narrows the double literal to float, so this does too.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "libusdr narrows its double literal to float"
    )]
    const TX_SCALE: f32 = 0.353_55_f64 as f32;

    /// The largest 24-bit term (`dsp_amp_max`).
    const TERM_MAX: f32 = 8_388_607.0;

    /// The four correction terms for the RX chain (`usdr_rxupdate_cal`); see
    /// [`Self::terms`].
    pub(crate) fn rx_terms(self) -> [i32; 4] {
        self.terms(Self::RX_SCALE)
    }

    /// The four correction terms for the TX chain (`usdr_txupdate_cal`); see
    /// [`Self::terms`].
    pub(crate) fn tx_terms(self) -> [i32; 4] {
        self.terms(Self::TX_SCALE)
    }

    /// The four correction terms at headroom `scale`, I and Q amplitudes then I and Q phase
    /// terms, in libusdr's arithmetic (`usdr_calc_iqimb`): sine and cosine in double,
    /// narrowed to float, the products in float, then truncated to integers. Exact parity
    /// rests on `f64::cos` and `sin` calling the same libm as libusdr (glibc on the oracle
    /// host); another libm may differ in the last bit.
    fn terms(self, scale: f32) -> [i32; 4] {
        let angle = FRAC_PI_4 * f64::from(self.phase) / Self::PHASE_FULL_SCALE;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr stores these doubles in floats"
        )]
        let (cos, sin) = (angle.cos() as f32, angle.sin() as f32);
        let amplitude = f64::from(self.amplitude) / Self::AMPLITUDE_FULL_SCALE;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr stores these doubles in floats"
        )]
        let (alpha, beta) = (
            if self.amplitude > 0 {
                1.0 - amplitude
            } else {
                1.0
            } as f32,
            if self.amplitude < 0 {
                1.0 + amplitude
            } else {
                1.0
            } as f32,
        );
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr converts the float products to int32_t, truncating"
        )]
        let term = |factor: f32| (Self::TERM_MAX * factor) as i32;
        [
            term(scale * cos * alpha),
            term(scale * cos * beta),
            term(-scale * sin * alpha),
            term(scale * sin * beta),
        ]
    }
}

/// The DSP chains' rate-change factor, with the FIR microcode for it: the RX chain's
/// decimation and the TX chain's interpolation, which libusdr always sets alike.
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

    /// The TX FIR microcode libusdr loads for this interpolation factor on the uSDR's
    /// 7-series FPGA (`fgearbox_load_fir_i_ex` with `DSP_7SERIES`).
    fn tx_fir(self) -> &'static [u8; 1280] {
        match self {
            Self::X1 => &fir_tables::BYPASS,
            Self::X2 => &fir_tables::INTERPOLATE_2,
            Self::X4 => &fir_tables::INTERPOLATE_4,
            Self::X8 => &fir_tables::INTERPOLATE_8,
            Self::X16 => &fir_tables::INTERPOLATE_16,
            Self::X32 => &fir_tables::INTERPOLATE_32,
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

    /// Writes the IQ imbalance correction terms, as [`IqImbalance::rx_terms`] and
    /// [`IqImbalance::tx_terms`] make them: I and Q amplitudes, then I and Q phase terms
    /// (`usdr_rxupdate_cal`, `usdr_txupdate_cal`). libusdr keeps each term's low 24 bits.
    pub(crate) fn set_iq_correction(self, bus: &mut dyn Bus, terms: [i32; 4]) -> Result<(), Error> {
        for word in Self::iq_correction_words(terms) {
            self.write_word(bus, word)?;
        }
        Ok(())
    }

    /// The words [`Self::set_iq_correction`] writes for `terms`, in its order.
    fn iq_correction_words(terms: [i32; 4]) -> [u32; 4] {
        let subs = [
            SubReg::IqAmplitudeI,
            SubReg::IqAmplitudeQ,
            SubReg::IqPhaseI,
            SubReg::IqPhaseQ,
        ];
        let mut words = [0; 4];
        for ((word, sub), term) in words.iter_mut().zip(subs).zip(terms) {
            let [b0, b1, b2, _] = term.to_le_bytes();
            *word = Self::word(sub, u32::from_le_bytes([b0, b1, b2, 0]));
        }
        words
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

    /// Writes the TX chain's DC offset, 12-bit two's-complement `i` and `q`
    /// (`usdr_tx_dccorr`).
    pub(crate) fn set_tx_dc_offset(bus: &mut dyn Bus, i: u12, q: u12) -> Result<(), Error> {
        let data = u32::from(q.value()) << 12 | u32::from(i.value());
        Self::Tx.write(bus, SubReg::DcControl, data)
    }

    /// Replaces the TX chain's samples with `constant` on both channels, or with `None`
    /// passes them again (`usdr_tx_gen_set` with channel mask 3): the enable, then, when
    /// on, channel A's I and Q and channel B's.
    pub(crate) fn set_tx_constant(
        bus: &mut dyn Bus,
        constant: Option<Complex<i16>>,
    ) -> Result<(), Error> {
        let write = |bus: &mut dyn Bus, reg: GeneratorReg, value: u16| {
            Self::Tx.write_word(bus, Self::generator_word(reg, value))
        };
        write(bus, GeneratorReg::Control, u16::from(constant.is_some()))?;
        let Some(constant) = constant else {
            return Ok(());
        };
        // Two's-complement bits, as libusdr's `int16_t` masked to 16 bits.
        let bits = |value: i16| u16::from_ne_bytes(value.to_ne_bytes());
        let (i, q) = (bits(constant.re), bits(constant.im));
        write(bus, GeneratorReg::ChannelAI, i)?;
        write(bus, GeneratorReg::ChannelAQ, q)?;
        write(bus, GeneratorReg::ChannelBI, i)?;
        write(bus, GeneratorReg::ChannelBQ, q)
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

    /// Loads the TX interpolator's FIR microcode for `interpolation`, a byte per write,
    /// 10 µs apart (`fgearbox_load_fir_i_ex` with `sleep_us` 10).
    pub(crate) fn load_tx_fir(bus: &mut dyn Bus, interpolation: Decimation) -> Result<(), Error> {
        for &byte in interpolation.tx_fir() {
            Self::Tx.write(bus, SubReg::FirLoad, byte.into())?;
            bus.sleep(Duration::from_micros(10));
        }
        Ok(())
    }

    /// The port word writing `value` to the constant generator's register `reg`.
    fn generator_word(reg: GeneratorReg, value: u16) -> u32 {
        let data = GeneratorWrite::new().with_reg(reg).with_value(value);
        Self::word(SubReg::Generator, data.to_raw())
    }

    /// The port word writing `data` to a sub-register (`MAKE_PHY_WR_REG`).
    fn word(sub: SubReg, data: u32) -> u32 {
        PhyWrite::new()
            .with_sub(u7::new(sub as u8))
            .with_data(u24::new(data))
            .to_raw()
    }

    /// Writes `data` to a sub-register.
    fn write(self, bus: &mut dyn Bus, sub: SubReg, data: u32) -> Result<(), Error> {
        self.write_word(bus, Self::word(sub, data))
    }

    /// Writes one port word.
    fn write_word(self, bus: &mut dyn Bus, word: u32) -> Result<(), Error> {
        let reg = self as u32;
        bus.write_regs(reg, &[word]).writing(FPGA, reg)
    }

    /// An RX stream create's DSP-port writes, as a sequence the blocking board and the
    /// native USB handle both run: the RX chain's IQ correction for `imbalance`, again
    /// (`usdr_rxupdate_cal`).
    pub(crate) fn rx_create_writes(imbalance: IqImbalance) -> EngineSequence {
        let words = Self::iq_correction_words(imbalance.rx_terms());
        EngineSequence::port_writes(Self::Rx as u32, &words)
    }

    /// A TX stream create's DSP-port writes, likewise: the constant generator off
    /// (`usdr_tx_gen_set(.., false, ..)`), then the TX chain's IQ correction for
    /// `imbalance`, again (`usdr_txupdate_cal`).
    pub(crate) fn tx_create_writes(imbalance: IqImbalance) -> EngineSequence {
        let [a, b, c, d] = Self::iq_correction_words(imbalance.tx_terms());
        let off = Self::generator_word(GeneratorReg::Control, 0);
        EngineSequence::port_writes(Self::Tx as u32, &[off, a, b, c, d])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::sequence::{FpgaAccess, Sequence, SequenceAction};

    #[test]
    fn a_tx_create_turns_the_generator_off_then_writes_the_iq_correction() {
        let mut dsp = Phy::tx_create_writes(IqImbalance::default());
        let mut words = Vec::new();
        while let Some(SequenceAction::Fpga(FpgaAccess::Write { reg: 57, value })) = dsp.next() {
            words.push(value);
            dsp.complete::<()>(Ok(0)).unwrap();
        }
        assert_eq!(dsp.next(), None, "only TX-port writes");
        // Sub-register 7 with generator register 0 and value 0 (`CFG_REG_DC_GEN_CTRL` off),
        // then sub-registers 2 to 5 with the default TX terms (`AMP_COMP_2CH_3DB`).
        assert_eq!(
            words,
            [
                0x0700_0000,
                0x022d_4120,
                0x032d_4120,
                0x0400_0000,
                0x0500_0000
            ]
        );
    }

    #[test]
    fn no_imbalance_gives_half_scale_amplitudes_and_no_phase_terms() {
        // RX's two-channel scale of 0.5 times 8388607, truncated.
        assert_eq!(
            IqImbalance::default().rx_terms(),
            [4_194_303, 4_194_303, 0, 0]
        );
    }

    #[test]
    fn tx_terms_use_the_3_db_two_channel_scale() {
        // 0.35355 times 8388607 in float, truncated (worked independently in single
        // precision): libusdr's `AMP_COMP_2CH_3DB`.
        assert_eq!(
            IqImbalance::default().tx_terms(),
            [2_965_792, 2_965_792, 0, 0]
        );
    }

    #[test]
    fn amplitude_scales_one_channel_and_phase_mirrors_the_cross_terms() {
        let imbalance = IqImbalance {
            amplitude: 65_536,
            phase: 16_000,
        };
        let [amp_i, amp_q, tan_i, tan_q] = imbalance.rx_terms();
        assert!(amp_i < amp_q, "a positive imbalance scales I down");
        assert!(tan_i < 0 && tan_q > 0, "phase terms take opposite signs");
    }
}
