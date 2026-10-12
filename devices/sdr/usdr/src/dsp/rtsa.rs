//! Real-time spectrum analysis: a decaying per-bin histogram of power levels, the display
//! behind a persistence ("waterfall density") spectrum (`xdsp/rtsa_functions.{c,h}` and
//! the generic templates `rtsa_update_u16_generic.{t,inc}`,
//! `rtsa_update_hwi16_u16_generic.t`).
//!
//! Each FFT bin owns a column of `depth` 16-bit cells, one per power step from the upper
//! bound down ([`RtsaSettings::divs_per_db`] steps per dB). Every update first discharges
//! each cell of the bins it covers, then charges the cell its new power falls in, so cells
//! hit often stay bright and the rest fade.
//!
//! The arithmetic is libusdr's generic `uint16_t` kernels, bit for bit, including the
//! double-precision charge and the exponential-free decay by shifts (checked against the
//! compiled C by the oracle's `rtsa_fm_parity` test). A level rounds to the nearest cell,
//! as libusdr's generic kernels do outside ARM; on ARM (`WVLT_NEON`) even its generic
//! kernels truncate instead. Where the C is undefined, this module
//! is defined instead:
//!
//! - A charge step of 65536 or more converts out of `uint16_t` range in C; x86-64 builds
//!   in practice keep its low bits, so cells flicker. Here the step saturates and the cell
//!   charges fully. libusdr's suggested settings (`raise_coef` 24 or more at 100 steps per
//!   dB) take this path whenever the charge rate exceeds about 0.63.
//! - A decay shift of 32 or more (`charging_frame * decay_coef` near 2^32) is undefined in
//!   C; here it shifts everything out.
//! - libusdr indexes past a column when a power level rounds above `depth - 1` in float
//!   (only for `depth` beyond 2^24); here it charges the last cell.
//! - A NaN level, from a non-finite `scale` or `corr`, converts to an index undefinedly in
//!   C; here it charges cell 0.
//!
//! [`RtsaSettings::new`] rejects settings C would divide by zero with, wrap or assert on.
//! libusdr's floating-point histogram (`RTSA_FP32`, `rtsa_update_f32_generic.t`) and
//! antialiasing variant are compiled out of the reference and not ported; neither is the
//! SIMD-only `rtsa_fill_hwi16_consts`, whose results the generic kernels ignore.

use core::f64::consts::E;
use core::ops::Range;

use num_complex::Complex32;
use thiserror::Error;

use super::log2::polylog2;

/// A full cell (`MAX_RTSA_PWR`).
const FULL: u16 = u16::MAX;
/// `CHARGE_NORM_COEF`: the charge target, in double as the C macro expands.
const CHARGE_TARGET: f64 = 65535.0 * E / (E - 1.0);
/// `(unsigned)DISCHARGE_NORM_COEF`: the fixed part of each discharge, before shifting.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "C truncates the positive double 38139.48 to unsigned"
)]
const DISCHARGE_BASE: u32 = (65535.0 / (E - 1.0)) as u32;
/// Bytes libusdr's generic build aligns a column to (`cpu_vcap_align(OPT_GENERIC)`).
const GENERIC_ALIGN_BYTES: u32 = 8;

