//! Disciplining a reference oscillator to a pulse-per-second input: the control loop of
//! libusdr's `usdr_dm_gpsdo` tool (`tools/usdr_dm_gpsdo.c`), as a sans-I/O machine.
//!
//! The gateware counts a clock between PPS pulses and reports the count with a 4-bit pulse
//! counter ([`PpsMeasurement`]). The tool polls that every 100 ms; when the pulse counter
//! moves, it compares the count with the frequency expected, runs the error through a PID
//! controller and steps the oscillator's 16-bit trim DAC by the result. [`OscillatorDiscipline`]
//! is that loop without the polling or the I/O: the caller reads the measurement, hands it
//! over with the time, and writes the DAC code the step asks for, confirming it with
//! [`OscillatorDiscipline::dac_written`]. The constants are the tool's: gains 0.75, 0.02
//! and 0.0075 per second, the integral held to ±10⁶, the output to ±10⁻³, an oscillator
//! that pulls ±7 ppm across the DAC's range, steps of at most 2000 codes from a start at
//! mid-scale (32767), and holdover after 2 s without a pulse.
//!
//! On the uSDR, the tool sets the board up first (an external 25 MHz reference, the sample
//! rate it expects, 4 MS/s by default, and a running `ci16` RX stream), then reads
//! `/dm/sensor/freqpps`, which the uSDR does not have at the pinned commit: only the XSDR
//! does, as GPI word 24 (`IGPI_CLK1PPS`). The tool's commented-out alternative is
//! `/dm/sdr/0/clkmeas`, the uSDR's GPI word 24
//! ([`Device::clock_measurement`](crate::Device::clock_measurement)), which the uSDR's
//! header names `IGPI_USBC` while naming word 28 `IGPI_CLK1PPS`. Which word carries the
//! count on the uSDR, and in what layout, is not shown by the pinned source, so this
//! module takes a decoded measurement and leaves the read to the caller; the DAC write is
//! [`Device::set_oscillator_dac`](crate::Device::set_oscillator_dac).
//!
//! Divergences from the tool, each where it drives the oscillator to a rail:
//!
//! - It rejects a measurement more than 2% above the expected frequency but follows one
//!   any distance below it, so a count of 0 saturates the controller and moves the DAC
//!   2000 codes a pulse. A measurement more than 2% off either way is rejected here.
//! - It rounds a code below 0 to a `long` and converts it to `uint32_t`, which wraps it
//!   past the top, so a DAC at the bottom of its range jumps to full scale. Codes clamp to
//!   0 here.
//! - After a failed DAC write it steps from the error code as if it were the DAC's code.
//!   Here it steps from the last code confirmed written.
//!
//! Kept as the tool does: a rejected measurement leaves the pulse unconsumed, so each poll
//! measures again until one passes or the counter moves on; holdover is only reported.

use std::num::NonZeroU32;
use std::time::Duration;

/// Proportional gain (`KP`).
const KP: f64 = 0.75;
/// Integral gain (`KI`).
const KI: f64 = 0.02;
/// Derivative gain (`KD`).
const KD: f64 = 0.0075;
/// Seconds between corrections: one PPS period (`dt`).
const DT: f64 = 1.0;
/// Bound on the error integral (`INTEGRAL_MAX`).
const INTEGRAL_MAX: f64 = 1e6;
/// Bound on the controller's output, a fractional frequency (`OUTPUT_MAX`).
const OUTPUT_MAX: f64 = 1e-3;
/// The oscillator's pull either side of nominal across the DAC's range (`DEVIATION_MAX`).
const DEVIATION_MAX: f64 = 7e-6;
/// The DAC's top code: 16 bits (`dac_bits`, `max_value`).
const DAC_MAX: u16 = u16::MAX;
/// The DAC's starting code (`center`).
const DAC_CENTER: u16 = DAC_MAX >> 1;
/// The most codes one correction moves the DAC.
const MAX_STEP: f64 = 2000.0;
/// The largest error, either way, the loop acts on (the tool's `0.02`, one-sided there).
const MAX_ERROR: f64 = 0.02;
/// How long without a pulse before holdover (`2 * dt`).
const HOLDOVER: Duration = Duration::from_secs(2);

