//! The SYSREF (LMX1204) and SYNC (LMX1214) windowing the TI distribution chips share: the
//! delay line's step size for the input clock, and the capture that reads the 32
//! clock-position samples a SYSREF or SYNC edge latched (`RB_CLKPOS`, R12:R11) and sets the
//! delay that centres the edge between two clock edges (`hw/common/common.c`,
//! `common_ti_calc_sync_delay`), written into R15 bits 6:1 on both chips.
//!
//! The C scans the samples from bit 0 for a rising pattern (`1011` or `1101` over four
//! samples, `111` over three, `11` over two), marks where one starts, skips past it and
//! marks the next sample clear; the delay is the midpoint of the first two marked
//! positions. It leaves the samples it skips unmarked in an uninitialised array, which it
//! then reads, and marks past the array's 32 entries when a pattern ends at the top: this
//! treats unmarked samples as clear and ignores marks past bit 31 (`PORTING.md`).

use hal::{Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::outcome::outcome;
use super::ti_spi;

/// The clock positions' lower half (`RB_CLKPOS_L`), on both chips.
const CLOCK_POSITION_LOW: u8 = 0x0b;
/// Their upper half (`RB_CLKPOS_U`).
const CLOCK_POSITION_HIGH: u8 = 0x0c;
/// The register the delay goes into, at bits 6:1 (`SYSREFREQ_DELAY_STEP`, `SYNC_DLY`).
const DELAY_REG: u8 = 0x0f;

/// The delay line's step size code for an input clock of `clkin_hz`
/// (`lmx1204_get_delay_step_size`, and `lmx1214_sysref_windowing_beforesync`'s copy): the
/// narrowest step whose range holds it, later ranges winning where they overlap. 0 is
/// 28 ps (1.4–2.7 GHz), 1 15 ps (2.4–4.7 GHz), 2 11 ps (3.1–5.7 GHz), 3 8 ps
/// (4.5–12.8 GHz).
pub(crate) fn delay_step(clkin_hz: u64) -> u16 {
    let mut step = 0;
    if clkin_hz > 2_400_000_000 && clkin_hz <= 4_700_000_000 {
        step = 1;
    }
    if clkin_hz > 3_100_000_000 && clkin_hz <= 5_700_000_000 {
        step = 2;
    }
    if clkin_hz > 4_500_000_000 && clkin_hz <= 12_800_000_000 {
        step = 3;
    }
    step
}

/// `*_sysref_windowing_capture`: the clock positions read (R11, then R12), then R15
/// written with the delay that centres the captured edge. Positions libusdr refuses
/// (`-EINVAL`) end the sequence without the write and without failure;
/// [`Capture::delay`] says so.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Capture {
    /// The chip, for failure reports.
    chip: &'static str,
    /// Where it answers.
    target: SpiAddr,
    /// R15 as the chip's plan sets it, with no delay.
    base: u16,
    /// The lower half, once read.
    low: Option<u16>,
    /// Both halves are read.
    read: bool,
    /// The delay, once both halves are read and libusdr accepts them.
    delay: Option<u8>,
    /// R15 written, or an access failed.
    done: bool,
}

impl Capture {
    /// The capture of `chip` at `target`, whose R15 is `base` without a delay.
    pub(crate) const fn new(chip: &'static str, target: SpiAddr, base: u16) -> Self {
        Self {
            chip,
            target,
            base,
            low: None,
            read: false,
            delay: None,
            done: false,
        }
    }

    /// The delay written; `None` until the positions are read, or when they were refused.
    #[allow(
        dead_code,
        reason = "the windowing's, which no libusdr PE_SYNC path calls (see `windowing`)"
    )]
    pub(crate) const fn delay(&self) -> Option<u8> {
        self.delay
    }
}

