//! What the receive processor's output carries at DC: a deterministic RF loop from the
//! transmitter back to the receiver, for the gateware's DC estimator to measure, as
//! libusdr's LO-leakage and I/Q-imbalance calibration uses it (`cal/cal_lo_iqimb.c`
//! through `device/m2_lm7_1/xsdr_ctrl.c`'s `xsdrcal_*`).
//!
//! Register semantics, each from the pinned source:
//! - Banks: `LML_0x0020` `MAC` selects which channel's copy of the per-channel pages a
//!   write reaches (1 A, 2 B, 3 both; `opts` in `hw/lms7002m/lms7002m.yaml`). libusdr
//!   selects a bank before every write it makes to the pages from `TRF_0x0100` to
//!   `RXNCO_0x0443`, and its synthesizer code names the SX banks "SXR[0] / SXT[1]",
//!   selecting A for the receive synthesizer and B for the transmit one
//!   (`lms7002m_sxx_tune`, `lms7002m_sxx_disable`). The model keeps a copy of those pages
//!   per bank, beside the flat file the rest of the sim reads (see the crate docs).
//! - Synthesizers: libusdr's own on/off is `SXX_0x0124` `EN_DIR_SXX` (set by
//!   `lms7002m_sxx_tune`, cleared by `lms7002m_sxx_disable`). Its LO is the reference
//!   times `INT_SDM + 4 + FRAC_SDM / 2^20` (`_lms7002m_sxx_pll_vco_set` writes
//!   `nint - 4`), halved `DIV_LOCH` times: `lms7002m_sxx_tune` doubles the LO into the
//!   VCO's range once per halving it writes.
//! - Converter clocks: the clock generator's output is the reference times
//!   `INT_SDM + 1 + FRAC_SDM / 2^20`, over `2 * (DIV_OUTCH + 1)` (`lms7002m_cgen_tune`);
//!   the ADC clock is that over 4 ("Always fixed to 4", `lms7002m_samplerate`'s
//!   `mpy_adc`) and the DAC clock that over `2^CLKH_OV_CLKL` (the `clkdiv` it encodes
//!   `mpy_dac` as).
//! - Mixers: a processor's NCO shifts by `FCW0 / 2^32` of its converter clock, down when
//!   `CMIX_SC` is set (`DOWNCONVERT`), unless `CMIX_BYP` bypasses it
//!   (`lms7002m_bb_translate`, `lms7002m_xxtsp_cmix`).
//! - The TX test generator: with `INSEL` on `TEST` and `TSGMODE` on `DC`, the processor
//!   sends the DC levels last loaded from `DC_REG` by the `TSGDCLDI` and `TSGDCLDQ`
//!   strobes (`lms7002m_xxtsp_gen`).
//! - The RF loopback: closed for a channel while its TX pad's loopback is enabled
//!   (`TRF_0x0101` `EN_LOOPB_TXPAD_TRF`) and one of its receiver's loopback inputs is
//!   powered (`RFE_0x010C` `PD_RLOOPB_1_RFE` or `PD_RLOOPB_2_RFE` clear), as
//!   `lms7002m_trf_path` and `lms7002m_rfe_path` write them for their loopback modes;
//!   or always with an external cable ([`Loopback::external`]).
//! - Corrections: the analog DC DACs (`DC_0x05C3` to `DC_0x05CA`, not banked, 11 bits),
//!   each processor's gain codes (`GCORRI`, `GCORRQ`, bypassed by `GC_BYP`) and phase code
//!   (`IQCORR`, 12 bits, bypassed by `PH_BYP`), and the receive DC corrector (`DC_BYP`).
//!
//! Assumed, not documented (no datasheet is vendored), each a sim choice:
//! - Units: full scale is 1. A DC level is its 16-bit code over 32768; a DC DAC step is
//!   1/1024 of full scale in both directions, read as 11-bit two's complement (the
//!   encoding `xsdr` already assumes); the receive output is reported in 12-bit LSBs,
//!   2048 to full scale (libusdr's `xsdr_rfe_pwrdc_get` divides by 2048).
//! - Correction stages act as `I' = gI·I` and `Q' = gQ·(Q + t·I)`, with `gI`, `gQ` the
//!   gain codes over 2047 and `t` the phase code over 2048, applied after the transmit
//!   mixer and before the receive one, so they can cancel an image as the calibration
//!   presumes. The transmit chain is the generator, the mixer, the correction, the DC
//!   DAC, the analog imbalance, then the LO leakage; the receive chain the analog
//!   imbalance, then the DC offset and DAC, the correction, the DC corrector (which
//!   removes what sits at DC) and the mixer.
//! - The loop scales the transmitter's output by [`Loopback::gain`], whatever the gain
//!   registers say; both channels share one [`Loopback`]. Nothing is filtered or delayed:
//!   a component counts fully when it lands within [`AT_DC_HZ`] of DC after the receive
//!   mixer, and not at all otherwise. Band selection, mutes, power-downs other than the
//!   loopback's, `EN_DIR` of other blocks, the processors' enables, decimation and the
//!   `LimeLight` paths are not modelled; the host's TX samples (`INSEL` on `LML`) are
//!   not either, so only the test generator transmits.