/// One reading of the gateware's PPS frequency counter, in the layout the tool decodes
/// (`get_pps_count`, `get_measured_freq`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpsMeasurement {
    /// Pulses seen, modulo 16: a change means a new count (bits 31:28).
    pub pulses: u8,
    /// Clock cycles counted over the last second (bits 27:0).
    pub count: u32,
}

impl PpsMeasurement {
    /// Decodes a counter word as the tool does. On the XSDR this is GPI word 24
    /// (`IGPI_CLK1PPS`); see the [module docs](self) for the uSDR.
    #[must_use]
    pub const fn from_word(word: u32) -> Self {
        Self {
            pulses: (word >> 28) as u8,
            count: word & 0x0fff_ffff,
        }
    }
}

/// libusdr's `PID` with the tool's gains, its time step fixed at one second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pid {
    /// The error summed over time, bounded (`integral`).
    integral: f64,
    /// The last error (`prev_error`).
    previous: f64,
}

impl Pid {
    /// One update (`pid_update`): the bounded sum of the proportional, integral and
    /// derivative terms, in C's order.
    fn update(&mut self, error: f64) -> f64 {
        let proportional = KP * error;
        self.integral = (self.integral + error * DT).clamp(-INTEGRAL_MAX, INTEGRAL_MAX);
        let integral = KI * self.integral;
        let derivative = KD * (error - self.previous) / DT;
        self.previous = error;
        (proportional + integral + derivative).clamp(-OUTPUT_MAX, OUTPUT_MAX)
    }
}

/// The tool's discipline loop: see the [module docs](self).
#[derive(Clone, Debug)]
pub struct OscillatorDiscipline {
    /// The frequency a measurement counts when the oscillator is exact, in Hz.
    expected: f64,
    /// Hz the counted clock moves per DAC code (`hz_per_step`).
    hz_per_code: f64,
    /// The controller.
    pid: Pid,
    /// The pulse counter of the last measurement acted on (`prev_pps_count`).
    pulses: u8,
    /// The code corrections step from: the last confirmed write, or mid-scale
    /// (`dac_offset`).
    code: u16,
    /// The code last confirmed written (`old_dac_value`); none yet.
    written: Option<u16>,
    /// When the last correction was made, or the loop started (`start_time`).
    last_correction: Duration,
    /// Whether holdover has been reported and not yet left.
    holdover: bool,
}

/// What one [`OscillatorDiscipline::measure`] did.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct DisciplineStep {
    /// What became of the measurement.
    pub outcome: Outcome,
    /// Whether holdover began or ended with this measurement. Not checked after a
    /// rejection, as in the tool.
    pub holdover: Option<Holdover>,
}

impl DisciplineStep {
    /// The DAC code to write, if this step asks for one.
    #[must_use]
    pub const fn dac_write(&self) -> Option<u16> {
        match self.outcome {
            Outcome::Corrected(Correction {
                dac, write: true, ..
            }) => Some(dac),
            _ => None,
        }
    }
}

/// What became of one measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// The pulse counter has not moved since the last correction: nothing to do.
    NoPulse,
    /// A new count more than 2% from the frequency expected, ignored; `error` is the
    /// fractional frequency error. The pulse stays unconsumed.
    Rejected {
        /// `(count - expected) / expected`.
        error: f64,
    },
    /// A new count, acted on.
    Corrected(Correction),
}

/// One correction: the controller's view of a new count and the DAC code it leads to.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Correction {
    /// The fractional frequency error, `(count - expected) / expected`.
    pub error: f64,
    /// The controller's output, a fractional frequency correction within ±10⁻³.
    pub output: f64,
    /// The correction in Hz of the counted clock: `-output × expected`.
    pub correction_hz: f64,
    /// DAC codes the correction asks for, before the ±2000 limit.
    pub steps: f64,
    /// The new DAC code: the code stepped from plus the limited steps, rounded half away
    /// from zero, within 0 to 65535.
    pub dac: u16,
    /// Whether the code differs from the last one written, so needs writing.
    pub write: bool,
}

