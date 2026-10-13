//! The RX chain's DC estimator behind `REG_CFG_PHY_0`, and an RF loopback for it to
//! measure, as libusdr's LO-leakage and IQ-imbalance calibration uses them
//! (`ipblks/dc_estim.c`, `cal/cal_lo_iqimb.c`).
//!
//! The DSP chains' configuration ports (registers 56 and 57) take sub-register writes,
//! bits 30:24 the sub-register and 23:0 its value; the sim latches each so the loopback
//! can read what the drivers set. Sub-register 6 of the RX port is the DC estimator,
//! addressed as `bit 31 read | bank 30:24 | address 23:16 | value 15:0`
//! (`_phy_rx_reg`): written with bit 31 clear, address 0 holds (1) or runs (0) it and
//! address 1 sets its accumulation; with bit 31 set, the address selects the word the
//! next read of register 56 returns: the generation (0) or a channel's I or Q mean
//! (4 to 7: A I, A Q, B I, B Q).
//!
//! Assumed, not documented (no gateware source is vendored):
//! - The estimator finishes one measurement per millisecond of virtual time while it
//!   runs, counting them in its generation word, whatever the accumulation. libusdr polls
//!   the generation every millisecond, so one poll sees it move.
//! - Its I and Q words are the mean of the channel's samples after the RX NCO (channel A
//!   NCO 0, B NCO 1), in 16-bit full-scale units, as 16.16 fixed point rounded to nearest
//!   and saturated: `phy_rfe_pwrdc_get` divides them by 65536 and by the full scale.
//!   They are computed from the registers when read; the drivers hold the registers
//!   still while a measurement runs.
//! - The IQ correction terms (sub-registers 2 to 5, `usdr_calc_iqimb`'s I and Q
//!   amplitudes and phase terms) act as `I = AMP_I·i + TAN_Q·q` and
//!   `Q = AMP_Q·q − TAN_I·i`, each over the chain's unity term (RX 0.5, TX 0.35355 of
//!   full scale). libusdr's comment gives `+ TAN_I·i`, but its `TAN_I` is already
//!   negated, which would make the phase correction a rotation that cannot cancel an
//!   image; the sign is chosen so the correction can, as the calibration presumes.
//!   Unwritten terms pass the signal unchanged.
//! - The TX chain is generator (sub-register 7, channel A's constant) shifted by TX NCO
//!   0, then the IQ correction, then the DC offset (sub-register 1, 12-bit I and Q in DAC
//!   steps of 16), then the analog IQ imbalance, then the LO leakage and TXVGA1's DC
//!   shift (LMS6002D 0x42, 0x43, offset binary, one DAC step per code). The RX chain is
//!   the analog IQ imbalance, then the IQ correction, then the NCO. NCO frequency is the
//!   signed word over 2^32 of the converter clock.
//! - The loopback couples the TX output into the receiver, scaled by [`Loopback::gain`],
//!   while the LMS6002D's RF loopback switch (0x08 `LBRFEN`) is on, or always with an
//!   external cable. LOs come from the synthesizers' registers. Nothing is filtered or
//!   delayed, and no gain register matters: a component counts fully when it lands
//!   within 1 kHz of DC after the NCO, and not at all otherwise.

use std::collections::BTreeMap;

use crate::chips::Lms6002d;

/// The RX chain's configuration port (`REG_CFG_PHY_0`).
pub(crate) const REG_PHY_RX: u32 = 56;
/// The TX chain's configuration port (`REG_CFG_PHY_1`).
pub(crate) const REG_PHY_TX: u32 = 57;

/// The DC estimator's sub-register on the RX port (`PHY_REG_RXBANK_DC_EST`).
const ESTIMATOR_BANK: u8 = 6;
/// Bit 31 of an estimator access: select a word to read.
const ESTIMATOR_READ: u32 = 1 << 31;
/// Estimator write address: hold (1) or run (0).
const ESTIMATOR_HOLD: u8 = 0;
/// Estimator read address: the generation.
const ESTIMATOR_GENERATION: u8 = 0;
/// Virtual microseconds per measurement.
const MEASUREMENT_US: u64 = 1000;

