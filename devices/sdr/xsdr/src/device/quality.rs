//! The TX quality check a sample rate runs while TX streams: libusdr's
//! `xsdr_check_rxtx_quality(d, 1)` (`device/m2_lm7_1/xsdr_ctrl.c`), which
//! `xsdr_set_samplerate_ex` calls after the self-tests when TX runs.
//!
//! The TX PHY's generator plays a 64-sample tone from its A/B-I/Q table into the
//! LMS7002M, whose baseband loopback brings it back to the RX PHY. Its capture, averaged
//! over 256 captures, is fitted with libusdr's own tone model (`sdr-cal`'s `fit`,
//! libusdr's `cal/cal_filt.c`), and channel A's spread, peak error and fit error make the
//! score. Until the score is good, up to eight attempts, the TX processors' clock is
//! toggled, and from the third attempt on delayed a step more. Then the generator stops
//! and the running channels' baseband filters are set again for the sample rates.
//!
//! Host-side, as its arithmetic needs floating-point `sin` and `cos`; blocking code over
//! the bus, as the streams'.
//!
//! Divergences: libusdr divides the rates by interpolation and decimation factors it sets
//! only on gateware with a DUC/DDC (`s_tx_int`, `s_rx_dec`), 0 elsewhere (and on a DSP
//! chain without a rate), so its integer division then traps (SIGFPE) after the generator
//! stops; this divides by 1 there.
//! A capture read that fails ends the check with its error, where libusdr ignores it and
//! fits what the buffer held; a capture word reporting busy forever ends it with
//! [`Error::CaptureBusy`] after [`CAPTURE_POLLS`] reads, where libusdr reads on.

use core::f64::consts::PI;
use std::time::Duration;

use hal::FpgaAccess;
use lms7002m::ctrl::{ChipCall, Direction};
use num_complex::Complex32;
use sdr_cal::fit;

use super::{Device, run};
use crate::error::Error;
use crate::portable::fpga::{REG_CFG_PHY_0, RxPhyBank, TxPhy};
use crate::portable::rate::RateState;
use crate::portable::step::Step;

/// Samples a capture holds per channel (`CAPTURE_SIZE`).
pub(super) const CAPTURE_SIZE: usize = 128;
/// Captures averaged (`1 << acc`).
const CAPTURES: u32 = 1 << 8;
/// The averaged sums' shift back to 12 bits (`4 + acc`).
const CAPTURE_SHIFT: u32 = 4 + 8;
/// Reads of one capture word while it reports busy (bits 31:24) before giving up.
pub(super) const CAPTURE_POLLS: u32 = 1000;
/// The tone's amplitude in LSBs (`xsdr_phy_load_sincos_64(d, 2000)`).
const TONE_AMPLITUDE: f32 = 2000.0;
/// The tone's period in samples, and the table's length (`period`, `P`).
const TONE_PERIOD: u32 = 64;
/// Scores below this pass (`score < 0.025`).
const GOOD_SCORE: f64 = 0.025;
/// Attempts before giving up.
const ATTEMPTS: u8 = 8;

impl Device {
    /// `xsdr_check_rxtx_quality(d, fix)`: the tone loaded and played through the baseband
    /// loopback, scored and, with `fix` until good, the TX processors' clock adjusted; then
    /// the generator stopped and the running channels' baseband filters set for the rates.
    pub(super) fn check_quality(&mut self, fix: bool) -> Result<(), Error> {
        self.load_tone()?;
        self.tx_generator(true, false)?;
        self.loop_baseband()?;
        for attempt in 0..ATTEMPTS {
            let [a, _] = self.capture_both()?;
            if f64::from(tx_score(&a)) < GOOD_SCORE || !fix {
                break;
            }
            let lml = self.control.lml();
            self.run_chip(&[ChipCall::LimelightTspClock(lml, false)])?;
            self.bus.sleep(Duration::from_micros(1));
            self.run_chip(&[ChipCall::LimelightTspClock(lml, true)])?;
            self.bus.sleep(Duration::from_micros(100));
            if attempt > 1 {
                let op = self.control.delay_tx_tsp();
                self.run_control(op)?;
            }
        }
        self.tx_generator(false, false)?;
        // libusdr's `s_txrate / s_tx_int` and `s_rxrate / s_rx_dec`: see the module docs.
        let (rx_rate, tx_rate) = host_rates(self.rate);
        let tx = self.control.update_bandwidth(Direction::Tx, tx_rate);
        self.run_control(tx)?;
        let rx = self.control.update_bandwidth(Direction::Rx, rx_rate);
        self.run_control(rx)
    }