/// Why RTSA settings or an update were rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum RtsaError {
    /// The upper bound is not above the lower one (C asserts).
    #[error("upper power bound {upper} dB is not above lower bound {lower} dB")]
    Bounds {
        /// Upper bound, dB.
        upper: i16,
        /// Lower bound, dB.
        lower: i16,
    },
    /// A count C divides by, or that yields a zero decay period, is zero.
    #[error("{0} must be at least 1")]
    Zero(&'static str),
    /// The column depth or decay period overflows libusdr's 32-bit arithmetic, or the
    /// adjusted lower bound its 16 bits.
    #[error("{0} overflows 32 bits")]
    Overflow(&'static str),
    /// The histogram has more cells than memory can address.
    #[error("{bins} bins of depth {depth} do not fit in memory")]
    Size {
        /// FFT bins.
        bins: usize,
        /// Cells per bin.
        depth: u32,
    },
    /// An update's bins reach past the histogram.
    #[error("bins {first}..{end} reach past the {bins}-bin histogram")]
    BinRange {
        /// First bin updated.
        first: usize,
        /// One past the last bin updated.
        end: usize,
        /// Bins in the histogram.
        bins: usize,
    },
}

/// RTSA display settings (`fft_rtsa_settings_t`), with the column depth computed and the
/// lower bound adjusted as `rtsa_calc_depth` does for libusdr's generic (scalar) kernels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtsaSettings {
    /// Top of the display, dB (`upper_pwr_bound`).
    upper: i16,
    /// Bottom of the display after rounding the depth up, dB (`lower_pwr_bound`).
    lower: i16,
    /// Cells per dB (`divs_for_dB`).
    divs_per_db: u32,
    /// Cells per bin (`rtsa_depth`).
    depth: u32,
    /// FFTs for one full charge or discharge (`charging_frame`).
    charging_frame: u32,
    /// Charge speed multiplier (`raise_coef`).
    raise: u32,
    /// Discharge speed divider (`decay_coef`).
    decay: u32,
}

impl RtsaSettings {
    /// Settings for a display from `upper` down to `lower` dB in `divs_per_db` cells per
    /// dB, as `rtsa_calc_depth` completes them.
    ///
    /// The depth is `(upper - lower) * divs_per_db` rounded up, through single precision as
    /// in C, to a multiple of the generic build's alignment (four cells), and `lower` then
    /// moves down to match (C truncates `depth / divs_per_db`). `charging_frame` is the FFT
    /// count for one full charge or discharge (libusdr suggests a power of two), `raise` the
    /// charge multiplier (24 or more suggested; see the module docs on saturation) and
    /// `decay` the discharge divider (1 suggested).
    ///
    /// # Errors
    ///
    /// [`RtsaError::Bounds`] unless `upper > lower`; [`RtsaError::Zero`] for a zero
    /// `divs_per_db`, `charging_frame` or `decay`; [`RtsaError::Overflow`] when the depth or
    /// `charging_frame * decay` exceeds 32 bits, or the rounded-down bottom leaves `i16`
    /// (C wraps it).
    pub fn new(
        upper: i16,
        lower: i16,
        divs_per_db: u32,
        charging_frame: u32,
        raise: u32,
        decay: u32,
    ) -> Result<Self, RtsaError> {
        if upper <= lower {
            return Err(RtsaError::Bounds { upper, lower });
        }
        for (name, value) in [
            ("divs_per_db", divs_per_db),
            ("charging_frame", charging_frame),
            ("decay", decay),
        ] {
            if value == 0 {
                return Err(RtsaError::Zero(name));
            }
        }
        charging_frame
            .checked_mul(decay)
            .ok_or(RtsaError::Overflow("charging_frame * decay"))?;
        let span = (i32::from(upper) - i32::from(lower)).unsigned_abs();
        let raw = span
            .checked_mul(divs_per_db)
            .ok_or(RtsaError::Overflow("depth"))?;
        let depth = aligned_depth(raw).ok_or(RtsaError::Overflow("depth"))?;
        // C: upper - depth / divs in unsigned arithmetic, stored to int16_t. Where that
        // leaves int16_t, C wraps the bottom above the top; rejected instead.
        let lower = i16::try_from(i64::from(upper) - i64::from(depth / divs_per_db))
            .map_err(|_| RtsaError::Overflow("lower bound"))?;
        Ok(Self {
            upper,
            lower,
            divs_per_db,
            depth,
            charging_frame,
            raise,
            decay,
        })
    }

    /// Top of the display, dB.
    #[must_use]
    pub fn upper(&self) -> i16 {
        self.upper
    }

    /// Bottom of the display, dB, after the depth was rounded up.
    #[must_use]
    pub fn lower(&self) -> i16 {
        self.lower
    }

    /// Cells per dB.
    #[must_use]
    pub fn divs_per_db(&self) -> u32 {
        self.divs_per_db
    }

    /// Cells per bin.
    #[must_use]
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// FFTs for one full charge or discharge.
    #[must_use]
    pub fn charging_frame(&self) -> u32 {
        self.charging_frame
    }

    /// Charge speed multiplier.
    #[must_use]
    pub fn raise(&self) -> u32 {
        self.raise
    }