/// The generator's sub-register on the TX port (`CFG_REG_DC_GEN`).
const GENERATOR: u8 = 7;
/// The DC offset's sub-register on the TX port (`CFG_REG_DCCTRL`).
const DC_OFFSET: u8 = 1;
/// The first IQ correction term (`CFG_REG_IQIMB_0`).
const IQ_TERMS: u8 = 2;
/// NCO 0's low and high frequency halves (`CFG_REG_NCO0_L`, `_H`); NCO 1 follows.
const NCO0: u8 = 8;

/// The largest correction term (`dsp_amp_max`).
const TERM_MAX: f64 = 8_388_607.0;
/// Components this close to DC after the NCO are measured, in Hz.
const AT_DC_HZ: f64 = 1_000.0;
/// DAC steps to 16-bit sample units: the LMS6002D's DAC has 12 bits.
const DAC_STEP: f64 = 16.0;

/// An analog IQ imbalance: Q leans by `phase` and I is scaled by `1 + gain`, so
/// `I' = (1 + gain)·I` and `Q' = sin(phase)·I + cos(phase)·Q`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Imbalance {
    /// The I gain error, as a fraction.
    pub gain: f64,

    /// The quadrature error, in radians.
    pub phase: f64,
}

/// What the RF loopback adds to a board; see the module docs for the model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loopback {
    /// The converters' clock in Hz, the NCOs' reference (libusdr's `adc_clk`, the same as
    /// `dac_clk` on the uSDR). The sim does not derive it from the Si5332, so a test sets
    /// it to the rate times the decimation the driver chose.
    pub converter_clock_hz: u32,

    /// The transmitter's LO leakage, I and Q, in TX DC-offset steps.
    pub tx_lo_leakage: [f64; 2],

    /// The transmitter's analog IQ imbalance.
    pub tx_imbalance: Imbalance,

    /// The receiver's analog IQ imbalance.
    pub rx_imbalance: Imbalance,

    /// The loop's amplitude gain, from the DAC to the ADC.
    pub gain: f64,

    /// A cable closes the loop outside the chip, so the TX output reaches the receiver
    /// whatever the LMS6002D's loopback switch.
    pub external: bool,
}

/// A DSP chain's configuration port: the last value of each sub-register.
#[derive(Debug, Default)]
pub(crate) struct PhyPort {
    /// By sub-register.
    subs: BTreeMap<u8, u32>,
    /// The constant generator's registers, by address (`CFG_REG_DC_GEN_*`).
    generator: BTreeMap<u8, u16>,
}

impl PhyPort {
    /// Latches a write.
    fn on_write(&mut self, value: u32) {
        let [sub, ..] = value.to_be_bytes();
        let sub = sub & 0x7f;
        let data = value & 0x00ff_ffff;
        if sub == GENERATOR {
            let [_, _, reg, _] = data.to_le_bytes();
            self.generator.insert(reg, (data & 0xffff) as u16);
        }
        self.subs.insert(sub, data);
    }

    /// NCO `index`'s frequency in Hz at `clock_hz`.
    fn nco_hz(&self, index: u8, clock_hz: u32) -> f64 {
        let half = |sub| {
            self.subs
                .get(&(NCO0 + 2 * index + sub))
                .map_or(0, |v| v & 0xffff)
        };
        let word = sign_extend(half(1) << 16 | half(0), 32);
        f64::from(word) / 4_294_967_296.0 * f64::from(clock_hz)
    }

    /// The IQ correction over `unity`, or `None` while no term was written.
    fn correction(&self, unity: f64) -> Matrix {
        let terms: Option<Vec<f64>> = (0..4)
            .map(|k| {
                self.subs
                    .get(&(IQ_TERMS + k))
                    .map(|&v| f64::from(sign_extend(v, 24)) / unity)
            })
            .collect();
        let Some([amp_i, amp_q, tan_i, tan_q]) = terms.as_deref().map(|t| [t[0], t[1], t[2], t[3]])
        else {
            return Matrix::IDENTITY;
        };
        Matrix([[amp_i, tan_q], [-tan_i, amp_q]])
    }
}