use std::collections::BTreeMap;

/// Components this close to DC after the receive mixer are measured, in hertz.
pub const AT_DC_HZ: f64 = 1_000.0;

/// The per-channel pages: `TRF_0x0100` to `RXNCO_0x0443`.
const BANKED: core::ops::Range<u16> = 0x0100..0x0500;

/// `TRF_0x0101`: `EN_LOOPB_TXPAD_TRF` in bit 0.
const TRF_LOOPBACK: u16 = 0x0101;
/// `RFE_0x010C`: `PD_RLOOPB_1_RFE` (bit 6) and `PD_RLOOPB_2_RFE` (bit 5).
const RFE_POWER: u16 = 0x010c;
/// `SXX_0x011D`: the fraction's low 16 bits.
const SXX_FRAC_LOW: u16 = 0x011d;
/// `SXX_0x011E`: `INT_SDM` (13:4) and the fraction's top 4 bits (3:0).
const SXX_INT_FRAC: u16 = 0x011e;
/// `SXX_0x011F`: `DIV_LOCH` in bits 8:6.
const SXX_DIVIDER: u16 = 0x011f;
/// `SXX_0x0124`: `EN_DIR_SXX` in bit 4.
const SXX_EN_DIR: u16 = 0x0124;
/// `CGEN_0x0087`: the fraction's low 16 bits.
const CGEN_FRAC_LOW: u16 = 0x0087;
/// `CGEN_0x0088`: `INT_SDM` (13:4) and the fraction's top 4 bits (3:0).
const CGEN_INT_FRAC: u16 = 0x0088;
/// `CGEN_0x0089`: `CLKH_OV_CLKL` (12:11) and `DIV_OUTCH` (10:3).
const CGEN_DIVIDERS: u16 = 0x0089;

/// `TXTSP_0x0200`: `TSGDCLDQ` (6), `TSGDCLDI` (5), `TSGMODE` (3), `INSEL` (2).
pub(crate) const TX_MODE: u16 = 0x0200;
/// `TXTSP_0x020C`: `DC_REG`.
pub(crate) const TX_DC_LEVEL: u16 = 0x020c;
/// `TSGDCLDI`.
pub(crate) const LOAD_I: u16 = 1 << 5;
/// `TSGDCLDQ`.
pub(crate) const LOAD_Q: u16 = 1 << 6;

/// A processor's registers: one of the two (`TXTSP`/`TXNCO`, `RXTSP`/`RXNCO`).
#[derive(Clone, Copy)]
struct Processor {
    /// `GCORRQ`.
    gain_q: u16,
    /// `GCORRI`.
    gain_i: u16,
    /// `IQCORR` (bits 11:0).
    phase: u16,
    /// The bypass word.
    config: u16,
    /// `CMIX_BYP`'s bit in the bypass word.
    mixer_bypass: u16,
    /// `DC_BYP`'s bit in the bypass word.
    dc_bypass: u16,
    /// `FCW0_HI`, then `FCW0_LO`.
    fcw: [u16; 2],
}