    /// Discharge speed divider.
    #[must_use]
    pub fn decay(&self) -> u32 {
        self.decay
    }

    /// The charge rate in cells: `raise * divs_per_db / charging_frame`, in single
    /// precision.
    #[expect(
        clippy::cast_precision_loss,
        reason = "C converts each unsigned operand to float"
    )]
    fn charge_rate(&self) -> f32 {
        self.raise as f32 * self.divs_per_db as f32 / self.charging_frame as f32
    }

    /// The discharge shift: `log2(charging_frame * decay)`, approximated and rounded.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "C converts the product to float and the rounded double to unsigned; \
                  the product is at least 1, so the double is non-negative"
    )]
    fn decay_shift(&self) -> u32 {
        let period = self.charging_frame * self.decay;
        (f64::from(polylog2(period as f32)) + 0.5) as u32
    }
}

/// `rtsa_calc_depth`'s rounding for the generic build: `d * ceil((float)depth / (float)d)`
/// with `d` cells per alignment unit, stored to unsigned. `None` if that leaves 32 bits.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "C's float quotient, double product and unsigned store, range-checked"
)]
fn aligned_depth(depth: u32) -> Option<u32> {
    let cells = GENERIC_ALIGN_BYTES / 2;
    let units = f64::from((depth as f32 / cells as f32).ceil());
    let aligned = f64::from(cells) * units;
    (aligned <= f64::from(u32::MAX)).then_some(aligned as u32)
}

/// An RTSA histogram (`fft_rtsa_data_t`), updated by libusdr's generic kernels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rtsa {
    /// The display settings.
    settings: RtsaSettings,
    /// FFT bins (`fft_size`).
    bins: usize,
    /// `depth` cells per bin, bin-major (`pwr`).
    cells: Vec<u16>,
}

impl Rtsa {
    /// An empty histogram for `bins` FFT bins (`rtsa_init`).
    ///
    /// # Errors
    ///
    /// [`RtsaError::Size`] when `bins * depth` cells cannot be addressed.
    pub fn new(settings: RtsaSettings, bins: usize) -> Result<Self, RtsaError> {
        let depth = settings.depth;
        let len = usize::try_from(depth)
            .ok()
            .and_then(|depth| depth.checked_mul(bins))
            .ok_or(RtsaError::Size { bins, depth })?;
        Ok(Self {
            settings,
            bins,
            cells: vec![0; len],
        })
    }

    /// The display settings.
    #[must_use]
    pub fn settings(&self) -> &RtsaSettings {
        &self.settings
    }

    /// FFT bins in the histogram.
    #[must_use]
    pub fn bins(&self) -> usize {
        self.bins
    }

    /// The histogram: `depth` cells per bin, bin after bin; cell `j` of a bin is
    /// `j / divs_per_db` dB below the upper bound. Cells range from 0 (dark) to
    /// `u16::MAX` (fully charged).
    #[must_use]
    pub fn cells(&self) -> &[u16] {
        &self.cells
    }

    /// Empties the histogram (`rtsa_init`).
    pub fn reset(&mut self) {
        self.cells.fill(0);
    }

    /// Folds in one spectrum of complex FFT bins starting at bin `first`
    /// (`rtsa_update` with `diap = {first, first + spectrum.len()}`).
    ///
    /// Each bin's level is `scale * log2(re² + im² + floor) + corr` dB, with libusdr's
    /// approximate logarithm.
    ///
    /// # Errors
    ///
    /// [`RtsaError::BinRange`] when the spectrum reaches past the histogram; nothing is
    /// updated then.
    pub fn update(
        &mut self,
        first: usize,
        spectrum: &[Complex32],
        scale: f32,
        floor: f32,
        corr: f32,
    ) -> Result<(), RtsaError> {
        let bins = self.bin_range(first, spectrum.len())?;
        self.update_with(bins, spectrum, |bin| {
            scale * polylog2(bin.re * bin.re + bin.im * bin.im + floor) + corr
        });
        Ok(())
    }

