//! The 100–5000 MHz front end's identification (libusdr's `fe1005000`,
//! `device/ext_fe_100_5000/ext_fe_100_5000.c`), the `LSDr`'s board: an XRA1405 GPIO
//! expander, two MAX2871 LOs and an attenuator behind the connector's SPI bus.
//!
//! libusdr finds the board by its expander: it routes the connector's SPI pins, writes
//! two configuration registers and reads them back. Every access is a register write or an
//! SPI exchange, so the check is a portable sequence, [`Fe1005000Probe`]. Each exchange
//! first selects the expander on the SPI-extension core (libusdr's `s_spi_ext_op`).
//!
//! What follows a found expander (`tune_rf_path`: the band switches, LOs and attenuator)
//! is not ported.

#[cfg(feature = "std")]
use rsl_lowlevel::{Bus, run_sequence};
use rsl_lowlevel::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

#[cfg(feature = "std")]
use crate::error::Error;
use crate::gpio::{FPGA, GpioCommand, PinMode};

/// The SPI-extension core's configuration register (`spiext_cfg`, hard-coded in
/// `device_fe_probe`): written before each exchange, it selects the chip and the frame.
pub const REG_SPI_EXT_CONFIG: u32 = 58;

/// The connector's SPI bus (`device_fe_probe` passes 4, the `LSDr`'s `SPI_EXTERNAL`).
pub const SPI_EXTERNAL: SpiAddr = SpiAddr(4);

/// The expander's SPI-extension configuration (`s_dev_cfg[DEV_EXPANDER]`,
/// `MAKE_SPI_CFG(b, cs, div)` = `b << 14 | cs << 8 | div`): one-byte frames
/// (`SPI_BYTES_XRA1405IL24`), chip select 7 (`CS_IDX_EXPANDER`), clock divider 25 (0x19)
/// (`SPI_DIV`).
const EXPANDER_CONFIG: u32 = 1 << 14 | 7 << 8 | 0x19;

/// The expander's name in errors.
const XRA1405: &str = "XRA1405";

/// The connector setup: the SPI clock, data and chip-select lines to the core
/// (`GPIO_SCK`, `GPIO_MOSI`, `GPIO_MISO`, `GPIO_CS0`, `GPIO_CS1`, `GPIO_CS2`, `GPIO_LE1`).
const SETUP: [GpioCommand; 7] = [
    GpioCommand::Configure(12, PinMode::Alternate0),
    GpioCommand::Configure(13, PinMode::Alternate0),
    GpioCommand::Configure(14, PinMode::Alternate0),
    GpioCommand::Configure(11, PinMode::Alternate0),
    GpioCommand::Configure(4, PinMode::Alternate0),
    GpioCommand::Configure(7, PinMode::Alternate0),
    GpioCommand::Configure(6, PinMode::Alternate0),
];

/// XRA1405 register addresses (`xra1405.yaml`; its datasheet is not vendored, so only the
/// names libusdr gives are used).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Reg {
    /// `OCR1`, 0x04.
    Ocr1 = 0x04,

    /// `OCR2`, 0x06.
    Ocr2 = 0x06,

    /// `GCR1`, 0x0C.
    Gcr1 = 0x0c,

    /// `GCR2`, 0x0E.
    Gcr2 = 0x0e,
}

/// One expander access: a register read, or a register write of a byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exchange {
    /// `xra1405il24_reg_rd`.
    Read(Reg),

    /// `xra1405il24_reg_wr`.
    Write(Reg, u8),
}

impl Exchange {
    /// The SPI word: the register (bit 7 set to read) above the data byte, which a read
    /// sends as 0xFF (`spiext_make_data_reg` over libusdr's two-byte frame).
    fn word(self) -> u32 {
        match self {
            Self::Read(reg) => u32::from(0x80 | reg as u8) << 8 | 0xff,
            Self::Write(reg, value) => u32::from(reg as u8 & 0x7f) << 8 | u32::from(value),
        }
    }

    /// The exchange as the SPI access that performs it.
    fn access(self) -> FpgaAccess {
        let (reg, write) = match self {
            Self::Read(reg) => (reg, false),
            Self::Write(reg, _) => (reg, true),
        };
        FpgaAccess::Spi {
            target: SPI_EXTERNAL,
            word: self.word(),
            reg: u32::from(reg as u8),
            write,
        }
    }
}

/// `ext_fe_100_5000_init`'s expander check: it reads `GCR1`, sets `GCR1` to 3 ("Do not
/// touch DCEN, RFEN") and `GCR2` to 0xC0, clears `OCR1` and `OCR2`, then reads `GCR1` and
/// `GCR2` back.
const CHECK: [Exchange; 7] = [
    Exchange::Read(Reg::Gcr1),
    Exchange::Write(Reg::Gcr1, 0x03),
    Exchange::Write(Reg::Gcr2, 0xc0),
    Exchange::Write(Reg::Ocr1, 0x00),
    Exchange::Write(Reg::Ocr2, 0x00),
    Exchange::Read(Reg::Gcr1),
    Exchange::Read(Reg::Gcr2),
];

/// What the read-back must hold for the board to be there: `GCR1` and `GCR2` as written.
const EXPECTED: [u8; 2] = [0x03, 0xc0];

/// Actions in the whole check: the pin setup, then a configuration write and an exchange
/// per expander access.
const ACTIONS: usize = SETUP.len() + 2 * CHECK.len();

/// libusdr's search for fe1005000 as far as its expander check, as a register-only
/// sequence: the connector's SPI pins, then the expander's register test.
#[derive(Clone, Copy, Debug)]
pub struct Fe1005000Probe {
    /// Actions completed; past [`ACTIONS`] once one failed.
    done: usize,
    /// The last two registers read back: `GCR1`, then `GCR2`.
    read_back: [u8; 2],
}

