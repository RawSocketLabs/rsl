//! The LMS7002M RF transceiver's SPI register file, shared by the simulated boards that
//! carry the chip (`xsdr-sim`, `limesdr-sim`).
//!
//! Word format, from libusdr's register YAML (`hw/lms7002m/lms7002m.yaml`: `wr_mask:
//! 0x80000000`, 16-bit address and data) and `gen/def_lms7002m.h` (`MAKE_LMS7002M_REG_WR`,
//! `MAKE_LMS7002M_REG_RD`): bit 31 selects a write, bits 30:16 carry the address and bits
//! 15:0 the data.
//!
//! Assumptions, each a sim choice the pinned source cannot settle:
//! - Reset values: the vendored YAML and header document fields but no reset values, so
//!   every register reads 0 until written, except the read-only version register below.
//!   No libusdr path the drivers port reads a register it has not written except that one,
//!   the VCO comparators and the processors' built-in self-test status. The self-test
//!   status (`RXTSP` `0x040E`, `TXTSP` `0x0209`, read by `lms7002m_xxtsp_bst_isdone`) is
//!   never written, so it reads 0: bit 0 clear, which libusdr takes for "done".
//! - VCO comparators (`CGEN_0x008C`, `SXX_0x0123`): with a reference clock given
//!   ([`Lms7002m::with_reference`]), their comparator bits follow the cap code and divider
//!   the driver programmed; the model and its assumptions are in `vco.rs`. Without one
//!   they read as stored, like any register.
//! - Channel banking: the chip duplicates some registers per channel, selected by
//!   `LML_0x0020` MAC bits; which ones the vendored source does not list, so the file
//!   reads flat and MAC writes are stored like any other register. libusdr selects the
//!   bank before each comparator read and rewrites every register the read depends on
//!   first, so a flat file reads the same there. The RF model ([`Loopback`]) keeps its
//!   own per-bank copy of the pages libusdr writes bank by bank (`rf.rs`); reads never
//!   see it.
//! - The receive output's DC, which an FPGA's DC estimator measures: with a
//!   [`Loopback`] ([`Lms7002m::with_loopback`]), [`Lms7002m::rx_mean`] follows the
//!   synthesizers, mixers, test generator, corrections and RF loopback the driver
//!   programmed; the model and its assumptions are in `rf.rs`. Without one it is 0.
//! - Soft resets (`LML_0x0020` reset bits) do not clear the file: with no reset values to
//!   restore, clearing would invent them. The reset pin does ([`Lms7002m::reset`]).
#![forbid(unsafe_code)]

mod rf;
mod vco;

use std::collections::BTreeMap;

pub use rf::{AT_DC_HZ, Imbalance, Loopback};

/// Bit 31 of an SPI word: a write (`wr_mask` in the YAML).
const WRITE: u32 = 1 << 31;
/// The address bits of an SPI word's upper half; bit 15 there is [`WRITE`].
const ADDR_MASK: u16 = 0x7fff;
/// `LML_0x002F`: chip version (`VER`, bits 15:11), revision (`REV`, 10:6) and metal mask
/// (`MASK`, 5:0).
const VERSION: u16 = 0x002f;
/// `LML_0x0020`, whose `MAC` field (bits 1:0) selects the banks writes reach.
const MAC: u16 = 0x0020;
/// `VER` 7 and `REV` 1, the only values `lms7002m_create` accepts (`hw/lms7002m/lms7002m.c`:
/// `-ENODEV` otherwise).
const VER_REV: u16 = 7 << 11 | 1 << 6;
/// The `MASK` field's width: bits 5:0.
const MASK_FIELD: u8 = 0x3f;

/// The LMS7002M's 15-bit-addressed, 16-bit-wide register file.
#[derive(Debug)]
pub struct Lms7002m {
    /// Registers written since power-on or the last reset, by address.
    regs: BTreeMap<u16, u16>,
    /// What [`VERSION`] reads.
    version: u16,
    /// The reference clock in hertz, which the VCO comparators need; `None` reads them as
    /// stored.
    reference: Option<u64>,
    /// A cap code at which both comparators read "fail"
    /// ([`Lms7002m::with_comparator_failure`]).
    failing_cap: Option<u8>,
    /// The per-channel pages as each bank holds them, for the RF model.
    banks: rf::Banks,
    /// The RF loop, if the board has one.
    loopback: Option<Loopback>,
}

