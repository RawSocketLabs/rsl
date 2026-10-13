//! Corrections: `dev_m2_lm6_1_sdr_rx_dccorr_set`, `...rx_dccorrmode_set`,
//! `...rx_ip2corr_set`, `...rx_phgaincorr_set`, `...tx_dccorr_set` and
//! `...tx_phgaincorr_set` in `device/m2_lm6_1/m2_lm6_1.c`.
//!
//! The RX DC and IP2 trims are analog, in the LMS6002D's RX front end; the RX automatic DC
//! correction, the TX DC offset and both IQ imbalance corrections are digital, in the
//! FPGA's chains.

use bnb::{u7, u12};

use super::Board;
use crate::error::Error;
use crate::fpga::{IqImbalance, Phy};

/// A 7-bit sign-magnitude trim code for the LMS6002D's RX front end: bit 6 the sign,
/// bits 5:0 the magnitude, as the datasheet gives `DCOFF_I`, `DCOFF_Q`, `IP2TRIM_I` and
/// `IP2TRIM_Q`. The datasheet does not say which sign bit value is negative.
///
/// libusdr takes 16 bits per channel, narrows them to `int8_t` (implementation-defined)
/// and shifts the possibly negative result into its register macro (undefined in C11 for a
/// negative value), which GCC and Clang reduce to keeping the low seven bits. A code that
/// does not fit is rejected here instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxTrim(u7);

impl RxTrim {
    /// The trim for `code`, or `None` above 0x7F.
    #[must_use]
    pub fn new(code: u8) -> Option<Self> {
        (code <= 0x7f).then(|| Self(u7::new(code)))
    }

    /// The code.
    #[must_use]
    pub fn code(self) -> u8 {
        self.0.value()
    }
}

/// An RX IQ imbalance correction, in the terms of libusdr's `rx/phgaincorr`. The FPGA's RX
/// chain applies it; stream creation writes it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxIqCorrection {
    /// I gain, 2047 being no correction. Below 2047 it alone sets the amplitude
    /// correction, scaling I by `1 − (2047 − i_gain) · 8 / 32767`.
    pub i_gain: u16,

    /// Q gain, 2047 being no correction, used when `i_gain` is 2047 or more: below 2047 it
    /// scales Q by `1 − (2047 − q_gain) · 8 / 32767`, above it scales I by
    /// `1 − (q_gain − 2047) · 8 / 32767`.
    pub q_gain: u16,

    /// Phase correction: the terms use the sine and cosine of `phase × 0.016°`
    /// (libusdr multiplies it by 16 into a scale where 45000 is 45°). Not clamped.
    pub phase: i16,
}

impl RxIqCorrection {
    /// The imbalance this correction asks for (`_phgaincorr_set`, then
    /// `usdr_rx_iqimb_set`); see [`iq_imbalance`].
    fn imbalance(self) -> IqImbalance {
        iq_imbalance(self.i_gain, self.q_gain, self.phase)
    }
}

/// A TX IQ imbalance correction, in the terms of libusdr's `tx/phgaincorr`, which decodes
/// it as [`RxIqCorrection`]'s. The FPGA's TX chain applies it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxIqCorrection {
    /// I gain, 2047 being no correction. Below 2047 it alone sets the amplitude
    /// correction, scaling I by `1 − (2047 − i_gain) · 8 / 32767`.
    pub i_gain: u16,

    /// Q gain, 2047 being no correction, used when `i_gain` is 2047 or more: below 2047 it
    /// scales Q by `1 − (2047 − q_gain) · 8 / 32767`, above it scales I by
    /// `1 − (q_gain − 2047) · 8 / 32767`.
    pub q_gain: u16,

    /// Phase correction: the terms use the sine and cosine of `phase × 0.016°`
    /// (libusdr multiplies it by 16 into a scale where 45000 is 45°). Not clamped.
    pub phase: i16,
}

