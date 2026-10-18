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

use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;
pub(crate) use crate::portable::fpga::{FPGA, Nco, Phy, SubReg};
use crate::portable::sequence::FpgaAccess;
use crate::portable::stream::EngineSequence;

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

impl Phy {
    /// Sets an NCO's frequency word, then pulses the NCO reset (`_usdr_set_nco`).
    pub(crate) fn set_nco(self, bus: &mut dyn Bus, nco: Nco, word: i32) -> Result<(), Error> {
        for access in self.nco(nco, word) {
            Self::run(bus, access)?;
        }
        Ok(())
    }

    /// Performs one port write.
    fn run(bus: &mut dyn Bus, access: FpgaAccess) -> Result<(), Error> {
        let FpgaAccess::Write { reg, value } = access else {
            unreachable!("invariant: a port access is a register write");
        };
        bus.write_regs(reg, &[value]).writing(FPGA, reg)
    }

    /// An RX stream create's DSP-port writes, as a sequence the blocking board and the
    /// native USB handle both run: the RX chain's IQ correction for `imbalance`, again
    /// (`usdr_rxupdate_cal`).
    pub(crate) fn rx_create_writes(imbalance: IqImbalance) -> EngineSequence {
        let words = Self::iq_correction_words(imbalance.rx_terms());
        EngineSequence::port_writes(Self::Rx as u32, &words)
    }

    /// Pulses the TX chain's reset sub-register bit 3, which libusdr's TX stream create
    /// sends before setting up the extended TX core (`usdr_reset_txfex`: the port words 8,
    /// then 0). libusdr names it only by that use.
    pub(crate) fn tx_extended_reset_writes() -> EngineSequence {
        let pulse = [8, 0].map(|data| Self::word(SubReg::Reset, data));
        EngineSequence::port_writes(Self::Tx as u32, &pulse)
    }

    /// A TX stream create's DSP-port writes, likewise: the constant generator off
    /// (`usdr_tx_gen_set(.., false, ..)`), then the TX chain's IQ correction for
    /// `imbalance`, again (`usdr_txupdate_cal`).
    pub(crate) fn tx_create_writes(imbalance: IqImbalance) -> EngineSequence {
        let [a, b, c, d] = Self::iq_correction_words(imbalance.tx_terms());
        let off = Self::generator_off_word();
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