/// A change of holdover state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holdover {
    /// More than 2 s since the last correction: the oscillator runs on its last code.
    Entered,
    /// A correction after holdover.
    Left,
}

impl OscillatorDiscipline {
    /// Starts the loop at `now` from `first`, whose pulse counter the next pulse must
    /// change, for a counted clock of `expected_hz` when the oscillator is exact (the tool
    /// counts the sample clock, so this is the sample rate). The DAC is taken to be at
    /// mid-scale and is not written until the first correction.
    #[must_use]
    pub fn new(expected_hz: NonZeroU32, first: PpsMeasurement, now: Duration) -> Self {
        let expected = f64::from(expected_hz.get());
        Self {
            expected,
            hz_per_code: 2.0 * DEVIATION_MAX * expected / f64::from(DAC_MAX),
            pid: Pid::default(),
            pulses: first.pulses,
            code: DAC_CENTER,
            written: None,
            last_correction: now,
            holdover: false,
        }
    }

    /// The code the next correction steps from.
    #[must_use]
    pub const fn dac_code(&self) -> u16 {
        self.code
    }

    /// Whether the loop is in holdover.
    #[must_use]
    pub const fn in_holdover(&self) -> bool {
        self.holdover
    }

    /// Takes one measurement made at `now` (the tool polls every 100 ms). A new pulse
    /// count within 2% of the expected frequency is corrected for; then, unless the
    /// measurement was rejected, holdover is checked.
    pub fn measure(&mut self, measurement: PpsMeasurement, now: Duration) -> DisciplineStep {
        let outcome = if measurement.pulses == self.pulses {
            Outcome::NoPulse
        } else {
            let error = (f64::from(measurement.count) - self.expected) / self.expected;
            if error.abs() > MAX_ERROR {
                return DisciplineStep {
                    outcome: Outcome::Rejected { error },
                    holdover: None,
                };
            }
            self.pulses = measurement.pulses;
            self.last_correction = now;
            Outcome::Corrected(self.correct(error))
        };
        DisciplineStep {
            outcome,
            holdover: self.check_holdover(now),
        }
    }

    /// Records that `code` was written to the DAC, so corrections step from it.
    pub fn dac_written(&mut self, code: u16) {
        self.code = code;
        self.written = Some(code);
    }