/// The DC estimator's state.
#[derive(Debug, Default)]
pub(crate) struct DcEstimator {
    /// When it last started running, in virtual microseconds; `None` while held.
    started_us: Option<u64>,
    /// The generation it had when it last started or was held.
    base_generation: u32,
    /// The word the next read of [`REG_PHY_RX`] returns, once one was selected.
    selected: Option<u8>,
}

impl DcEstimator {
    /// The generation at `now_us`.
    fn generation(&self, now_us: u64) -> u32 {
        let finished = self
            .started_us
            .map_or(0, |start| (now_us - start) / MEASUREMENT_US);
        self.base_generation
            .wrapping_add(u32::try_from(finished).unwrap_or(u32::MAX))
    }
}

/// The RX and TX configuration ports, the DC estimator and the loopback.
#[derive(Debug, Default)]
pub(crate) struct Chains {
    /// The RX port.
    rx: PhyPort,
    /// The TX port.
    tx: PhyPort,
    /// The estimator.
    estimator: DcEstimator,
    /// The loopback, if the board has one.
    pub(crate) loopback: Option<Loopback>,
}

impl Chains {
    /// Follows a register write at `now_us`.
    pub(crate) fn on_write(&mut self, addr: u32, value: u32, now_us: u64) {
        match addr {
            REG_PHY_RX if (value >> 24) & 0x7f == u32::from(ESTIMATOR_BANK) => {
                let [_, _, address, _] = value.to_le_bytes();
                if value & ESTIMATOR_READ != 0 {
                    self.estimator.selected = Some(address);
                } else if address == ESTIMATOR_HOLD {
                    let estimator = &mut self.estimator;
                    estimator.base_generation = estimator.generation(now_us);
                    // libusdr writes 1 to hold it and 0 to run it.
                    estimator.started_us = (value & 1 == 0).then_some(now_us);
                }
            }
            REG_PHY_RX => self.rx.on_write(value),
            REG_PHY_TX => self.tx.on_write(value),
            _ => {}
        }
    }

    /// What a read of `addr` returns, if these blocks answer it.
    pub(crate) fn read(&self, addr: u32, now_us: u64, lms: &Lms6002d) -> Option<u32> {
        if addr != REG_PHY_RX {
            return None;
        }
        let selected = self.estimator.selected?;
        if selected == ESTIMATOR_GENERATION {
            return Some(self.estimator.generation(now_us));
        }
        let (channel, quadrature) = match selected {
            4 => (0, false),
            5 => (0, true),
            6 => (1, false),
            7 => (1, true),
            _ => return Some(0),
        };
        let mean = self.measure(channel, lms);
        let part = if quadrature { mean.im } else { mean.re };
        // 16.16 fixed point, saturated as a 32-bit word.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "rounded and clamped to the i32 range first"
        )]
        let fixed = (part * 65_536.0)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        Some(u32::from_ne_bytes(fixed.to_ne_bytes()))
    }

    /// The mean of channel `channel`'s samples after its RX NCO.
    fn measure(&self, channel: u8, lms: &Lms6002d) -> Complex {
        let Some(loopback) = self.loopback else {
            return Complex::ZERO;
        };
        if !(loopback.external || lms.rf_loopback_on()) {
            return Complex::ZERO;
        }
        let clock = loopback.converter_clock_hz;

        // TX: generator, NCO, correction, DC offset, analog imbalance, leakage.
        let mut tx = Vec::new();
        if self.tx.generator.get(&0).is_some_and(|&on| on & 1 != 0) {
            let value = |reg| {
                f64::from(
                    self.tx
                        .generator
                        .get(&reg)
                        .map_or(0, |&v| i16::from_ne_bytes(v.to_ne_bytes())),
                )
            };
            tx.push((self.tx.nco_hz(0, clock), Complex::new(value(4), value(5))));
        }
        let tx_unity = f64::from(0.353_55_f32) * TERM_MAX;
        let mut tx = self.tx.correction(tx_unity).apply(&tx);
        let offset = self.tx.subs.get(&DC_OFFSET).copied().unwrap_or(0);
        let twelve = |bits: u32| f64::from(sign_extend(bits, 12)) * DAC_STEP;
        let vga1_shift = |reg| f64::from(i16::from(lms.reg(reg)) - 0x80) * DAC_STEP;
        tx.push((
            0.0,
            Complex::new(
                twelve(offset & 0xfff) + vga1_shift(0x42),
                twelve(offset >> 12) + vga1_shift(0x43),
            ),
        ));
        let mut tx = loopback.tx_imbalance.matrix().apply(&tx);
        let [leak_i, leak_q] = loopback.tx_lo_leakage;
        tx.push((0.0, Complex::new(leak_i * DAC_STEP, leak_q * DAC_STEP)));

        // The loop: the TX LO lands this far from the RX LO.
        #[expect(
            clippy::cast_precision_loss,
            reason = "synthesizer LOs are far below 2^52 Hz, so exact"
        )]
        let shift = lms.tx_lo_hz() as f64 - lms.rx_lo_hz() as f64;
        let rx: Vec<_> = tx
            .into_iter()
            .map(|(hz, z)| (hz + shift, z.scale(loopback.gain)))
            .collect();
        let rx = loopback.rx_imbalance.matrix().apply(&rx);
        let rx = self.rx.correction(0.5 * TERM_MAX).apply(&rx);
        let nco = self.rx.nco_hz(channel, clock);
        rx.into_iter()
            .filter(|(hz, _)| (hz + nco).abs() < AT_DC_HZ)
            .fold(Complex::ZERO, |sum, (_, z)| sum.add(z))
    }
}