impl TxIqCorrection {
    /// The imbalance this correction asks for (`_phgaincorr_set`, then
    /// `usdr_tx_iqimb_set`); see [`iq_imbalance`].
    fn imbalance(self) -> IqImbalance {
        iq_imbalance(self.i_gain, self.q_gain, self.phase)
    }
}

/// The imbalance a phase/gain correction asks for, in libusdr's arithmetic for either
/// chain: `_phgaincorr_set`'s amplitude from whichever gain is off unity, then
/// `usdr_*_iqimb_set`'s scaling of it into `IMB_AMPL_MAX` units, truncated.
fn iq_imbalance(i_gain: u16, q_gain: u16, phase: i16) -> IqImbalance {
    /// The amplitude imbalance's input scale (`INT16_MAX` in `usdr_*_iqimb_set`).
    const AMPLITUDE_SCALE: i64 = 32_767;
    /// The amplitude imbalance's full scale (`IMB_AMPL_MAX`).
    const AMPLITUDE_FULL_SCALE: i64 = 262_144;
    /// The gain with no correction.
    const UNITY: i32 = 2047;

    let i_gain = i32::from(i_gain);
    let imbalance = if i_gain < UNITY {
        (UNITY - i_gain) * 8
    } else {
        (i32::from(q_gain) - UNITY) * 8
    };
    let amplitude = i64::from(imbalance) * AMPLITUDE_FULL_SCALE / AMPLITUDE_SCALE;
    IqImbalance {
        amplitude: i32::try_from(amplitude).expect("invariant: at most 507904 * 262144 / 32767"),
        phase: i32::from(phase) * 16,
    }
}

/// A DC offset for one channel of the FPGA's TX chain, a 12-bit two's-complement value
/// (-2048 to 2047) that cancels the transmitter's LO leakage; libusdr's TX LO calibration
/// searches it as a signed value.
///
/// libusdr takes 16 bits per channel and keeps the low 12, so a value outside the range
/// silently writes another. It is rejected here instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxDcOffset(i16);

impl TxDcOffset {
    /// The lowest offset.
    pub const MIN: i16 = -2048;

    /// The highest offset.
    pub const MAX: i16 = 2047;

    /// The offset `value`, or `None` outside [`Self::MIN`]..=[`Self::MAX`].
    #[must_use]
    pub fn new(value: i16) -> Option<Self> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Self(value))
    }

    /// The offset.
    #[must_use]
    pub fn value(self) -> i16 {
        self.0
    }

    /// The offset's 12 two's-complement bits.
    fn bits(self) -> u12 {
        u12::new(u16::from_ne_bytes(self.0.to_ne_bytes()) & 0xfff)
    }
}

impl Board {
    /// Writes the RX front end's DC-offset trims (`lms6002d_set_rxfedc`).
    pub(crate) fn set_rx_dc_trim(&mut self, i: RxTrim, q: RxTrim) -> Result<(), Error> {
        self.lms.set_rx_dc_trim(self.bus.as_mut(), i.0, q.0)
    }

    /// Writes the RX front end's IP2 trims (`lms6002d_set_rxfe_ip2corr`).
    pub(crate) fn set_rx_ip2_trim(&mut self, i: RxTrim, q: RxTrim) -> Result<(), Error> {
        self.lms.set_rx_ip2_trim(self.bus.as_mut(), i.0, q.0)
    }

    /// Switches the RX chain's automatic DC correction (`usdr_set_rxdccorr`).
    pub(crate) fn set_rx_auto_dc_correction(&mut self, enabled: bool) -> Result<(), Error> {
        Phy::Rx.set_dc_correction(self.bus.as_mut(), enabled)
    }

    /// Records and writes the RX IQ imbalance correction (`usdr_rx_iqimb_set`, then
    /// `usdr_rxupdate_cal`). As in libusdr, it is recorded even if a write fails.
    pub(crate) fn set_rx_iq_correction(&mut self, correction: RxIqCorrection) -> Result<(), Error> {
        self.set_rx_iq_imbalance(correction.imbalance())
    }