    /// The controller's correction for `error` and the code it leads to.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to the DAC's range first"
    )]
    fn correct(&mut self, error: f64) -> Correction {
        let output = self.pid.update(error);
        let correction_hz = -output * self.expected;
        let steps = correction_hz / self.hz_per_code;
        let limited = steps.clamp(-MAX_STEP, MAX_STEP);
        let dac = (f64::from(self.code) + limited)
            .round()
            .clamp(0.0, f64::from(DAC_MAX)) as u16;
        let write = self.written != Some(dac);
        if !write {
            self.code = dac;
        }
        Correction {
            error,
            output,
            correction_hz,
            steps,
            dac,
            write,
        }
    }

    /// Reports entering holdover more than 2 s after the last correction, and leaving it
    /// at the next.
    fn check_holdover(&mut self, now: Duration) -> Option<Holdover> {
        let lapsed = now.saturating_sub(self.last_correction) > HOLDOVER;
        (lapsed != self.holdover).then(|| {
            self.holdover = lapsed;
            if lapsed {
                Holdover::Entered
            } else {
                Holdover::Left
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::time::Duration;

    use super::{Holdover, OscillatorDiscipline, Outcome, PpsMeasurement};

    /// 4 MS/s, the tool's default.
    const EXPECTED: u32 = 4_000_000;

    /// A loop started at 0 with pulse counter 0.
    fn discipline() -> OscillatorDiscipline {
        let expected = NonZeroU32::new(EXPECTED).expect("nonzero");
        OscillatorDiscipline::new(expected, measurement(0, EXPECTED), Duration::ZERO)
    }

    /// A measurement of `count` with pulse counter `pulses`.
    fn measurement(pulses: u8, count: u32) -> PpsMeasurement {
        PpsMeasurement { pulses, count }
    }

    /// Milliseconds as a time.
    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn words_split_into_the_pulse_counter_and_the_count() {
        let word = 0xa000_0000 + 4_000_123;
        assert_eq!(PpsMeasurement::from_word(word), measurement(0xa, 4_000_123));
    }

    #[test]
    fn a_fast_oscillator_is_pulled_down_and_a_slow_one_up() {
        let mut fast = discipline();
        let step = fast.measure(measurement(1, EXPECTED + 40), ms(1000));
        let code = step.dac_write().expect("a new code to write");
        assert!(code < 32767, "a fast count lowers the code: {code}");

        let mut slow = discipline();
        let code = slow
            .measure(measurement(1, EXPECTED - 40), ms(1000))
            .dac_write()
            .expect("a new code to write");
        assert!(code > 32767, "a slow count raises the code: {code}");
    }

    #[test]
    fn an_unchanged_pulse_counter_does_nothing() {
        let mut loop_ = discipline();
        let step = loop_.measure(measurement(0, EXPECTED + 400), ms(100));
        assert_eq!(step.outcome, Outcome::NoPulse);
        assert_eq!(loop_.dac_code(), 32767);
    }

    #[test]
    fn a_count_more_than_two_percent_off_either_way_is_rejected_and_measured_again() {
        for count in [EXPECTED + EXPECTED / 40, 0] {
            let mut loop_ = discipline();
            let step = loop_.measure(measurement(1, count), ms(100));
            assert!(
                matches!(step.outcome, Outcome::Rejected { .. }),
                "{count}: {step:?}"
            );
            let step = loop_.measure(measurement(1, EXPECTED), ms(200));
            assert!(
                matches!(step.outcome, Outcome::Corrected(_)),
                "the same pulse is measured again: {step:?}"
            );
        }
    }

    #[test]
    fn a_code_is_written_once_and_steps_follow_from_the_confirmed_one() {
        let mut loop_ = discipline();
        let first = loop_
            .measure(measurement(1, EXPECTED + 1), ms(1000))
            .dac_write()
            .expect("a write");
        // Unconfirmed: the next correction steps from mid-scale again.
        assert_eq!(loop_.dac_code(), 32767);
        loop_.dac_written(first);
        assert_eq!(loop_.dac_code(), first);
        let step = loop_.measure(measurement(2, EXPECTED), ms(2000));
        let Outcome::Corrected(correction) = step.outcome else {
            panic!("a correction: {step:?}");
        };
        assert_eq!(
            correction.write,
            correction.dac != first,
            "only a changed code is written"
        );
    }

    #[test]
    fn the_dac_stops_at_its_rails() {
        let mut low = discipline();
        let mut high = discipline();
        for pulse in 1..=40_u8 {
            let at = ms(1000 * u64::from(pulse));
            for (loop_, count) in [(&mut low, EXPECTED + 4000), (&mut high, EXPECTED - 4000)] {
                if let Some(code) = loop_
                    .measure(measurement(pulse % 16, count), at)
                    .dac_write()
                {
                    loop_.dac_written(code);
                }
            }
        }
        assert_eq!(low.dac_code(), 0, "a persistently fast count rails at 0");
        assert_eq!(high.dac_code(), u16::MAX, "a slow one at full scale");
    }

    #[test]
    fn holdover_begins_after_two_seconds_without_a_pulse_and_ends_at_the_next() {
        let mut loop_ = discipline();
        let quiet = |loop_: &mut OscillatorDiscipline, at| {
            loop_.measure(measurement(0, EXPECTED), ms(at)).holdover
        };
        assert_eq!(quiet(&mut loop_, 2000), None, "two seconds is not past two");
        assert_eq!(quiet(&mut loop_, 2100), Some(Holdover::Entered));
        assert_eq!(quiet(&mut loop_, 2200), None, "reported once");
        assert!(loop_.in_holdover());
        let step = loop_.measure(measurement(1, EXPECTED), ms(2300));
        assert_eq!(step.holdover, Some(Holdover::Left));
        assert!(!loop_.in_holdover());
    }
}
