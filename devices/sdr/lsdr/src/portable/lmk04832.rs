//! TI LMK04832 clock generator, fitted on dual-ADC boards: libusdr's presence test
//! (`lmk04832_reset_and_test`, `hw/lmk04832/lmk04832.c`) as a sans-I/O sequence.
//!
//! Frame, as `lmk04832.c` builds the SPI word: 24 bits, bit 23 set for a read
//! (`MAKE_LMK04832_RDREG`), the 15-bit register address in bits 22:8, data in bits 7:0; a
//! read keeps the low byte of the word returned. The generated header and YAML put the read
//! bit at 31 instead; the presence test does not use them. Register and field names are the
//! vendored YAML's; its 15-bit addresses do not fit `rsl-lowlevel`'s 8-bit register traits,
//! so values here are plain bytes. Chip-level code: it moves to a shared
//! `devices/sdr/chips/lmk04832` crate when a second driver needs it (`PORTING.md`).

use bnb::{bitfield, u3};
use rsl_lowlevel::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::outcome::outcome;

/// The chip's name in errors.
pub(crate) const NAME: &str = "LMK04832";

/// Bit 23 of the frame: read.
const READ: u32 = 1 << 23;

/// The registers the presence test touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reg {
    /// See [`Reset`]. `RESET`, R0.
    Reset = 0x000,

    /// Product ID, the byte libusdr's `lmk04832_read16` puts low. `PROD_HI`, R4.
    ProductHigh = 0x004,

    /// Product ID, the byte libusdr puts high. `PROD_LOW`, R5.
    ProductLow = 0x005,

    /// Vendor ID, the byte libusdr puts low. `VNDR_HI`, R12.
    VendorHigh = 0x00c,

    /// Vendor ID, the byte libusdr puts high. `VNDR_LOW`, R13.
    VendorLow = 0x00d,

    /// See [`ResetConfig`]. `RESET_CFG`, R330.
    ResetConfig = 0x14a,
}

/// Resets the chip and selects its SPI mode. `RESET`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reset {
    /// Reset the chip; clears itself (YAML: "automatically cleared"). `RESET`, bit 7.
    #[bits(7..=7)]
    reset: bool,

    /// Four-wire SPI: 3-wire mode disabled. `SPI_3WARE_DIS` (sic), bit 4.
    #[bits(4..=4)]
    three_wire_disabled: bool,
}

/// What the RESET pin does. `RESET_CFG`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResetConfig {
    /// What the pin outputs, when [`Self::pin_type`] is an output (YAML). `RESET_MUX`,
    /// bits 5:3; [`SPI_READBACK`] routes register readback to it.
    #[bits(3..=5)]
    mux: u3,

    /// The pin's I/O type. `RESET_TYPE`, bits 2:0; [`OUTPUT_PUSH_PULL`] drives it.
    #[bits(0..=2)]
    pin_type: u3,
}

/// `RESET_MUX_SPI_READBACK`.
const SPI_READBACK: u3 = u3::new(6);

/// `RESET_TYPE_OUTPUT_PUSH_PULL`.
const OUTPUT_PUSH_PULL: u3 = u3::new(3);

/// The product ID libusdr accepts, `PROD_LOW << 8 | PROD_HI`.
const PRODUCT: u16 = 0xd163;

/// The vendor ID libusdr accepts, `VNDR_LOW << 8 | VNDR_HI`.
const VENDOR: u16 = 0x0451;

/// A write of `data` to `reg`.
fn write(target: SpiAddr, reg: Reg, data: u8) -> FpgaAccess {
    FpgaAccess::Spi {
        target,
        word: (reg as u32) << 8 | u32::from(data),
        reg: reg as u32,
        write: true,
    }
}

/// A read of `reg`.
fn read(target: SpiAddr, reg: Reg) -> FpgaAccess {
    FpgaAccess::Spi {
        target,
        word: READ | (reg as u32) << 8,
        reg: reg as u32,
        write: false,
    }
}

/// The test's SPI exchanges, in libusdr's order: reset (with 4-wire SPI), route readback to
/// the RESET pin and drive it, release the reset (still 4-wire), then read the product and
/// vendor IDs, each low address first.
const STEPS: usize = 7;

