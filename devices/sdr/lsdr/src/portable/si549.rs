//! Skyworks (Silicon Labs) Si549 oscillator, the ADCs' sample clock: libusdr's
//! `hw/si549/si549.c`, the output gate (`si549_enable`) and the frequency plan
//! (`si549_set_freq`, [`SetFrequency`]), which `/dm/rate/master` runs.
//!
//! Field names and meanings from the vendored `hw/si549/si549.yaml`; the plan's limits and
//! crystal are `si549.c`'s `enum si549_helpers`. Chip-level code: it moves to a shared
//! `devices/sdr/chips/si549` crate when a second driver needs it (`PORTING.md`; libusdr
//! uses the Si549 on no other board).

use bnb::{BitEnum, bitfield};
use hal::{I2cAddr, Register, RegisterAccess, Sequence, SequenceAction, SequenceFailure};

use super::outcome::outcome;

/// The chip's name in errors.
pub(crate) const NAME: &str = "Si549";

/// The registers the `LSDr` touches.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// The device type, read-only; see [`DEVICE_TYPE`]. `DEVICE`, register 0.
    Device = 0,

    /// Reset and frequency calibration, self-clearing bits. `CFG`, register 7.
    Config = 7,

    /// See [`OutputControl`]. `ODC`, register 17.
    OutputControl = 17,

    /// The high-speed divider's low byte. `DIV` (23:24) bits 7:0, register 23.
    DividerLow = 23,

    /// The low-speed divider's code in bits 6:4 and the high-speed divider's bits 10:8 in
    /// bits 2:0. `DIV` bits 15:8, register 24.
    DividerHigh = 24,

    /// The feedback divider, least significant byte first: four fraction bytes, then the
    /// integer's two. `FBDIV`, registers 26 to 31.
    Feedback0 = 26,

    /// `FBDIV`, second byte.
    Feedback1 = 27,

    /// `FBDIV`, third byte.
    Feedback2 = 28,

    /// `FBDIV`, fourth byte (the fraction's most significant).
    Feedback3 = 29,

    /// `FBDIV`, fifth byte (the integer's least significant).
    Feedback4 = 30,

    /// `FBDIV`, sixth byte.
    Feedback5 = 31,

    /// The frequency-calibration override. `FCAL_OVR`, register 69.
    CalibrationOverride = 69,

    /// The register page. `PAGE`, register 255.
    Page = 255,
}

/// Drives or gates the output clock. `ODC`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputControl {
    /// Drive the output clock; clear disables it. `OE`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for OutputControl {
    type Map = Reg;
    const ADDR: Reg = Reg::OutputControl;
}

/// What [`Reg::Device`] reads on an Si549 (the YAML's "value of 49"); `si549_set_freq`
/// refuses any other (`-EIO`).
pub(crate) const DEVICE_TYPE: u8 = 0x31;

/// [`Reg::Config`]'s `MS_ICAL2`, bit 3: start the frequency calibration.
const START_CALIBRATION: u8 = 0x08;

/// The lowest output `si549_set_freq` accepts (`si549_Fout_min`), in Hz.
const OUTPUT_MIN: u32 = 200_000;

/// The highest output it accepts (`si549_Fout_max`), in Hz.
const OUTPUT_MAX: u32 = 800_000_000;

/// The VCO range the plan aims for (`si549_Fvco_min`, `si549_Fvco_max`), in Hz.
const VCO: (u64, u64) = (10_800_000_000, 12_109_728_345);

/// The crystal the feedback divider multiplies (`si549_xtal`), in Hz.
const CRYSTAL: u64 = 152_600_000;

/// The largest high-speed divider (`si549_hsdiv_max`).
const HS_DIVIDER_MAX: u64 = 2046;

/// The dividers `si549_set_freq` programs for one output frequency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrequencyPlan {
    /// The high-speed divider, even (11 bits).
    hs_divider: u64,

    /// The low-speed divider's code, its power of two (`lsdiv_rv`).
    ls_code: u8,

    /// The feedback divider's integer part.
    feedback: u64,

    /// The feedback divider's fraction, in 2^-32.
    fraction: u32,
}

impl FrequencyPlan {
    /// libusdr's plan for `output` Hz, or `None` outside 200 kHz to 800 MHz (its `-ERANGE`,
    /// before any bus access).
    ///
    /// As `si549_set_freq`: the smallest low-speed divider (1 to 32) whose output, times
    /// the largest high-speed divider, reaches the VCO's minimum; then the high-speed
    /// divider that keeps the VCO at or below its maximum, rounded up to even and capped;
    /// the VCO it gives divided by the crystal is the feedback divider. The VCO is not
    /// checked against its range afterwards, as in libusdr.
    pub(crate) fn new(output: u32) -> Option<Self> {
        if !(OUTPUT_MIN..=OUTPUT_MAX).contains(&output) {
            return None;
        }
        // The loop's exit when no low-speed divider fits (unreachable from 200 kHz up):
        // the last one tried, code 6 after the increment, and no high-speed divider.
        let mut plan = (u64::from(output) << 5, 6, 1);
        for ls_code in 0..6 {
            let vco = u64::from(output) << ls_code;
            if vco * HS_DIVIDER_MAX < VCO.0 {
                continue;
            }
            let mut hs_divider = VCO.1 / vco;
            hs_divider += hs_divider & 1;
            let hs_divider = hs_divider.min(HS_DIVIDER_MAX);
            plan = (vco * hs_divider, ls_code, hs_divider);
            break;
        }
        let (vco, ls_code, hs_divider) = plan;
        let fraction = ((vco % CRYSTAL) << 32) / CRYSTAL;
        Some(Self {
            hs_divider,
            ls_code,
            feedback: vco / CRYSTAL,
            fraction: u32::try_from(fraction).expect("invariant: a remainder's fraction is < 1"),
        })
    }