    /// `xsdr_phy_load_sincos_64(d, 2000)`: one turn of the tone in 64 words, each pushed
    /// to both channels' tables, Q (the sine) in bits 23:12 and I (the cosine) in 11:0.
    fn load_tone(&mut self) -> Result<(), Error> {
        for step in 0..TONE_PERIOD {
            let word = tone_word(step);
            run(
                &mut *self.bus,
                &mut Step::fpga(TxPhy::IqAbPushA.write(word)),
            )?;
            run(
                &mut *self.bus,
                &mut Step::fpga(TxPhy::IqAbPushB.write(word)),
            )?;
        }
        Ok(())
    }

    /// `xsdr_phy_en_lfsr_generator_mimo(d, run, lfsr)`: the generator stopped, 1 µs, its
    /// pattern (the LFSR, or the A/B-I/Q table), then run or not.
    fn tx_generator(&mut self, enable: bool, lfsr: bool) -> Result<(), Error> {
        run(
            &mut *self.bus,
            &mut Step::fpga(TxPhy::LfsrGenerator.write(0)),
        )?;
        self.bus.sleep(Duration::from_micros(1));
        let table = TxPhy::IqAbGenerator.write(u32::from(!lfsr));
        run(&mut *self.bus, &mut Step::fpga(table))?;
        let running = TxPhy::LfsrGenerator.write(u32::from(enable));
        run(&mut *self.bus, &mut Step::fpga(running))
    }

    /// `xsdr_capture_rxiq`: 256 captures of both channels' 128 samples, each started and
    /// stopped, then read word by word (A's, then B's); each sample's I and Q averaged
    /// and packed back as 12-bit fields, A's words first.
    pub(super) fn capture_both(&mut self) -> Result<[[u32; CAPTURE_SIZE]; 2], Error> {
        let mut sums = [[[0_i32; 2]; CAPTURE_SIZE]; 2];
        for _ in 0..CAPTURES {
            self.capture_control(1)?;
            self.bus.sleep(Duration::from_micros(10));
            self.capture_control(0)?;
            for (channel, sums) in sums.iter_mut().enumerate() {
                for (index, sum) in sums.iter_mut().enumerate() {
                    let word = self.capture_word(channel == 1, index)?;
                    // `(int16_t)(uint16_t)(field << 4)`.
                    let lane = |field: u32| {
                        let bits = u16::try_from((field & 0xfff) << 4).expect("16 bits");
                        i32::from(i16::from_ne_bytes(bits.to_ne_bytes()))
                    };
                    sum[0] += lane(word);
                    sum[1] += lane(word >> 12);
                }
            }
        }
        let field = |sum: i32| u32::from_ne_bytes((sum >> CAPTURE_SHIFT).to_ne_bytes()) & 0xfff;
        Ok(sums.map(|channel| channel.map(|[i, q]| field(i) | field(q) << 12)))
    }

    /// `xsdr_phy_capture_start(d, value)`.
    fn capture_control(&mut self, value: u16) -> Result<(), Error> {
        let start = RxPhyBank::Capture.access(true, 0, value);
        run(&mut *self.bus, &mut Step::fpga(start))
    }