impl Lms7002m {
    /// A chip fresh out of power-on reset whose version register reports `VER` 7, `REV` 1
    /// and metal-mask revision `mask`. libusdr's generic driver ignores the mask; a board
    /// driver may not (the `u3_limesdr` family's `limesdr_init` accepts only 1).
    ///
    /// # Panics
    ///
    /// When `mask` does not fit the 6-bit `MASK` field.
    #[must_use]
    pub fn new(mask: u8) -> Self {
        assert!(mask <= MASK_FIELD, "MASK is a 6-bit field, got {mask:#x}");
        Self {
            regs: BTreeMap::new(),
            version: VER_REV | u16::from(mask),
            reference: None,
            failing_cap: None,
            banks: rf::Banks::default(),
            loopback: None,
        }
    }

    /// The same chip with an RF loop from its transmitter to its receiver, which
    /// [`Self::rx_mean`] follows. Needs a reference ([`Self::with_reference`]).
    #[must_use]
    pub fn with_loopback(mut self, loopback: Loopback) -> Self {
        self.loopback = Some(loopback);
        self
    }

    /// The mean of channel `channel`'s (0 is A) receive output at DC, I and Q in 12-bit
    /// LSBs, as the registers now program the chip (see the crate docs): 0 without a
    /// loop or a reference.
    #[must_use]
    pub fn rx_mean(&self, channel: usize) -> [f64; 2] {
        let (Some(loopback), Some(reference), true) = (self.loopback, self.reference, channel < 2)
        else {
            return [0.0; 2];
        };
        let flat = |addr| self.reg(addr);
        #[expect(
            clippy::cast_precision_loss,
            reason = "references are far below 2^52 Hz, so exact"
        )]
        let view = rf::View {
            flat: &flat,
            banks: &self.banks,
            reference: reference as f64,
        };
        view.rx_mean(channel, &loopback)
    }

    /// The same chip with its VCO comparators reading "fail" (`VCO_CMPLO` alone) whenever
    /// the cap code last written is `cap`, in CGEN and SXX alike: a fault injection for the
    /// tuning searches' error paths, not a modelled behaviour. Needs a reference.
    #[must_use]
    pub fn with_comparator_failure(mut self, cap: u8) -> Self {
        self.failing_cap = Some(cap);
        self
    }

    /// The same chip clocked from a `hertz` reference, so its VCO comparators answer the
    /// cap codes a tuning search writes (see the crate docs).
    #[must_use]
    pub fn with_reference(mut self, hertz: u32) -> Self {
        self.reference = Some(hertz.into());
        self
    }

    /// Shifts one SPI word through the chip and returns the word read back: the register's
    /// value in bits 15:0 for a read, 0 for a write. libusdr keeps only bits 15:0 of a read
    /// (`lms7002m_spi_rd`), so what MISO carries during the address phase is unconstrained;
    /// the sim returns 0 there.
    pub fn transact(&mut self, word: u32) -> u32 {
        let [addr_hi, addr_lo, data_hi, data_lo] = word.to_be_bytes();
        let addr = u16::from_be_bytes([addr_hi, addr_lo]);
        if word & WRITE != 0 {
            self.write(addr, u16::from_be_bytes([data_hi, data_lo]));
            return 0;
        }
        u32::from(self.reg(addr))
    }

    /// Writes a register. Bit 15 of `addr`, the SPI write flag, is not part of the address.
    /// The version register is read-only.
    pub fn write(&mut self, addr: u16, value: u16) {
        let addr = addr & ADDR_MASK;
        if addr != VERSION {
            self.regs.insert(addr, value);
        }
        let mac = self.regs.get(&MAC).copied().unwrap_or(0) & 0b11;
        self.banks.write(mac, addr, value);
    }

    /// A register as the chip reports it. Bit 15 of `addr` is ignored, as in
    /// [`Lms7002m::write`].
    #[must_use]
    pub fn reg(&self, addr: u16) -> u16 {
        let addr = addr & ADDR_MASK;
        let stored = |addr| self.regs.get(&addr).copied().unwrap_or(0);
        match (addr, self.reference) {
            (VERSION, _) => self.version,
            (vco::CGEN_COMPARATORS, Some(reference)) => {
                let reading = if self.fails_at(vco::cgen_cap(stored)) {
                    vco::FAIL
                } else {
                    vco::cgen(stored, reference)
                };
                vco::merge(stored(addr), reading)
            }
            (vco::SXX_COMPARATORS, Some(reference)) => {
                let reading = if self.fails_at(vco::sxx_cap(stored)) {
                    vco::FAIL
                } else {
                    vco::sxx(stored, reference)
                };
                vco::merge(stored(addr), reading)
            }
            _ => stored(addr),
        }
    }

    /// Whether the comparators are forced to fail at cap code `cap`.
    fn fails_at(&self, cap: u16) -> bool {
        self.failing_cap
            .is_some_and(|failing| u16::from(failing) == cap)
    }

    /// Pulses the chip's reset pin (`RESET`): every register returns to its power-on state,
    /// which this model represents as unwritten (see the crate docs on reset values).
    pub fn reset(&mut self) {
        self.regs.clear();
        self.banks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chip with a loop whose receiver is offset by (10, -3) DAC steps, the RX
    /// synthesizer running at 1 GHz and the TX one at 1 GHz plus `tx_offset_hz`, both
    /// channels' RX DC correctors and corrections bypassed and their mixers off.
    fn looped_chip(tx_offset_hz: u32) -> Lms7002m {
        let mut chip = Lms7002m::new(0)
            .with_reference(25_000_000)
            .with_loopback(Loopback {
                rx_dc_offset: [10.0, -3.0],
                tx_lo_leakage: [40.0, 0.0],
                gain: 0.5,
                ..Loopback::default()
            });
        // CGEN 400 MHz: (31 + 1) * 25 MHz over 2 * (0 + 1); ADC 100 MHz.
        chip.write(0x0088, 31 << 4);
        chip.write(0x0089, 0);
        // SXR (bank A) 1 GHz = 25 MHz * 80 / 2; SXT (bank B) from its fraction.
        chip.write(0x0020, 1);
        chip.write(0x011e, (80 - 4) << 4);
        chip.write(0x011f, 1 << 6);
        chip.write(0x0124, 1 << 4);
        chip.write(0x0020, 2);
        let fraction = u64::from(tx_offset_hz) * 2 * (1 << 20) / 25_000_000;
        let high = u16::try_from(fraction >> 16).expect("4 bits");
        chip.write(0x011e, (80 - 4) << 4 | high);
        chip.write(0x011d, u16::try_from(fraction & 0xffff).expect("16 bits"));
        chip.write(0x011f, 1 << 6);
        chip.write(0x0124, 1 << 4);
        chip.write(0x0020, 3);
        chip.write(0x040c, 1 << 2 | 1 << 7 | 0b11);
        chip
    }

    /// `value`'s low 11 bits, as a DC DAC takes it.
    fn dac(value: i16) -> u16 {
        u16::from_ne_bytes(value.to_ne_bytes()) & 0x7ff
    }

    #[test]
    fn the_receivers_dc_offset_reads_until_its_dac_cancels_it() {
        let mut chip = looped_chip(0);
        assert_eq!(chip.rx_mean(0), [20.0, -6.0], "two 12-bit LSBs per step");
        chip.write(0x05c7, dac(-10));
        chip.write(0x05c8, dac(3));
        assert_eq!(chip.rx_mean(0), [0.0, 0.0]);
        assert_eq!(chip.rx_mean(1), [20.0, -6.0], "channel B has its own DACs");
        chip.write(0x0020, 2);
        chip.write(0x040c, 1 << 7 | 0b11);
        assert_eq!(chip.rx_mean(1), [0.0, 0.0], "B's corrector removes its DC");
        assert_eq!(chip.rx_mean(0), [0.0, 0.0], "A's DACs still cancel");
        let mut unclocked = looped_chip(0);
        unclocked.reference = None;
        assert_eq!(unclocked.rx_mean(0), [0.0; 2], "no reference, no LO");
    }

    #[test]
    fn the_tx_leakage_is_measured_only_through_the_loopback_at_the_mixed_offset() {
        // TX 1 MHz above RX; the RX mixer down by 1 MHz brings the leakage to DC.
        let mut chip = looped_chip(1_000_000);
        chip.write(0x05c7, dac(-10));
        chip.write(0x05c8, dac(3));
        chip.write(0x0020, 1);
        // 1 MHz of the 100 MHz ADC clock.
        let word = (1_u64 << 32) / 100;
        chip.write(0x0442, u16::try_from(word >> 16).expect("16 bits"));
        chip.write(0x0443, u16::try_from(word & 0xffff).expect("16 bits"));
        chip.write(0x040c, 1 << 2 | 1 << 13 | 0b11);
        assert_eq!(chip.rx_mean(0), [0.0, 0.0], "the loop is open");
        chip.write(0x0101, 1);
        chip.write(0x010c, 1 << 6);
        let [i, q] = chip.rx_mean(0);
        // 40 steps of 1/1024, halved by the loop, in 12-bit LSBs: 40.
        assert!((i - 40.0).abs() < 1e-6 && q.abs() < 1e-6, "{i} {q}");
        chip.write(0x05c3, dac(-40));
        let [i, _] = chip.rx_mean(0);
        assert!(i.abs() < 1e-6, "the TX DAC cancels the leakage: {i}");
    }

    #[test]
    fn version_reads_the_only_chip_libusdr_accepts() {
        let mut chip = Lms7002m::new(0);
        let ver = chip.transact(u32::from(VERSION) << 16);
        assert_eq!((ver >> 11) & 0x1f, 7, "VER must be 7");
        assert_eq!((ver >> 6) & 0x1f, 1, "REV must be 1");
    }

    #[test]
    fn the_metal_mask_is_the_version_registers_low_bits() {
        assert_eq!(
            Lms7002m::new(1).reg(VERSION),
            0x3841,
            "LimeSDR's required word"
        );
        assert_eq!(Lms7002m::new(0).reg(VERSION), 0x3840);
    }

    #[test]
    #[should_panic(expected = "6-bit field")]
    fn a_mask_wider_than_its_field_is_refused() {
        let _ = Lms7002m::new(0x40);
    }

    #[test]
    fn a_write_is_read_back_and_the_write_flag_is_not_part_of_the_address() {
        let mut chip = Lms7002m::new(0);
        assert_eq!(chip.transact(WRITE | 0x0092 << 16 | 0xbeef), 0);
        assert_eq!(chip.transact(0x0092 << 16), 0xbeef);
        chip.write(0x8093, 0x1234);
        assert_eq!(chip.reg(0x0093), 0x1234);
    }

    #[test]
    fn the_version_register_ignores_writes() {
        let mut chip = Lms7002m::new(0);
        chip.transact(WRITE | u32::from(VERSION) << 16);
        assert_eq!(chip.reg(VERSION), 0x3840);
    }

    #[test]
    fn comparators_answer_only_with_a_reference_and_keep_their_other_bits() {
        // CGEN at 90 * 26 MHz; cap 0 is far too fast for it.
        let program = |chip: &mut Lms7002m| {
            chip.write(0x0088, 89 << 4);
            chip.write(0x008b, 0);
            chip.write(0x008c, 0x0f0f);
        };
        let mut plain = Lms7002m::new(0);
        program(&mut plain);
        assert_eq!(plain.transact(0x008c << 16), 0x0f0f, "read as stored");

        let mut clocked = Lms7002m::new(0).with_reference(26_000_000);
        program(&mut clocked);
        clocked.write(0x008b, 255 << 1);
        assert_eq!(
            clocked.transact(0x008c << 16),
            0x3f0f,
            "the slowest code reads high (both comparators) over the stored bits"
        );
    }

    #[test]
    fn a_forced_failure_reads_fail_at_its_cap_code_only() {
        let mut chip = Lms7002m::new(0)
            .with_reference(26_000_000)
            .with_comparator_failure(128);
        chip.write(0x0088, 89 << 4);
        chip.write(0x008b, 128 << 1);
        assert_eq!(chip.reg(0x008c) >> 12, 0b01, "VCO_CMPLO alone");
        chip.write(0x008b, 127 << 1);
        assert_ne!(chip.reg(0x008c) >> 12, 0b01);
    }

    #[test]
    fn reset_is_power_on_except_for_the_version() {
        let mut chip = Lms7002m::new(1);
        chip.write(0x0020, 0xffff);
        chip.reset();
        assert_eq!(chip.reg(0x0020), 0, "a reset forgets every write");
        assert_eq!(
            chip.reg(VERSION),
            0x3841,
            "the version is silicon, not state"
        );
    }
}