    /// Folds in one spectrum the gateware delivered as fixed-point log powers, starting at
    /// bin `first` (`rtsa_update_hwi16`). Each bin's level is `scale * value + corr` dB.
    ///
    /// # Errors
    ///
    /// [`RtsaError::BinRange`] when the spectrum reaches past the histogram; nothing is
    /// updated then.
    pub fn update_hw(
        &mut self,
        first: usize,
        spectrum: &[u16],
        scale: f32,
        corr: f32,
    ) -> Result<(), RtsaError> {
        let bins = self.bin_range(first, spectrum.len())?;
        self.update_with(bins, spectrum, |&bin| scale * f32::from(bin) + corr);
        Ok(())
    }

    /// The histogram bins `len` spectrum bins from `first` cover, if they all exist.
    fn bin_range(&self, first: usize, len: usize) -> Result<Range<usize>, RtsaError> {
        first
            .checked_add(len)
            .filter(|&end| end <= self.bins)
            .map(|end| first..end)
            .ok_or(RtsaError::BinRange {
                first,
                end: first.saturating_add(len),
                bins: self.bins,
            })
    }

    /// Cells per bin, as an index.
    fn depth(&self) -> usize {
        usize::try_from(self.settings.depth).expect("invariant: `new` addressed the cells")
    }

    /// The shared update loop: discharge each covered column, then charge the cell its
    /// level (from `level`) falls in.
    fn update_with<T>(&mut self, bins: Range<usize>, spectrum: &[T], level: impl Fn(&T) -> f32) {
        let settings = self.settings;
        let depth = self.depth();
        let charge_rate = settings.charge_rate();
        let shift = settings.decay_shift();
        #[expect(
            clippy::cast_precision_loss,
            reason = "C converts the unsigned index to float"
        )]
        let deepest = settings.depth.wrapping_sub(1) as f32 - 0.5;
        for (bin, value) in bins.zip(spectrum) {
            let column = &mut self.cells[bin * depth..(bin + 1) * depth];
            for cell in column.iter_mut() {
                discharge(cell, shift);
            }
            let mut p = level(value) - f32::from(settings.upper);
            p = p.abs();
            #[expect(
                clippy::cast_precision_loss,
                reason = "C converts the unsigned step count to float"
            )]
            let steps = settings.divs_per_db as f32;
            p *= steps;
            if p > deepest {
                p = deepest;
            }
            // C: (unsigned)(p + 0.5f), truncating; NaN (undefined in C) lands in cell 0.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "p is non-negative and at most depth - 1 after clamping"
            )]
            let index = (p + 0.5) as usize;
            charge(&mut column[index.min(depth - 1)], charge_rate);
        }
    }
}

/// `rtsa_charge_u16`: moves a cell towards the charge target by `rate`, in double, the
/// step truncated to 16 bits (saturating where C is undefined), and caps it full.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "C converts the non-negative double step to uint16_t; beyond it, saturate"
)]
fn charge(cell: &mut u16, rate: f32) {
    let step = ((f64::from(-f32::from(*cell)) + CHARGE_TARGET) * f64::from(rate)) as u16;
    *cell = cell.checked_add(step).unwrap_or(FULL);
}

/// `rtsa_discharge_u16`: lowers a non-empty cell by its value and the fixed discharge,
/// both shifted down by `shift`, stopping at zero.
fn discharge(cell: &mut u16, shift: u32) {
    if *cell == 0 {
        return;
    }
    let delta = u32::from(*cell).checked_shr(shift).unwrap_or(0)
        + DISCHARGE_BASE.checked_shr(shift).unwrap_or(0);
    *cell =
        u16::try_from(u32::from(*cell).saturating_sub(delta)).expect("invariant: at most the cell");
}

#[cfg(test)]
mod tests {
    use num_complex::Complex32;

    use super::{Rtsa, RtsaError, RtsaSettings};

    #[test]
    fn depth_rounds_up_to_whole_alignment_units() {
        let settings = RtsaSettings::new(0, -9, 3, 16, 1, 1).expect("valid");
        // 27 cells round up to 28; the bottom moves down to 0 - 28 / 3 = -9.
        assert_eq!((settings.depth(), settings.lower()), (28, -9));
        let settings = RtsaSettings::new(10, -110, 100, 16, 24, 1).expect("valid");
        assert_eq!((settings.depth(), settings.lower()), (12_000, -110));
        let settings = RtsaSettings::new(0, -5, 1, 16, 24, 1).expect("valid");
        assert_eq!((settings.depth(), settings.lower()), (8, -8));
    }