/// The transmit processor (`TXTSP_0x0201` to `0x0208`, `TXNCO_0x0242`, `0x0243`).
const TXTSP: Processor = Processor {
    gain_q: 0x0201,
    gain_i: 0x0202,
    phase: 0x0203,
    config: 0x0208,
    mixer_bypass: 1 << 8,
    dc_bypass: 1 << 3,
    fcw: [0x0242, 0x0243],
};
/// The receive processor (`RXTSP_0x0401` to `0x040C`, `RXNCO_0x0442`, `0x0443`).
const RXTSP: Processor = Processor {
    gain_q: 0x0401,
    gain_i: 0x0402,
    phase: 0x0403,
    config: 0x040c,
    mixer_bypass: 1 << 7,
    dc_bypass: 1 << 2,
    fcw: [0x0442, 0x0443],
};
/// `CMIX_SC`, bit 13 of both bypass words.
const MIXER_DOWN: u16 = 1 << 13;
/// `GC_BYP`, bit 1 of both.
const GAIN_BYPASS: u16 = 1 << 1;
/// `PH_BYP`, bit 0 of both.
const PHASE_BYPASS: u16 = 1;

/// The TX DC DACs, I then Q, per channel (`DC_0x05C3` to `0x05C6`).
const TX_DC_DACS: [[u16; 2]; 2] = [[0x05c3, 0x05c4], [0x05c5, 0x05c6]];
/// The RX DC DACs (`DC_0x05C7` to `0x05CA`).
const RX_DC_DACS: [[u16; 2]; 2] = [[0x05c7, 0x05c8], [0x05c9, 0x05ca]];

/// A DC DAC step, in full scale.
const DAC_STEP: f64 = 1.0 / 1024.0;
/// A DC level's full-scale code.
const LEVEL_SCALE: f64 = 32_768.0;
/// The receive output's full scale, in 12-bit LSBs.
const RX_FULL_SCALE: f64 = 2048.0;

/// An analog I/Q imbalance: Q leans by `phase` and I is scaled by `1 + gain`, so
/// `I' = (1 + gain)·I` and `Q' = sin(phase)·I + cos(phase)·Q`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Imbalance {
    /// The I gain error, as a fraction.
    pub gain: f64,

    /// The quadrature error, in radians.
    pub phase: f64,
}

/// The RF loop a board adds to the chip; see the module docs for the model.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Loopback {
    /// The receiver's own DC offset at its mixer, I and Q, in RX DC DAC steps.
    pub rx_dc_offset: [f64; 2],

    /// The transmitter's LO leakage, I and Q, in TX DC DAC steps.
    pub tx_lo_leakage: [f64; 2],

    /// The transmitter's analog I/Q imbalance.
    pub tx_imbalance: Imbalance,

    /// The receiver's analog I/Q imbalance.
    pub rx_imbalance: Imbalance,

    /// The loop's amplitude gain, from the transmitter's output to the receiver's input.
    pub gain: f64,

    /// A cable closes the loop outside the chip, so the transmitter reaches the receiver
    /// whatever the loopback registers say.
    pub external: bool,
}

/// The per-bank copies of the per-channel pages, with the DC levels each bank's
/// generator loaded.
#[derive(Debug, Default)]
pub(crate) struct Banks {
    /// Bank A's then B's registers.
    regs: [BTreeMap<u16, u16>; 2],
    /// Each bank's TX generator DC levels, I then Q.
    levels: [[u16; 2]; 2],
}

impl Banks {
    /// Follows a write of `value` to `addr` with `mac` (`MAC`, bits 1:0) selected.
    pub(crate) fn write(&mut self, mac: u16, addr: u16, value: u16) {
        if !BANKED.contains(&addr) {
            return;
        }
        for bank in 0..2 {
            if mac & (1 << bank) == 0 {
                continue;
            }
            self.regs[bank].insert(addr, value);
            if addr == TX_MODE {
                let level = self.reg(bank, TX_DC_LEVEL);
                if value & LOAD_I != 0 {
                    self.levels[bank][0] = level;
                }
                if value & LOAD_Q != 0 {
                    self.levels[bank][1] = level;
                }
            }
        }
    }