/// The low `width` bits of `bits` as a two's-complement number.
fn sign_extend(bits: u32, width: u32) -> i32 {
    let unused = 32 - width;
    i32::from_ne_bytes((bits << unused).to_ne_bytes()) >> unused
}

impl Imbalance {
    /// As a matrix on `(I, Q)`.
    fn matrix(self) -> Matrix {
        Matrix([[1.0 + self.gain, 0.0], [self.phase.sin(), self.phase.cos()]])
    }
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
    /// No change.
    const IDENTITY: Self = Self([[1.0, 0.0], [0.0, 1.0]]);

    /// Applies the matrix to tones at their frequencies in Hz: a tone `z` at `f` becomes
    /// `α·z` at `f` and its image `β·conj(z)` at `−f`, with
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
        let out = m.apply(&[(0.0, Complex::new(1.0, 1.0))]);
        let sum = out.iter().fold(Complex::ZERO, |s, &(_, z)| s.add(z));
        assert!((sum.re - 3.0).abs() < 1e-12 && (sum.im - 7.0).abs() < 1e-12);
    }

    #[test]
    fn a_symmetric_correction_cancels_the_image_of_an_opposite_skew() {
        // A Q lean of 2° and its correction at half the angle on each axis, as the
        // calibration finds it.
        let skew = Imbalance {
            gain: 0.0,
            phase: 2_f64.to_radians(),
        }
        .matrix();
        let half = -1_f64.to_radians();
        let (s, c) = half.sin_cos();
        let correct = Matrix([[c, s], [s, c]]);
        let tone = [(1e6, Complex::new(1.0, 0.0))];
        let image = |tones: &[(f64, Complex)]| {
            let z = tones
                .iter()
                .filter(|(hz, _)| *hz < 0.0)
                .fold(Complex::ZERO, |sum, &(_, z)| sum.add(z));
            z.re.hypot(z.im)
        };
        let skewed = skew.apply(&tone);
        assert!(image(&skewed) > 0.01);
        assert!(image(&correct.apply(&skewed)) < 1e-3);
    }

    #[test]
    fn the_estimator_counts_a_measurement_per_millisecond_while_running() {
        let mut chains = Chains::default();
        let lms = Lms6002d::new();
        let select = |chains: &mut Chains, at| {
            chains.on_write(REG_PHY_RX, ESTIMATOR_READ | 6 << 24, at);
            chains.read(REG_PHY_RX, at, &lms)
        };
        chains.on_write(REG_PHY_RX, 6 << 24 | 1, 0);
        chains.on_write(REG_PHY_RX, 6 << 24, 1);
        assert_eq!(select(&mut chains, 1), Some(0));
        assert_eq!(select(&mut chains, 1000), Some(0));
        assert_eq!(select(&mut chains, 1001), Some(1));
        chains.on_write(REG_PHY_RX, 6 << 24 | 1, 5000);
        assert_eq!(select(&mut chains, 9000), Some(4), "held");
    }
}