    /// The writes `si549_set_freq` issues after the device-type check, in its order: page
    /// 0, calibration override off, output gated, the dividers, the feedback divider, the
    /// calibration started, the output enabled.
    fn writes(self) -> [(Reg, u8); 13] {
        let [hs_low, hs_high, ..] = self.hs_divider.to_le_bytes();
        let [f0, f1, f2, f3] = self.fraction.to_le_bytes();
        let [fb0, fb1, ..] = self.feedback.to_le_bytes();
        [
            (Reg::Page, 0),
            (Reg::CalibrationOverride, 0),
            (
                Reg::OutputControl,
                OutputControl::new().with_enabled(false).into(),
            ),
            (Reg::DividerLow, hs_low),
            (Reg::DividerHigh, self.ls_code << 4 | hs_high & 0x7),
            (Reg::Feedback0, f0),
            (Reg::Feedback1, f1),
            (Reg::Feedback2, f2),
            (Reg::Feedback3, f3),
            (Reg::Feedback4, fb0),
            (Reg::Feedback5, fb1),
            (Reg::Config, START_CALIBRATION),
            (
                Reg::OutputControl,
                OutputControl::new().with_enabled(true).into(),
            ),
        ]
    }
}

/// `si549_set_freq`: reads the device type, refuses a chip that is not an Si549, then
/// programs [`FrequencyPlan`] and enables the output. Every failure ends it, as libusdr
/// returns on each.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SetFrequency {
    /// Where the oscillator answers.
    dev: I2cAddr,

    /// The writes after the check.
    writes: [(Reg, u8); 13],

    /// Actions completed.
    done: usize,

    /// An action failed; nothing more is issued.
    failed: bool,
}

impl SetFrequency {
    /// Starts programming `plan` into the oscillator at `dev`; nothing has been accessed
    /// yet.
    pub(crate) fn new(dev: I2cAddr, plan: FrequencyPlan) -> Self {
        Self {
            dev,
            writes: plan.writes(),
            done: 0,
            failed: false,
        }
    }

    /// Action `step`.
    fn access(&self, step: usize) -> Option<RegisterAccess> {
        if step == 0 {
            return Some(RegisterAccess::read(self.dev, Reg::Device));
        }
        let entry = *self.writes.get(step - 1)?;
        Some(RegisterAccess::write(self.dev, entry))
    }
}

impl Sequence for SetFrequency {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.access(self.done).map(SequenceAction::Access)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let word = outcome(action, result, NAME).inspect_err(|_| self.failed = true)?;
        let found = word.to_le_bytes()[0];
        if self.done == 0 && found != DEVICE_TYPE {
            self.failed = true;
            return Err(SequenceFailure::ChipId {
                chip: NAME,
                expected: DEVICE_TYPE.into(),
                found: found.into(),
            });
        }
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The writes `set` issues, answering the type check with `device_type`, and how many.
    fn bytes_written(mut set: SetFrequency, device_type: u32) -> ([(u8, u8); 13], usize) {
        let mut written = [(0, 0); 13];
        let mut count = 0;
        while let Some(SequenceAction::Access(access)) = set.next() {
            if let RegisterAccess::Write { reg, value, .. } = access {
                written[count] = (reg, value);
                count += 1;
            }
            if set.complete::<()>(Ok(device_type)).is_err() {
                break;
            }
        }
        (written, count)
    }

    #[test]
    fn gating_the_output_writes_libusdrs_bytes_to_odc() {
        let entry = |enabled| OutputControl::new().with_enabled(enabled).entry();
        assert_eq!(entry(false), (Reg::OutputControl, 0));
        assert_eq!(entry(true), (Reg::OutputControl, 1));
    }

    #[test]
    fn a_250_mhz_clock_is_libusdrs_plan() {
        // From libusdr's /dm/rate/master = 50 MS/s: LSDIV 1, HSDIV 48, VCO 12 GHz.
        let plan = FrequencyPlan::new(250_000_000).expect("in range");
        let set = SetFrequency::new(I2cAddr::new(0, 0x67), plan);
        assert_eq!(
            bytes_written(set, 0x31),
            (
                [
                    (255, 0x00),
                    (69, 0x00),
                    (17, 0x00),
                    (23, 0x30),
                    (24, 0x00),
                    (26, 0xf2),
                    (27, 0xc4),
                    (28, 0x0f),
                    (29, 0xa3),
                    (30, 0x4e),
                    (31, 0x00),
                    (7, 0x08),
                    (17, 0x01),
                ],
                13
            )
        );
    }

    #[test]
    fn the_range_is_libusdrs_and_low_outputs_take_a_low_speed_divider() {
        assert_eq!(FrequencyPlan::new(199_999), None);
        assert_eq!(FrequencyPlan::new(800_000_001), None);
        let low = FrequencyPlan::new(200_000).expect("the minimum");
        assert_eq!((low.ls_code, low.hs_divider), (5, 1892));
        let high = FrequencyPlan::new(800_000_000).expect("the maximum");
        assert_eq!((high.ls_code, high.hs_divider), (0, 16));
    }

    #[test]
    fn another_device_type_is_refused_before_anything_is_written() {
        let plan = FrequencyPlan::new(250_000_000).expect("in range");
        let mut set = SetFrequency::new(I2cAddr::new(0, 0x67), plan);
        assert_eq!(
            set.complete::<()>(Ok(0xff)),
            Err(SequenceFailure::ChipId {
                chip: NAME,
                expected: 0x31,
                found: 0xff,
            })
        );
        assert_eq!(set.next(), None);
    }
}