    /// Bank `bank`'s copy of `addr`, 0 until written.
    fn reg(&self, bank: usize, addr: u16) -> u16 {
        self.regs[bank].get(&addr).copied().unwrap_or(0)
    }

    /// Forgets every write.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// The chip's registers as the model reads them.
pub(crate) struct View<'a> {
    /// The flat file, for the pages that are not banked.
    pub(crate) flat: &'a dyn Fn(u16) -> u16,
    /// The banks.
    pub(crate) banks: &'a Banks,
    /// The reference clock in hertz.
    pub(crate) reference: f64,
}

impl View<'_> {
    /// A synthesizer's LO in hertz, from bank `bank` (A the receive one, B the transmit
    /// one), or `None` while libusdr has it off.
    fn lo(&self, bank: usize) -> Option<f64> {
        let reg = |addr| self.banks.reg(bank, addr);
        if reg(SXX_EN_DIR) & (1 << 4) == 0 {
            return None;
        }
        let ratio = divider(reg(SXX_INT_FRAC), reg(SXX_FRAC_LOW), 4.0);
        let halvings = (reg(SXX_DIVIDER) >> 6) & 0b111;
        Some(self.reference * ratio / f64::from(1_u16 << halvings))
    }

    /// The ADC and DAC clocks in hertz.
    fn converters(&self) -> (f64, f64) {
        let ratio = divider((self.flat)(CGEN_INT_FRAC), (self.flat)(CGEN_FRAC_LOW), 1.0);
        let dividers = (self.flat)(CGEN_DIVIDERS);
        let output = f64::from((dividers >> 3) & 0xff) + 1.0;
        let cgen = self.reference * ratio / (2.0 * output);
        let dac_order = (dividers >> 11) & 0b11;
        (cgen / 4.0, cgen / f64::from(1_u16 << dac_order))
    }

    /// Channel `channel`'s processor `tsp`: its correction, and its mixer's shift in
    /// hertz at `clock` (`None` when bypassed).
    fn processor(&self, channel: usize, tsp: &Processor, clock: f64) -> (Matrix, Option<f64>) {
        let reg = |addr| self.banks.reg(channel, addr);
        let config = reg(tsp.config);
        let (gain_i, gain_q) = if config & GAIN_BYPASS == 0 {
            let code = |addr| f64::from(reg(addr) & 0x7ff) / 2047.0;
            (code(tsp.gain_i), code(tsp.gain_q))
        } else {
            (1.0, 1.0)
        };
        let lean = if config & PHASE_BYPASS == 0 {
            f64::from(sign_extend(reg(tsp.phase), 12)) / 2048.0
        } else {
            0.0
        };
        let correction = Matrix([[gain_i, 0.0], [gain_q * lean, gain_q]]);
        let mixer = (config & tsp.mixer_bypass == 0).then(|| {
            let [high, low] = tsp.fcw.map(reg);
            let word = f64::from(u32::from(high) << 16 | u32::from(low));
            let shift = word / 4_294_967_296.0 * clock;
            if config & MIXER_DOWN == 0 {
                shift
            } else {
                -shift
            }
        });
        (correction, mixer)
    }

    /// Whether channel `channel`'s RF loopback is closed.
    fn looped(&self, channel: usize) -> bool {
        let reg = |addr| self.banks.reg(channel, addr);
        let pad = reg(TRF_LOOPBACK) & 1 != 0;
        let input = reg(RFE_POWER) & (0b11 << 5) != 0b11 << 5;
        pad && input
    }

    /// A DC DAC's value in full scale.
    fn dac(&self, addr: u16) -> f64 {
        f64::from(sign_extend((self.flat)(addr), 11)) * DAC_STEP
    }

    /// The mean of channel `channel`'s receive output at DC, I and Q in 12-bit LSBs.
    pub(crate) fn rx_mean(&self, channel: usize, loopback: &Loopback) -> [f64; 2] {
        let Some(rx_lo) = self.lo(0) else {
            return [0.0; 2];
        };
        let (adc, dac) = self.converters();

        // The transmitter: generator, mixer, correction, DC DAC, imbalance, leakage.
        let mut tx = Vec::new();
        let (tx_correction, tx_mixer) = self.processor(channel, &TXTSP, dac);
        let mode = self.banks.reg(channel, TX_MODE);
        if mode & (1 << 2) != 0 && mode & (1 << 3) != 0 {
            let [i, q] = self.banks.levels[channel]
                .map(|level| f64::from(i16::from_ne_bytes(level.to_ne_bytes())) / LEVEL_SCALE);
            tx.push((tx_mixer.unwrap_or(0.0), Complex::new(i, q)));
        }
        let mut tx = tx_correction.apply(&tx);
        let [dac_i, dac_q] = TX_DC_DACS[channel].map(|addr| self.dac(addr));
        tx.push((0.0, Complex::new(dac_i, dac_q)));
        let mut tx = matrix(loopback.tx_imbalance).apply(&tx);
        let [leak_i, leak_q] = loopback.tx_lo_leakage.map(|steps| steps * DAC_STEP);
        tx.push((0.0, Complex::new(leak_i, leak_q)));

        // The loop: the TX LO lands this far from the RX LO.
        let mut rx = Vec::new();
        if let Some(tx_lo) = self
            .lo(1)
            .filter(|_| loopback.external || self.looped(channel))
        {
            rx.extend(
                tx.into_iter()
                    .map(|(hz, z)| (hz + tx_lo - rx_lo, z.scale(loopback.gain))),
            );
        }

        // The receiver: imbalance, DC offset and DAC, correction, DC corrector, mixer.
        let mut rx = matrix(loopback.rx_imbalance).apply(&rx);
        let [dac_i, dac_q] = RX_DC_DACS[channel].map(|addr| self.dac(addr));
        let [offset_i, offset_q] = loopback.rx_dc_offset.map(|steps| steps * DAC_STEP);
        rx.push((0.0, Complex::new(offset_i + dac_i, offset_q + dac_q)));
        let (rx_correction, rx_mixer) = self.processor(channel, &RXTSP, adc);
        let mut rx = rx_correction.apply(&rx);
        if self.banks.reg(channel, RXTSP.config) & RXTSP.dc_bypass == 0 {
            rx.retain(|(hz, _)| hz.abs() >= AT_DC_HZ);
        }
        let shift = rx_mixer.unwrap_or(0.0);
        let mean = rx
            .into_iter()
            .filter(|(hz, _)| (hz + shift).abs() < AT_DC_HZ)
            .fold(Complex::ZERO, |sum, (_, z)| sum.add(z));
        [mean.re * RX_FULL_SCALE, mean.im * RX_FULL_SCALE]
    }
}