    /// `xsdr_phy_capture_get_item`, again while the word reports busy (bits 31:24).
    fn capture_word(&mut self, channel_b: bool, index: usize) -> Result<u32, Error> {
        let address =
            u8::try_from(index & 0x7f).expect("7 bits") | if channel_b { 0x80 } else { 0 };
        for _ in 0..CAPTURE_POLLS {
            let select = RxPhyBank::Capture.access(false, address, 0);
            run(&mut *self.bus, &mut Step::fpga(select))?;
            let mut read = Step::fpga(FpgaAccess::Read { reg: REG_CFG_PHY_0 });
            run(&mut *self.bus, &mut read)?;
            let word = read.value().unwrap_or_default();
            if word >> 24 == 0 {
                return Ok(word);
            }
        }
        Err(Error::CaptureBusy)
    }
}

/// The RX and TX rates before the FPGA's DSP chains: the LMS7002M's rates (`s_rxrate`,
/// `s_txrate`) divided by the chains' factors as last loaded, or (no DUC/DDC, where libusdr
/// divides by 0 and traps) by 1.
pub(super) fn host_rates(state: RateState) -> (u32, u32) {
    let divide = |rate: u32, factor: u32| rate.checked_div(factor).unwrap_or(rate);
    (
        divide(state.rx, state.rx_decimation),
        divide(state.tx, state.tx_interpolation),
    )
}

/// Table word `step` of the tone: `amp * sin` and `amp * cos` of `2 * M_PI * step / 64`
/// (the phase computed in double, stored to float; the products truncated to `int32_t`),
/// masked to 12 bits each.
#[expect(
    clippy::cast_possible_truncation,
    reason = "libusdr stores the double phase to float and truncates the products to int32_t"
)]
fn tone_word(step: u32) -> u32 {
    let phase = (2.0 * PI * f64::from(step) / f64::from(TONE_PERIOD)) as f32;
    let (sin, cos) = phase.sin_cos();
    let bits = |value: f32| u32::from_ne_bytes(((TONE_AMPLITUDE * value) as i32).to_ne_bytes());
    (bits(sin) & 0xfff) << 12 | bits(cos) & 0xfff
}

/// Channel A's score (`xsdr_check_tx_quality`'s `norm_a`): the capture's DC removed, its
/// magnitudes' spread, its largest component error from the best-fitting tone of the
/// period and its fit error, each relative to its mean magnitude, combined as a vector
/// length. NaN for a silent capture, which libusdr's comparison fails too.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "libusdr's float arithmetic: a double square root stored to float, the count and period as float"
)]
fn tx_score(words: &[u32; CAPTURE_SIZE]) -> f32 {
    let mut samples = [Complex32::new(0.0, 0.0); CAPTURE_SIZE];
    fit::from_dump12(words, &mut samples);
    fit::remove_dc(&mut samples);
    let (amp, spread) = fit::magnitude_mean_std(&samples);
    let period = TONE_PERIOD as f32;
    let best = fit::best_phase(&samples, amp, period);
    let peak = fit::max_component_error(&samples, amp, period, best.phase);
    let relative = [
        spread / amp,
        peak / amp,
        best.error / CAPTURE_SIZE as f32 / amp,
    ];
    let squares: f32 = relative.iter().map(|value| value * value).sum();
    f64::from(squares).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tone_table_starts_on_the_cosine_and_turns_once() {
        // Step 0: sin 0 = 0 in bits 23:12, cos 0 * 2000 = 2000 in bits 11:0.
        assert_eq!(tone_word(0), 2000);
        // Step 16 (a quarter turn): sin 2000, cos ~0.
        assert_eq!(tone_word(16) >> 12, 2000);
        // Step 32 (half a turn): cos -2000, as 12-bit two's complement.
        assert_eq!(tone_word(32) & 0xfff, 0x1000 - 2000);
    }

    #[test]
    fn a_clean_tone_read_newest_first_scores_well_and_silence_does_not() {
        // The table played newest first: sample n is word 63 - n of the tone.
        let words: [u32; CAPTURE_SIZE] = core::array::from_fn(|n| {
            let n = u32::try_from(n).expect("128 samples");
            tone_word(TONE_PERIOD - 1 - n % TONE_PERIOD)
        });
        let score = tx_score(&words);
        assert!(f64::from(score) < GOOD_SCORE, "score {score}");
        assert!(tx_score(&[0; CAPTURE_SIZE]).is_nan(), "a silent capture");
    }
}