    /// Records and writes the RX IQ imbalance correction in the calibration's terms
    /// (libusdr's `rx_corr` set, then `usdr_rxupdate_cal`).
    pub(crate) fn set_rx_iq_imbalance(&mut self, imbalance: IqImbalance) -> Result<(), Error> {
        self.rx.iq_imbalance = imbalance;
        Phy::Rx.set_iq_correction(self.bus.as_mut(), imbalance.rx_terms())
    }

    /// Writes the TX chain's DC offsets (`usdr_tx_dccorr`).
    pub(crate) fn set_tx_dc_offset(&mut self, i: TxDcOffset, q: TxDcOffset) -> Result<(), Error> {
        Phy::set_tx_dc_offset(self.bus.as_mut(), i.bits(), q.bits())
    }

    /// Writes the TX IQ imbalance correction (`usdr_tx_iqimb_set`, then
    /// `usdr_txupdate_cal`), at the TX chain's headroom scale, and keeps it for TX stream
    /// creation, which writes it again.
    pub(crate) fn set_tx_iq_correction(&mut self, correction: TxIqCorrection) -> Result<(), Error> {
        self.set_tx_iq_imbalance(correction.imbalance())
    }

    /// Records and writes the TX IQ imbalance correction in the calibration's terms
    /// (libusdr's `tx_corr` set, then `usdr_txupdate_cal`).
    pub(crate) fn set_tx_iq_imbalance(&mut self, imbalance: IqImbalance) -> Result<(), Error> {
        self.tx.iq_imbalance = imbalance;
        Phy::Tx.set_iq_correction(self.bus.as_mut(), imbalance.tx_terms())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_take_seven_bit_codes_only() {
        assert_eq!(RxTrim::new(0x7f).map(RxTrim::code), Some(0x7f));
        assert_eq!(RxTrim::new(0).map(RxTrim::code), Some(0));
        assert_eq!(RxTrim::new(0x80), None);
        assert_eq!(RxTrim::new(u8::MAX), None);
    }

    /// The imbalance a correction with `i_gain`, `q_gain` and `phase` asks for.
    fn imbalance(i_gain: u16, q_gain: u16, phase: i16) -> IqImbalance {
        RxIqCorrection {
            i_gain,
            q_gain,
            phase,
        }
        .imbalance()
    }

    #[test]
    fn unity_gains_and_no_phase_ask_for_no_correction() {
        assert_eq!(imbalance(2047, 2047, 0), IqImbalance::default());
    }

    #[test]
    fn i_below_unity_sets_the_amplitude_and_ignores_q() {
        // (2047 - 0) * 8 = 16376, then * 262144 / 32767, truncated.
        let full = imbalance(0, 9999, 0);
        assert_eq!(full.amplitude, 131_011);
        assert_eq!(full.amplitude, imbalance(0, 2047, 0).amplitude);
        assert!(imbalance(2046, 0, 0).amplitude > 0);
    }

    #[test]
    fn from_unity_up_q_sets_the_amplitude_either_way() {
        // Truncated towards zero, as C's int64_t division does.
        assert_eq!(imbalance(2047, 0, 0).amplitude, -131_011);
        assert!(imbalance(u16::MAX, 2048, 0).amplitude > 0);
    }

    #[test]
    fn tx_dc_offsets_take_twelve_bit_twos_complement_values_only() {
        let bits = |value| TxDcOffset::new(value).map(|offset| offset.bits().value());
        assert_eq!(bits(0), Some(0));
        assert_eq!(bits(2047), Some(0x7ff));
        assert_eq!(bits(-1), Some(0xfff));
        assert_eq!(bits(-2048), Some(0x800));
        assert_eq!(bits(2048), None);
        assert_eq!(bits(-2049), None);
        assert_eq!(bits(i16::MIN), None);
    }

    #[test]
    fn phase_is_scaled_by_16_without_clamping() {
        assert_eq!(imbalance(2047, 2047, -1).phase, -16);
        assert_eq!(imbalance(2047, 2047, i16::MAX).phase, 524_272);
    }
}