/// A PLL's feedback ratio: `INT_SDM + offset + FRAC_SDM / 2^20`.
fn divider(int_frac: u16, frac_low: u16, offset: f64) -> f64 {
    let integer = f64::from((int_frac >> 4) & 0x3ff) + offset;
    let fraction = f64::from(u32::from(int_frac & 0xf) << 16 | u32::from(frac_low));
    integer + fraction / f64::from(1_u32 << 20)
}

/// The low `width` bits of `bits` as a two's-complement number.
fn sign_extend(bits: u16, width: u32) -> i16 {
    let unused = 16 - width;
    i16::from_ne_bytes((bits << unused).to_ne_bytes()) >> unused
}

/// An imbalance as a matrix on `(I, Q)`.
fn matrix(imbalance: Imbalance) -> Matrix {
    Matrix([
        [1.0 + imbalance.gain, 0.0],
        [imbalance.phase.sin(), imbalance.phase.cos()],
    ])
}

/// A complex amplitude, I real.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Complex {
    /// I.
    re: f64,
    /// Q.
    im: f64,
}

impl Complex {
    /// Nothing.
    const ZERO: Self = Self::new(0.0, 0.0);

    /// `re + j·im`.
    const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    /// The sum.
    fn add(self, other: Self) -> Self {
        Self::new(self.re + other.re, self.im + other.im)
    }