/// `lmk04832_reset_and_test`: resets the chip, routes its register readback and reads its
/// IDs; the chip is present when they are libusdr's ([`Self::present`]). A missing chip is
/// not a failure (libusdr's `-ENODEV`, which open tolerates); a failed exchange is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClockGeneratorProbe {
    /// The chip's SPI target.
    target: SpiAddr,

    /// Exchanges completed.
    done: usize,

    /// The ID bytes read, in [`Self::exchange`]'s read order.
    ids: [u8; 4],

    /// An exchange failed; nothing more is issued.
    failed: bool,
}

impl ClockGeneratorProbe {
    /// Starts the test on the chip at `target`; nothing has been accessed yet.
    pub(crate) const fn new(target: SpiAddr) -> Self {
        Self {
            target,
            done: 0,
            ids: [0; 4],
            failed: false,
        }
    }

    /// Exchange `step` of [`STEPS`].
    fn exchange(&self, step: usize) -> Option<FpgaAccess> {
        let target = self.target;
        let reset = |reset| {
            u8::from(
                Reset::new()
                    .with_reset(reset)
                    .with_three_wire_disabled(true),
            )
        };
        let routed = ResetConfig::new()
            .with_mux(SPI_READBACK)
            .with_pin_type(OUTPUT_PUSH_PULL);
        Some(match step {
            0 => write(target, Reg::Reset, reset(true)),
            1 => write(target, Reg::ResetConfig, routed.into()),
            2 => write(target, Reg::Reset, reset(false)),
            3 => read(target, Reg::ProductHigh),
            4 => read(target, Reg::ProductLow),
            5 => read(target, Reg::VendorHigh),
            6 => read(target, Reg::VendorLow),
            _ => return None,
        })
    }

    /// Whether the IDs read are the LMK04832's, as libusdr assembles them
    /// (`R(addr + 1) << 8 | R(addr)`); false until the test has finished.
    pub(crate) fn present(&self) -> bool {
        let [product_high, product_low, vendor_high, vendor_low] = self.ids;
        self.done == STEPS
            && u16::from_le_bytes([product_high, product_low]) == PRODUCT
            && u16::from_le_bytes([vendor_high, vendor_low]) == VENDOR
    }
}

impl Sequence for ClockGeneratorProbe {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.exchange(self.done).map(SequenceAction::Fpga)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let word = outcome(action, result, NAME).inspect_err(|_| self.failed = true)?;
        if let Some(id) = self.done.checked_sub(3).and_then(|at| self.ids.get_mut(at)) {
            *id = word.to_le_bytes()[0];
        }
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the test answering the four ID reads with `ids`; returns the words sent.
    fn run(probe: &mut ClockGeneratorProbe, ids: [u32; 4]) -> [u32; STEPS] {
        let mut words = [0; STEPS];
        let mut at = 0;
        while let Some(SequenceAction::Fpga(FpgaAccess::Spi { word, .. })) = probe.next() {
            words[at] = word;
            let answer = at.checked_sub(3).map_or(u32::MAX, |id| ids[id]);
            probe.complete::<()>(Ok(answer)).unwrap();
            at += 1;
        }
        words
    }

    #[test]
    fn the_test_resets_routes_readback_and_reads_the_ids_as_libusdr_does() {
        let mut probe = ClockGeneratorProbe::new(SpiAddr(2));
        let words = run(&mut probe, [0x63, 0xd1, 0x51, 0x04]);
        assert_eq!(
            words,
            [
                0x90, 0x01_4a33, 0x10, 0x80_0400, 0x80_0500, 0x80_0c00, 0x80_0d00
            ],
            "the words of MAKE_LMK04832_RESET, _RESET_CFG and _RDREG"
        );
        assert!(probe.present());
    }

    #[test]
    fn other_ids_or_a_floating_bus_are_an_absent_chip_not_a_failure() {
        let mut floating = ClockGeneratorProbe::new(SpiAddr(2));
        run(&mut floating, [u32::MAX; 4]);
        assert!(!floating.present());
        let mut swapped = ClockGeneratorProbe::new(SpiAddr(2));
        run(&mut swapped, [0xd1, 0x63, 0x51, 0x04]);
        assert!(!swapped.present(), "libusdr reads PROD_HI as the low byte");
    }

    #[test]
    fn a_failed_exchange_ends_the_test_absent() {
        let mut probe = ClockGeneratorProbe::new(SpiAddr(2));
        probe.complete::<()>(Ok(0)).unwrap();
        let failure = probe.complete(Err(())).unwrap_err();
        assert_eq!(failure.access().map(|(chip, ..)| chip), Some(NAME));
        assert_eq!(probe.next(), None);
        assert!(!probe.present());
    }
}