impl Sequence for Capture {
    fn next(&self) -> Option<SequenceAction> {
        if self.done {
            return None;
        }
        if self.low.is_none() {
            return Some(ti_spi::read(self.target, CLOCK_POSITION_LOW));
        }
        if !self.read {
            return Some(ti_spi::read(self.target, CLOCK_POSITION_HIGH));
        }
        let delay = self.delay?;
        Some(ti_spi::write(
            self.target,
            (DELAY_REG, self.base | u16::from(delay) << 1),
        ))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let word = outcome(action, result, self.chip).inspect_err(|_| self.done = true)?;
        match self.low {
            None => self.low = Some(ti_spi::value(word)),
            Some(low) if !self.read => {
                let high = ti_spi::value(word);
                self.read = true;
                self.delay = sync_delay(u32::from(high) << 16 | u32::from(low));
            }
            Some(_) => self.done = true,
        }
        Ok(())
    }
}

/// The SYSREFREQ delay step for the clock positions `clkpos` (R12 in the upper half), or
/// `None` where libusdr refuses (`-EINVAL`): the first or last sample is not set, fewer
/// than two rising patterns are found, or the midpoint is 0 or above 0x3F.
pub(crate) fn sync_delay(clkpos: u32) -> Option<u8> {
    const SAMPLES: usize = 32;
    if clkpos & 0x8000_0001 != 0x8000_0001 {
        return None;
    }
    let mut rising = [false; SAMPLES];
    let mut i = 0;
    while i < SAMPLES {
        let window = clkpos >> i;
        let skip = if matches!(window & 0b1111, 0b1101 | 0b1011) {
            4
        } else if window & 0b111 == 0b111 {
            3
        } else if window & 0b11 == 0b11 {
            2
        } else {
            0
        };
        if skip > 0 {
            rising[i] = true;
            i += skip;
        }
        i += 1;
    }
    let mut marks = (0..SAMPLES).filter(|&i| rising[i]);
    let (first, second) = (marks.next()?, marks.next()?);
    let delay = u8::try_from(usize::midpoint(first, second)).ok()?;
    (1..=0x3f).contains(&delay).then_some(delay)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::testing::{Exchange, drive};

    #[test]
    fn samples_without_both_end_bits_are_refused() {
        assert_eq!(sync_delay(0), None);
        assert_eq!(sync_delay(0x7fff_ffff), None, "no top sample");
        assert_eq!(sync_delay(0xffff_fffe), None, "no bottom sample");
    }

    #[test]
    fn the_delay_is_the_midpoint_of_the_first_two_rising_patterns() {
        // All set: `111` at 0, then (after the skip and the clear mark) at 4.
        assert_eq!(sync_delay(0xffff_ffff), Some(2));
        // `11` at 0 and at 30, nothing between.
        assert_eq!(sync_delay(0xc000_0003), Some(15));
        // `1011` at 0 (bits 0, 1 and 3), then `11` at 30.
        assert_eq!(sync_delay(0xc000_000b), Some(15));
    }

    #[test]
    fn a_single_rising_pattern_is_refused() {
        // Bit 0 alone matches no pattern; bits 30 and 31 are the only one.
        assert_eq!(sync_delay(0xc000_0001), None);
    }

    #[test]
    fn a_capture_writes_the_centring_delay_into_r15() {
        let mut capture = Capture::new("LMX1204", SpiAddr(4), 0x0380);
        let (done, outcome) = drive(&mut capture, |_| Ok(0xffff));
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done,
            [
                Exchange::Spi(0x008b_0000),
                Exchange::Spi(0x008c_0000),
                Exchange::Spi(0x000f_0384),
            ],
            "delay 2 in bits 6:1"
        );
        assert_eq!(capture.delay(), Some(2));
    }

    #[test]
    fn refused_positions_end_the_capture_without_a_write() {
        let mut capture = Capture::new("LMX1214", SpiAddr(5), 0x0b00);
        let (done, outcome) = drive(&mut capture, |_| Ok(0));
        assert_eq!(outcome, Ok(()), "reported by `delay`, not as a failure");
        assert_eq!(done.len(), 2);
        assert_eq!(capture.delay(), None);
    }
}