    /// The product.
    fn mul(self, other: Self) -> Self {
        Self::new(
            self.re * other.re - self.im * other.im,
            self.re * other.im + self.im * other.re,
        )
    }

    /// Scaled by `k`.
    fn scale(self, k: f64) -> Self {
        Self::new(self.re * k, self.im * k)
    }

    /// The conjugate.
    fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }
}

/// A real 2×2 matrix acting on `(I, Q)`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Matrix([[f64; 2]; 2]);

impl Matrix {
    /// Applies the matrix to tones at their frequencies in hertz: a tone `z` at `f`
    /// becomes `α·z` at `f` and its image `β·conj(z)` at `−f`, with
    /// `α = ((a + d) + j(c − b)) / 2` and `β = ((a − d) + j(c + b)) / 2` for
    /// `[[a, b], [c, d]]`. At DC the two add up to the matrix applied to the vector.
    fn apply(self, tones: &[(f64, Complex)]) -> Vec<(f64, Complex)> {
        let [[a, b], [c, d]] = self.0;
        let alpha = Complex::new(f64::midpoint(a, d), (c - b) / 2.0);
        let beta = Complex::new((a - d) / 2.0, f64::midpoint(c, b));
        tones
            .iter()
            .flat_map(|&(hz, z)| [(hz, alpha.mul(z)), (-hz, beta.mul(z.conj()))])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matrix_at_dc_is_applied_to_the_vector() {
        let m = Matrix([[1.0, 2.0], [3.0, 4.0]]);
        let sum = m
            .apply(&[(0.0, Complex::new(1.0, 1.0))])
            .iter()
            .fold(Complex::ZERO, |s, &(_, z)| s.add(z));
        assert!((sum.re - 3.0).abs() < 1e-12 && (sum.im - 7.0).abs() < 1e-12);
    }

    #[test]
    fn the_correction_cancels_the_image_of_a_lean_it_mirrors() {
        // A 1° lean of Q, undone by a phase code of -sin(1°) * 2048 on unity gains.
        let lean = 1_f64.to_radians();
        let skew = matrix(Imbalance {
            gain: 0.0,
            phase: lean,
        });
        let t = -lean.sin() / lean.cos();
        let correct = Matrix([[1.0, 0.0], [t, 1.0 / lean.cos()]]);
        let image = |tones: Vec<(f64, Complex)>| {
            let z = tones
                .iter()
                .filter(|(hz, _)| *hz < 0.0)
                .fold(Complex::ZERO, |sum, &(_, z)| sum.add(z));
            z.re.hypot(z.im)
        };
        let tone = [(1e6, Complex::new(1.0, 0.0))];
        assert!(image(skew.apply(&tone)) > 5e-3);
        let skewed = skew.apply(&tone);
        assert!(image(correct.apply(&skewed)) < 1e-9);
    }

    #[test]
    fn a_bank_selection_routes_writes_and_both_reach_each() {
        let mut banks = Banks::default();
        banks.write(1, 0x0201, 7);
        banks.write(2, 0x0201, 9);
        banks.write(3, 0x0400, 5);
        banks.write(3, 0x0020, 1);
        assert_eq!([banks.reg(0, 0x0201), banks.reg(1, 0x0201)], [7, 9]);
        assert_eq!([banks.reg(0, 0x0400), banks.reg(1, 0x0400)], [5, 5]);
        assert_eq!(banks.reg(0, 0x0020), 0, "global registers are not banked");
    }

    #[test]
    fn the_generator_loads_its_levels_on_the_strobes() {
        let mut banks = Banks::default();
        banks.write(1, TX_DC_LEVEL, 100);
        banks.write(1, TX_MODE, LOAD_I);
        banks.write(1, TX_DC_LEVEL, 200);
        banks.write(1, TX_MODE, 0);
        assert_eq!(banks.levels[0], [100, 0], "Q not loaded without its strobe");
        banks.write(1, TX_MODE, LOAD_Q);
        assert_eq!(banks.levels[0], [100, 200]);
    }
}