impl Fe1005000Probe {
    /// The check, from its first pin write.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            done: 0,
            read_back: [0xcc; 2],
        }
    }

    /// Whether the expander answered as libusdr requires, once the sequence has finished;
    /// `None` before, or after a failure.
    #[must_use]
    pub fn found(&self) -> Option<bool> {
        (self.done == ACTIONS).then_some(self.read_back == EXPECTED)
    }

    /// Action `index` of the check, and the chip a failure of it names.
    fn action(index: usize) -> Option<(FpgaAccess, &'static str)> {
        if let Some(command) = SETUP.get(index) {
            return Some((command.access(), FPGA));
        }
        let exchange = CHECK.get((index - SETUP.len()) / 2)?;
        Some(if (index - SETUP.len()) % 2 == 0 {
            let select = FpgaAccess::Write {
                reg: REG_SPI_EXT_CONFIG,
                value: EXPANDER_CONFIG,
            };
            (select, FPGA)
        } else {
            (exchange.access(), XRA1405)
        })
    }
}

impl Default for Fe1005000Probe {
    fn default() -> Self {
        Self::new()
    }
}

impl Sequence for Fe1005000Probe {
    fn next(&self) -> Option<SequenceAction> {
        Self::action(self.done).map(|(access, _)| SequenceAction::Fpga(access))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some((access, chip)) = Self::action(self.done) else {
            return Ok(());
        };
        let word = result.map_err(|source| {
            self.done = usize::MAX;
            SequenceFailure::FpgaTransport {
                chip,
                access,
                source,
            }
        })?;
        // The last two exchanges, the last actions but one and the last, are the read-back;
        // the byte read is the word's low byte (`spiext_parse_data_reg`).
        let slot = match ACTIONS - self.done {
            3 => Some(0),
            1 => Some(1),
            _ => None,
        };
        if let Some(slot) = slot {
            self.read_back[slot] = word.to_le_bytes()[0];
        }
        self.done += 1;
        Ok(())
    }
}

/// Runs [`Fe1005000Probe`] over `bus`: whether the board's expander answered.
///
/// # Errors
///
/// Any bus failure.
#[cfg(feature = "std")]
pub(crate) fn detect(bus: &mut dyn Bus) -> Result<bool, Error> {
    let mut probe = Fe1005000Probe::new();
    run_sequence(bus, &mut probe)?;
    Ok(probe.found() == Some(true))
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use super::*;

    /// Runs the check, answering every SPI exchange with `answer(word)` and every write
    /// with zero; returns the actions and the verdict.
    fn run(mut answer: impl FnMut(u32) -> u32) -> (Vec<FpgaAccess>, Option<bool>) {
        let mut probe = Fe1005000Probe::new();
        let mut actions = Vec::new();
        while let Some(SequenceAction::Fpga(access)) = probe.next() {
            actions.push(access);
            let reply = match access {
                FpgaAccess::Spi { word, .. } => answer(word),
                _ => 0,
            };
            probe.complete::<()>(Ok(reply)).expect("no failure");
        }
        (actions, probe.found())
    }

    #[test]
    fn the_check_matches_libusdrs_words_on_an_empty_connector() {
        // libusdr's own trace on the LSDr simulator's empty connector (lsdr-oracle's
        // golden, lines 27-47): seven pin writes, then select-and-exchange pairs.
        let (actions, found) = run(|_| u32::MAX);
        let words: Vec<(u32, u32)> = actions
            .iter()
            .map(|access| match *access {
                FpgaAccess::Write { reg, value } => (reg, value),
                FpgaAccess::Spi { word, .. } => (u32::MAX, word),
                FpgaAccess::Read { .. } => unreachable!("the check reads no register"),
            })
            .collect();
        let select = (58, 0x4719);
        let spi = |word| (u32::MAX, word);
        assert_eq!(
            words,
            [
                (3, 0xf000_0002),
                (3, 0xf400_0002),
                (3, 0xf800_0002),
                (3, 0xec00_0002),
                (3, 0xd000_0002),
                (3, 0xdc00_0002),
                (3, 0xd800_0002),
                select,
                spi(0x8cff),
                select,
                spi(0x0c03),
                select,
                spi(0x0ec0),
                select,
                spi(0x0400),
                select,
                spi(0x0600),
                select,
                spi(0x8cff),
                select,
                spi(0x8eff),
            ]
        );
        assert_eq!(found, Some(false), "an all-ones read-back is no expander");
    }

    #[test]
    fn an_expander_that_keeps_its_configuration_is_found() {
        let (_, found) = run(|word| match word {
            0x8cff => 0x03,
            0x8eff => 0xc0,
            _ => 0,
        });
        assert_eq!(found, Some(true));
    }

    #[test]
    fn only_the_final_read_back_decides() {
        // The first GCR1 read is libusdr's too, and its value is overwritten unused.
        let mut first = true;
        let (_, found) = run(move |word| match word {
            0x8cff if first => {
                first = false;
                0x03
            }
            _ => 0xff,
        });
        assert_eq!(found, Some(false));
    }

    #[test]
    fn a_failed_exchange_names_the_expander_register_and_stops() {
        let mut probe = Fe1005000Probe::new();
        for _ in 0..=SETUP.len() {
            probe
                .complete::<()>(Ok(0))
                .expect("pins and the select land");
        }
        let failure = probe.complete(Err(())).expect_err("the exchange fails");
        assert_eq!(
            failure.access(),
            Some((XRA1405, rsl_lowlevel::Access::Read, 0x0c))
        );
        assert_eq!(probe.next(), None);
        assert_eq!(probe.found(), None);
    }
}