    #[test]
    fn settings_c_cannot_compute_are_rejected() {
        assert_eq!(
            RtsaSettings::new(0, 0, 1, 1, 1, 1),
            Err(RtsaError::Bounds { upper: 0, lower: 0 })
        );
        assert_eq!(
            RtsaSettings::new(0, -1, 0, 1, 1, 1),
            Err(RtsaError::Zero("divs_per_db"))
        );
        assert_eq!(
            RtsaSettings::new(0, -1, 1, 0, 1, 1),
            Err(RtsaError::Zero("charging_frame"))
        );
        assert_eq!(
            RtsaSettings::new(0, -1, 1, 1 << 16, 1, 1 << 16),
            Err(RtsaError::Overflow("charging_frame * decay"))
        );
        assert_eq!(
            RtsaSettings::new(i16::MAX, i16::MIN, u32::MAX / 1000, 1, 1, 1),
            Err(RtsaError::Overflow("depth"))
        );
        // Three dB from the floor rounds up to four cells: C would wrap the bottom to 32767.
        assert_eq!(
            RtsaSettings::new(-32765, -32768, 1, 1, 1, 1),
            Err(RtsaError::Overflow("lower bound"))
        );
    }

    #[test]
    fn a_repeated_tone_charges_one_cell_and_the_rest_stay_dark() {
        let settings = RtsaSettings::new(0, -40, 1, 4, 1, 1).expect("valid");
        let mut rtsa = Rtsa::new(settings, 2).expect("small");
        // Bin power 2^-10: 10 * log2 / log2(10) ≈ -30.1 dB with scale 10*log10(2).
        let scale = 10.0 * 2.0_f32.log10();
        let tone = [Complex32::new(2.0_f32.powi(-5), 0.0); 2];
        for _ in 0..64 {
            rtsa.update(0, &tone, scale, 0.0, 0.0).expect("in range");
        }
        let depth = 40;
        for bin in 0..2 {
            let column = &rtsa.cells()[bin * depth..(bin + 1) * depth];
            let lit: Vec<usize> = (0..depth).filter(|&j| column[j] != 0).collect();
            assert_eq!(lit, [30], "bin {bin} lights only its level");
            assert!(column[30] > u16::MAX / 2, "the hit cell is bright");
        }
    }

    #[test]
    fn unused_cells_fade_to_dark() {
        let settings = RtsaSettings::new(0, -8, 1, 2, 1, 1).expect("valid");
        let mut rtsa = Rtsa::new(settings, 1).expect("small");
        rtsa.update_hw(0, &[3], 1.0, 0.0).expect("in range");
        assert_ne!(rtsa.cells()[3], 0);
        for _ in 0..64 {
            rtsa.update_hw(0, &[6], 1.0, 0.0).expect("in range");
        }
        assert_eq!(rtsa.cells()[3], 0, "the old level fades");
        assert_ne!(rtsa.cells()[6], 0, "the new level is lit");
    }

    #[test]
    fn oversized_charge_steps_saturate_to_full() {
        // raise 24 at 100 cells per dB over 16 FFTs: a rate of 150, a step of millions.
        let settings = RtsaSettings::new(0, -1, 100, 16, 24, 1).expect("valid");
        let mut rtsa = Rtsa::new(settings, 1).expect("small");
        rtsa.update_hw(0, &[0], 1.0, -0.5).expect("in range");
        assert_eq!(rtsa.cells()[50], u16::MAX);
    }

    #[test]
    fn spectra_past_the_histogram_are_rejected_untouched() {
        let settings = RtsaSettings::new(0, -8, 1, 2, 1, 1).expect("valid");
        let mut rtsa = Rtsa::new(settings, 4).expect("small");
        assert_eq!(
            rtsa.update_hw(3, &[0, 0], 1.0, 0.0),
            Err(RtsaError::BinRange {
                first: 3,
                end: 5,
                bins: 4
            })
        );
        assert!(rtsa.cells().iter().all(|&cell| cell == 0));
    }
}
