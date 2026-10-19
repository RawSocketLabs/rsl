//! The 100–5000 MHz front end (libusdr's `fe1005000`,
//! `device/ext_fe_100_5000/ext_fe_100_5000.c`), the `LSDr`'s board: a double-conversion
//! receive path with four RF bands, two IF filters and an LNA preselector switched by an
//! XRA1405 GPIO expander, two MAX2871 LOs and an RFSA3713 attenuator, all behind the
//! connector's SPI bus.
//!
//! libusdr finds the board by its expander: it routes the connector's SPI pins, writes
//! two configuration registers and reads them back. It then tunes the path to 900 MHz
//! (`tune_rf_path`: both LOs and the band switches), ignoring how that went, and reads the
//! expander's second status register for its log. Every access is a register write or an
//! SPI exchange, so each step is a portable sequence: [`Fe1005000Probe`] for the check, then
//! batches of exchanges for the tuning and the controls. Each exchange first selects its
//! chip on the SPI-extension core (libusdr's `s_spi_ext_op`).

#[cfg(feature = "std")]
use deps::tracing::{info, warn};
#[cfg(feature = "std")]
use hal::{Bus, run_sequence};
use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

#[cfg(feature = "std")]
use crate::error::Error;
use crate::gpio::{FPGA, GpioCommand, PinMode};

/// The SPI-extension core's configuration register (`spiext_cfg`, hard-coded in
/// `device_fe_probe`): written before each exchange, it selects the chip and the frame.
pub const REG_SPI_EXT_CONFIG: u32 = 58;

/// The connector's SPI bus (`device_fe_probe` passes 4, the `LSDr`'s `SPI_EXTERNAL`).
pub const SPI_EXTERNAL: SpiAddr = SpiAddr(4);

/// The board's name in errors (libusdr's `fe1005000`).
#[cfg(feature = "std")]
const NAME: &str = "fe1005000";

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

/// The SPI-extension core's clock divider for every chip on the board (`SPI_DIV`).
const SPI_DIVIDER: u32 = 25;

/// A chip's SPI-extension configuration (`MAKE_SPI_CFG(b, cs, div)` = `b << 14 | cs << 8 |
/// div`): the frame-length code and the chip select, with [`SPI_DIVIDER`].
const fn spi_config(frame: u32, chip_select: u32) -> u32 {
    frame << 14 | chip_select << 8 | SPI_DIVIDER
}

/// XRA1405 register addresses (`xra1405.yaml`; its datasheet is not vendored, so only the
/// names libusdr gives are used).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Reg {
    /// `GSR2`, 0x02: the second port's pin states, which libusdr only logs.
    #[cfg(feature = "std")]
    Gsr2 = 0x02,

    /// `OCR1`, 0x04: the first port's outputs.
    Ocr1 = 0x04,

    /// `OCR2`, 0x06: the second port's outputs.
    Ocr2 = 0x06,

    /// `GCR1`, 0x0C.
    Gcr1 = 0x0c,

    /// `GCR2`, 0x0E.
    Gcr2 = 0x0e,
}

/// Which MAX2871 (`DEV_LO1`, `DEV_LO2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lo {
    /// The first LO, on chip select 0 (`CS_IDX_LO1`).
    First,

    /// The second LO, on chip select 3 (`CS_IDX_LO2`).
    Second,
}

/// One exchange with a chip behind the SPI-extension core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exchange {
    /// An expander register read (`xra1405il24_reg_rd`).
    Read(Reg),

    /// An expander register write (`xra1405il24_reg_wr`).
    Write(Reg, u8),

    /// A MAX2871 register word, its address in bits 2:0 (`max2871_wr_dat`).
    Synthesizer(Lo, u32),

    /// The attenuator's code (`rfsa3713tr7_reg_wr`), in 0.25 dB steps.
    Attenuator(u8),
}

impl Exchange {
    /// The chip's configuration of the SPI-extension core (`s_dev_cfg`): one-byte frames
    /// on chip select 7 for the expander (`SPI_BYTES_XRA1405IL24`, `CS_IDX_EXPANDER`),
    /// three-byte frames on chip selects 0 and 3 for the LOs, one-byte frames on chip
    /// select 2 for the attenuator.
    const fn config(self) -> u32 {
        match self {
            Self::Read(_) | Self::Write(..) => spi_config(1, 7),
            Self::Synthesizer(Lo::First, _) => spi_config(3, 0),
            Self::Synthesizer(Lo::Second, _) => spi_config(3, 3),
            Self::Attenuator(_) => spi_config(1, 2),
        }
    }

    /// The chip's name in errors.
    const fn chip(self) -> &'static str {
        match self {
            Self::Read(_) | Self::Write(..) => "XRA1405",
            Self::Synthesizer(..) => "MAX2871",
            Self::Attenuator(_) => "RFSA3713",
        }
    }

    /// The SPI word, as `spiext_make_data_reg` packs libusdr's bytes, first byte lowest:
    /// for the expander the data byte below the register (bit 7 set to read, the data
    /// 0xFF then); a MAX2871's register word as it is; the attenuator's code, bit-reversed,
    /// above a zero byte.
    fn word(self) -> u32 {
        match self {
            Self::Read(reg) => u32::from(0x80 | reg as u8) << 8 | 0xff,
            Self::Write(reg, value) => u32::from(reg as u8 & 0x7f) << 8 | u32::from(value),
            Self::Synthesizer(_, word) => word,
            Self::Attenuator(code) => u32::from(code.reverse_bits()) << 8,
        }
    }

    /// The exchange as the SPI access that performs it.
    fn access(self) -> FpgaAccess {
        let (reg, write) = match self {
            Self::Read(reg) => (u32::from(reg as u8), false),
            Self::Write(reg, _) => (u32::from(reg as u8), true),
            Self::Synthesizer(_, word) => (word & 0b111, true),
            Self::Attenuator(_) => (0, true),
        };
        FpgaAccess::Spi {
            target: SPI_EXTERNAL,
            word: self.word(),
            reg,
            write,
        }
    }

    /// Action `index` of a run of exchanges, each the configuration write that selects its
    /// chip and then the exchange, with the chip a failure names.
    fn action(exchanges: &[Self], index: usize) -> Option<(FpgaAccess, &'static str)> {
        let exchange = *exchanges.get(index / 2)?;
        Some(if index % 2 == 0 {
            let select = FpgaAccess::Write {
                reg: REG_SPI_EXT_CONFIG,
                value: exchange.config(),
            };
            (select, FPGA)
        } else {
            (exchange.access(), exchange.chip())
        })
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
        match SETUP.get(index) {
            Some(command) => Some((command.access(), FPGA)),
            None => Exchange::action(&CHECK, index.checked_sub(SETUP.len())?),
        }
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

/// The most exchanges one control makes: six words to each LO, then both band-switch
/// registers.
const BATCH: usize = 14;

/// A run of exchanges a control makes, as a register-only sequence: each a configuration
/// write selecting its chip, then the exchange. It stops at the first failure.
#[derive(Clone, Copy, Debug)]
struct Batch {
    /// The exchanges, the first `len` of them used.
    exchanges: [Exchange; BATCH],
    /// How many are used.
    len: usize,
    /// Actions completed; past the batch's once one failed.
    done: usize,
    /// The byte the last expander read returned.
    read: u8,
}

impl Batch {
    /// No exchanges yet.
    const fn new() -> Self {
        Self {
            exchanges: [Exchange::Attenuator(0); BATCH],
            len: 0,
            done: 0,
            read: 0xff,
        }
    }

    /// A batch of the one exchange `exchange`.
    #[cfg(feature = "std")]
    const fn of(exchange: Exchange) -> Self {
        let mut batch = Self::new();
        batch.exchanges[0] = exchange;
        batch.len = 1;
        batch
    }

    /// Appends `exchange`.
    fn push(&mut self, exchange: Exchange) {
        self.exchanges[self.len] = exchange;
        self.len += 1;
    }

    /// The exchanges used.
    fn used(&self) -> &[Exchange] {
        &self.exchanges[..self.len]
    }
}

impl Sequence for Batch {
    fn next(&self) -> Option<SequenceAction> {
        Exchange::action(self.used(), self.done).map(|(access, _)| SequenceAction::Fpga(access))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some((access, chip)) = Exchange::action(self.used(), self.done) else {
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
        if matches!(access, FpgaAccess::Spi { write: false, .. }) {
            self.read = word.to_le_bytes()[0];
        }
        self.done += 1;
        Ok(())
    }
}

/// One fe1005000 control as a register-only sequence: what it writes, and why libusdr
/// refuses it partway (its `-EINVAL`), if it does. libusdr stops at the refusal, after the
/// writes before it; so does this sequence, and [`Self::refused`] then says why.
#[derive(Clone, Copy, Debug)]
pub struct Fe1005000Control {
    /// The exchanges, up to the refusal.
    batch: Batch,
    /// Why the control was refused.
    refused: Option<&'static str>,
}

impl Fe1005000Control {
    /// Why libusdr refuses the control, once the writes before the refusal are done.
    #[must_use]
    pub const fn refused(&self) -> Option<&'static str> {
        self.refused
    }

    /// Nothing written, nothing refused.
    const fn new() -> Self {
        Self {
            batch: Batch::new(),
            refused: None,
        }
    }

    /// Appends `exchange`, unless the control was refused already.
    fn push(&mut self, exchange: Exchange) {
        if self.refused.is_none() {
            self.batch.push(exchange);
        }
    }

    /// Refuses the control for `reason`, unless it was refused already.
    fn refuse(&mut self, reason: &'static str) {
        self.refused.get_or_insert(reason);
    }

    /// Appends the words `max2871_init` writes to `lo` for `khz`: none for 0 or above
    /// 6 GHz; the power-down words for 1 (`TCXO only`); otherwise its auto-integer mode,
    /// the band's output divider and power (`max2871_band_sel`), the fixed registers 3 and
    /// 2, the modulus and the N and fraction words. Below 93.75 MHz there is no band:
    /// refused after the mode word.
    fn synthesizer(&mut self, lo: Lo, khz: u32) {
        const POWER_DOWN: [u32; 6] = [
            0x03c0_0005,
            0x1c9a_0cdc,
            0x0200_1f23,
            0x0401_1e72,
            0x2001_03e9,
            0x0123_8000,
        ];
        /// The phase detector frequency: the 40 MHz TCXO halved (`F_PFD_KHZ`), in kHz.
        const PFD_KHZ: u32 = 20_000;
        /// The fractional modulus (`modls`).
        const MODULUS: u32 = 4095;

        if khz == 0 || khz > 6_000_000 {
            return;
        }
        if khz == 1 {
            for word in POWER_DOWN {
                self.push(Exchange::Synthesizer(lo, word));
            }
            return;
        }
        self.push(Exchange::Synthesizer(lo, 0x0140_0005));
        let Some((divider, register4)) = output_divider(khz) else {
            self.refuse("LO frequency below 93.75 MHz");
            return;
        };
        let vco_khz = khz * divider;
        let integer = vco_khz / PFD_KHZ;
        let fraction = (vco_khz - integer * PFD_KHZ) * MODULUS / PFD_KHZ;
        for word in [
            register4,
            0x0000_1f23,
            0x0800_9e42,
            0x2001_0001 | MODULUS << 3,
            integer << 15 | fraction << 3,
        ] {
            self.push(Exchange::Synthesizer(lo, word));
        }
    }
}

impl Sequence for Fe1005000Control {
    fn next(&self) -> Option<SequenceAction> {
        self.batch.next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        self.batch.complete(result)
    }
}

/// A MAX2871 band's output divider and register 4 (`max2871_band_sel`: output power,
/// divider and mute-until-lock), for an output of `khz`; `None` below 93.75 MHz. The flatness
/// correction libusdr also picks is unused.
const fn output_divider(khz: u32) -> Option<(u32, u32)> {
    /// Register 4's fixed bits.
    const BASE: u32 = 0x6085_0424;
    let (divider, code, power) = match khz {
        4_800_000.. => (1, 0, 3),
        3_000_000.. => (1, 0, 1),
        1_500_000.. => (2, 1, 2),
        1_200_000.. => (4, 2, 2),
        750_000.. => (4, 2, 3),
        375_000.. => (8, 3, 3),
        187_500.. => (16, 4, 3),
        93_750.. => (32, 5, 3),
        _ => return None,
    };
    Some((divider, BASE | code << 20 | power << 3))
}

/// The receive band switched in (libusdr's `band`, 0 to 3 for bands 1 to 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fe1005000Band {
    /// Band 1, 20–1950 MHz (0).
    #[default]
    Band1,

    /// Band 2, 1950–2650 MHz (1).
    Band2,

    /// Band 3, 2650–3550 MHz (2).
    Band3,

    /// Band 4, 3550–4950 MHz (3).
    Band4,

    /// No band: every band powered down (any other value libusdr is given).
    Off,
}

/// The first IF filter switched in (libusdr's `ifsel`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fe1005000IfFilter {
    /// The 1224 MHz filter (0).
    #[default]
    Mhz1224,

    /// The 2250 MHz filter (1).
    Mhz2250,
}

/// A manual tuning of fe1005000 (`ext_fe_100_5000_rf_manual`, libusdr's
/// `/dm/sdr/0/fe/{lo1,lo2,band,ifsel}`): both LOs, in kHz, and the switches. An LO of 0 or
/// above 6 GHz is left alone, 1 powers its synthesizer down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fe1005000Manual {
    /// The first LO, in kHz.
    pub lo1_khz: u32,

    /// The second LO, in kHz.
    pub lo2_khz: u32,

    /// The band.
    pub band: Fe1005000Band,

    /// The first IF filter.
    pub if_filter: Fe1005000IfFilter,
}

/// One fe1005000 control, as libusdr's `/debug/hw/fe_100_5000_cmd/0/reg` commands set them
/// (`ext_fe_100_5000_cmd_wr`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fe1005000Setting {
    /// Tunes the path to a frequency in kHz (`FE_FREQ`): band, IF filter and both LOs.
    FrequencyKhz(u32),

    /// The attenuation, in 0.25 dB steps (`FE_ATTN`).
    Attenuation(u8),

    /// The LNA preselector, two bits (`FE_PRESEL`, `ext_fe_100_5000_set_lna`).
    Preselector(u8),
}

/// The 100–5000 MHz front end's settings, as libusdr tracks them (`ext_fe_100_5000_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fe1005000 {
    /// The band switched in.
    band: Fe1005000Band,
    /// The first IF filter.
    if_filter: Fe1005000IfFilter,
    /// The LNA preselector (`gain`), of which two bits are used.
    preselector: u8,
    /// The attenuation, in 0.25 dB steps (`atten`).
    attenuation: u8,
    /// The frequency last asked for, in kHz (`lofreq_khz`).
    frequency_khz: u32,
}

impl Fe1005000 {
    /// The tuning libusdr's bring-up asks for, in kHz.
    #[cfg(any(feature = "std", test))]
    const INITIAL_KHZ: u32 = 900_000;

    /// The settings libusdr's bring-up starts from: band 1, the 1224 MHz filter, no
    /// preselector or attenuation, 900 MHz.
    #[cfg(any(feature = "std", test))]
    const fn initial() -> Self {
        Self {
            band: Fe1005000Band::Band1,
            if_filter: Fe1005000IfFilter::Mhz1224,
            preselector: 0,
            attenuation: 0,
            frequency_khz: Self::INITIAL_KHZ,
        }
    }

    /// The frequency last asked for, in kHz (`FE_FREQ` read).
    #[must_use]
    pub const fn frequency_khz(&self) -> u32 {
        self.frequency_khz
    }

    /// The attenuation, in 0.25 dB steps (`FE_ATTN` read).
    #[must_use]
    pub const fn attenuation(&self) -> u8 {
        self.attenuation
    }

    /// The LNA preselector (`FE_PRESEL` read).
    #[must_use]
    pub const fn preselector(&self) -> u8 {
        self.preselector
    }

    /// The band switched in.
    #[must_use]
    pub const fn band(&self) -> Fe1005000Band {
        self.band
    }

    /// The first IF filter switched in.
    #[must_use]
    pub const fn if_filter(&self) -> Fe1005000IfFilter {
        self.if_filter
    }

    /// Plans one control (`ext_fe_100_5000_cmd_wr`): a frequency tunes the path
    /// (`tune_rf_path`: band and IF filter, both LOs, the band switches), the attenuation
    /// writes the attenuator, the preselector the band switches. The setting is kept even
    /// when the writes fail or libusdr refuses it, as libusdr keeps it.
    pub fn control(&mut self, setting: Fe1005000Setting) -> Fe1005000Control {
        let mut plan = Fe1005000Control::new();
        match setting {
            Fe1005000Setting::FrequencyKhz(khz) => {
                self.frequency_khz = khz;
                plan = self.plan_tune(khz);
            }
            Fe1005000Setting::Attenuation(code) => {
                self.attenuation = code;
                plan.push(Exchange::Attenuator(code));
            }
            Fe1005000Setting::Preselector(code) => {
                self.preselector = code;
                let [low, high] = self.outputs().to_le_bytes();
                plan.push(Exchange::Write(Reg::Ocr1, low));
                plan.push(Exchange::Write(Reg::Ocr2, high));
            }
        }
        plan
    }

    /// Plans a manual tuning (`ext_fe_100_5000_rf_manual`): the second LO, the first, then
    /// the band switches as given. The band and filter are kept even when the writes fail.
    pub fn manual(&mut self, manual: Fe1005000Manual) -> Fe1005000Control {
        self.band = manual.band;
        self.if_filter = manual.if_filter;
        let mut plan = Fe1005000Control::new();
        self.plan_switches(&mut plan, manual.lo1_khz, manual.lo2_khz);
        plan
    }

    /// Plans `tune_rf_path` to `khz`: past 6 GHz or below 20 MHz refused before any
    /// write; otherwise the band and IF filter for it, the second LO then the first (the
    /// IF minus or plus the frequency), and the band switches.
    fn plan_tune(&mut self, khz: u32) -> Fe1005000Control {
        /// The first IFs (`IF1_FREQ1`, `IF1_FREQ2`) and the second (`IF2_FREQ`), in kHz.
        const IF1_LOW: u32 = 1_224_000;
        /// See [`IF1_LOW`].
        const IF1_HIGH: u32 = 2_250_000;
        /// See [`IF1_LOW`].
        const IF2: u32 = 312_500;

        let mut plan = Fe1005000Control::new();
        let (band, if_filter, lo2, lo1) = match khz {
            6_000_001.. => {
                plan.refuse("frequency above 6 GHz");
                return plan;
            }
            4_625_000.. => (
                Fe1005000Band::Band4,
                Fe1005000IfFilter::Mhz2250,
                IF1_HIGH - IF2,
                khz - IF1_HIGH,
            ),
            3_550_000.. => (
                Fe1005000Band::Band4,
                Fe1005000IfFilter::Mhz1224,
                IF1_LOW + IF2,
                khz - IF1_LOW,
            ),
            2_650_000.. => (
                Fe1005000Band::Band3,
                Fe1005000IfFilter::Mhz1224,
                IF1_LOW - IF2,
                khz - IF1_LOW,
            ),
            1_950_000.. => (
                Fe1005000Band::Band2,
                Fe1005000IfFilter::Mhz1224,
                IF1_LOW - IF2,
                khz + IF1_LOW,
            ),
            20_000.. => (
                Fe1005000Band::Band1,
                Fe1005000IfFilter::Mhz2250,
                IF1_HIGH + IF2,
                khz + IF1_HIGH,
            ),
            _ => {
                plan.refuse("frequency below 20 MHz");
                return plan;
            }
        };
        self.band = band;
        self.if_filter = if_filter;
        self.plan_switches(&mut plan, lo1, lo2);
        plan
    }

    /// Plans both LOs, the second first, then the band switches (the tail of
    /// `tune_rf_path` and `ext_fe_100_5000_rf_manual`).
    fn plan_switches(self, plan: &mut Fe1005000Control, lo1_khz: u32, lo2_khz: u32) {
        plan.synthesizer(Lo::Second, lo2_khz);
        plan.synthesizer(Lo::First, lo1_khz);
        let outputs = self.outputs();
        let [low, high] = outputs.to_le_bytes();
        plan.push(Exchange::Write(Reg::Ocr1, low));
        plan.push(Exchange::Write(Reg::Ocr2, high));
    }

    /// The expander's outputs for the current settings (`update_band_sel`): the IF filter
    /// (bit 10), band 1's enable (9), bands 2 to 4's power-downs (8, 7, 6), the band
    /// selector (5:4), the preselector (3:2), and the DC and RF enables (1:0) on.
    const fn outputs(self) -> u16 {
        // Band selector codes (`SB_BAND1` to `SB_BAND4`) and which band is powered.
        let (selector, band1_on, band2_off, band3_off, band4_off) = match self.band {
            Fe1005000Band::Band1 => (3, true, true, true, true),
            Fe1005000Band::Band2 => (0, false, false, true, true),
            Fe1005000Band::Band3 => (2, false, true, false, true),
            Fe1005000Band::Band4 => (1, false, true, true, false),
            Fe1005000Band::Off => (0, false, true, true, true),
        };
        let if_high = matches!(self.if_filter, Fe1005000IfFilter::Mhz2250);
        (if_high as u16) << 10
            | (band1_on as u16) << 9
            | (band2_off as u16) << 8
            | (band3_off as u16) << 7
            | (band4_off as u16) << 6
            | selector << 4
            | (self.preselector as u16 & 0b11) << 2
            | 0b11
    }
}

/// Runs [`Fe1005000Probe`] over `bus`: whether the board's expander answered.
///
/// # Errors
///
/// Any bus failure.
#[cfg(feature = "std")]
fn detect(bus: &mut dyn Bus) -> Result<bool, Error> {
    let mut probe = Fe1005000Probe::new();
    run_sequence(bus, &mut probe)?;
    Ok(probe.found() == Some(true))
}

#[cfg(feature = "std")]
impl Fe1005000 {
    /// Looks for the board and brings it up (`ext_fe_100_5000_init`): the expander check,
    /// then the path tuned to 900 MHz, then `GSR2` read for the log. libusdr ignores how
    /// the tuning went, so a failed exchange there is logged and the read follows. `None`
    /// when the expander does not keep its configuration.
    ///
    /// # Errors
    ///
    /// Any bus failure but the tuning's.
    pub(crate) fn init(bus: &mut dyn Bus) -> Result<Option<Self>, Error> {
        if !detect(bus)? {
            return Ok(None);
        }
        let mut board = Self::initial();
        let mut plan = board.plan_tune(Self::INITIAL_KHZ);
        if let Err(error) = run_sequence(bus, &mut plan.batch) {
            warn!(error = %Error::from(error), "fe1005000's tuning failed; carrying on as libusdr does");
        }
        let mut status = Batch::of(Exchange::Read(Reg::Gsr2));
        run_sequence(bus, &mut status)?;
        info!(gsr2 = status.read, "fe1005000 expander status");
        Ok(Some(board))
    }

    /// Applies one control (`ext_fe_100_5000_cmd_wr`); see [`Self::control`].
    ///
    /// # Errors
    ///
    /// [`Error::InvalidSetting`] for a frequency below 20 MHz or above 6 GHz, before any
    /// bus access; any bus failure.
    pub fn apply(&mut self, bus: &mut dyn Bus, setting: Fe1005000Setting) -> Result<(), Error> {
        self.control(setting).run(bus)
    }

    /// Tunes both LOs and sets the switches as given; see [`Self::manual`].
    ///
    /// # Errors
    ///
    /// [`Error::InvalidSetting`] for an LO below 93.75 MHz (but 0 and 1), after the writes
    /// before it, as libusdr refuses it; any bus failure.
    pub fn tune_manually(
        &mut self,
        bus: &mut dyn Bus,
        manual: Fe1005000Manual,
    ) -> Result<(), Error> {
        self.manual(manual).run(bus)
    }
}

#[cfg(feature = "std")]
impl Fe1005000Control {
    /// Runs the writes over `bus`, then reports the refusal, if any.
    fn run(mut self, bus: &mut dyn Bus) -> Result<(), Error> {
        run_sequence(bus, &mut self)?;
        self.refused.map_or(Ok(()), |reason| {
            Err(Error::InvalidSetting {
                board: NAME,
                reason,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use super::*;

    /// Runs `sequence`, answering every SPI exchange with `answer(word)` and every write
    /// with zero; returns the actions.
    fn drive(sequence: &mut impl Sequence, mut answer: impl FnMut(u32) -> u32) -> Vec<FpgaAccess> {
        let mut actions = Vec::new();
        while let Some(SequenceAction::Fpga(access)) = sequence.next() {
            actions.push(access);
            let reply = match access {
                FpgaAccess::Spi { word, .. } => answer(word),
                _ => 0,
            };
            sequence.complete::<()>(Ok(reply)).expect("no failure");
        }
        actions
    }

    /// Runs the check; returns the actions and the verdict.
    fn run(answer: impl FnMut(u32) -> u32) -> (Vec<FpgaAccess>, Option<bool>) {
        let mut probe = Fe1005000Probe::new();
        let actions = drive(&mut probe, answer);
        (actions, probe.found())
    }

    /// The SPI words a batch shifts out, with the configuration selected before each.
    fn words(batch: &mut Batch) -> Vec<(u32, u32)> {
        let actions = drive(batch, |_| u32::MAX);
        actions
            .chunks(2)
            .map(|pair| match pair {
                [
                    FpgaAccess::Write {
                        reg: REG_SPI_EXT_CONFIG,
                        value,
                    },
                    FpgaAccess::Spi { word, .. },
                ] => (*value, *word),
                other => panic!("not a select-and-exchange pair: {other:?}"),
            })
            .collect()
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
        assert_eq!(failure.access(), Some(("XRA1405", hal::Access::Read, 0x0c)));
        assert_eq!(probe.next(), None);
        assert_eq!(probe.found(), None);
    }

    #[test]
    fn the_bring_ups_900_mhz_tuning_programs_both_los_then_band_1() {
        // Band 1 with the 2250 MHz IF: LO2 = 2250 + 312.5 MHz, LO1 = 900 + 2250 MHz.
        // MAX2871 words from `max2871_band_sel`'s arithmetic, worked by hand: LO2 at
        // 2562.5 MHz takes divider 2 (VCO 5125 MHz: N 256, fraction 1023); LO1 at 3150 MHz
        // divider 1 (N 157, fraction 2047).
        let mut board = Fe1005000::initial();
        let mut plan = board.plan_tune(900_000);
        assert_eq!(plan.refused, None);
        let lo2 = 0xc319;
        let lo1 = 0xc019;
        let expander = 0x4719;
        assert_eq!(
            words(&mut plan.batch),
            [
                (lo2, 0x0140_0005),
                (lo2, 0x6095_0434),
                (lo2, 0x0000_1f23),
                (lo2, 0x0800_9e42),
                (lo2, 0x2001_7ff9),
                (lo2, 256 << 15 | 1023 << 3),
                (lo1, 0x0140_0005),
                (lo1, 0x6085_042c),
                (lo1, 0x0000_1f23),
                (lo1, 0x0800_9e42),
                (lo1, 0x2001_7ff9),
                (lo1, 157 << 15 | 2047 << 3),
                // IF 2250 (bit 10), band 1 on (9), bands 2-4 down (8-6), selector 3.
                (expander, 0x0400 | 0xf3),
                (expander, 0x0600 | 0x07),
            ]
        );
        assert_eq!(
            (board.band(), board.if_filter()),
            (Fe1005000Band::Band1, Fe1005000IfFilter::Mhz2250)
        );
    }

    #[test]
    fn frequencies_outside_20_mhz_to_6_ghz_are_refused_untouched() {
        for khz in [19_999, 6_000_001] {
            let mut board = Fe1005000::initial();
            let plan = board.plan_tune(khz);
            assert!(plan.refused.is_some(), "{khz} kHz");
            assert_eq!(plan.batch.len, 0, "{khz} kHz: nothing written");
            assert_eq!(board.band(), Fe1005000Band::Band1, "{khz} kHz: band kept");
        }
    }

    #[test]
    fn a_manual_lo_below_the_lowest_band_is_refused_after_its_mode_word() {
        let board = Fe1005000::initial();
        let mut plan = Fe1005000Control::new();
        board.plan_switches(&mut plan, 900_000, 50_000);
        assert_eq!(plan.refused, Some("LO frequency below 93.75 MHz"));
        assert_eq!(words(&mut plan.batch), [(0xc319, 0x0140_0005)]);
    }

    #[test]
    fn lo_zero_is_skipped_and_one_powers_the_synthesizer_down() {
        let board = Fe1005000::initial();
        let mut plan = Fe1005000Control::new();
        board.plan_switches(&mut plan, 1, 0);
        let written: Vec<u32> = words(&mut plan.batch)
            .into_iter()
            .map(|(_, word)| word)
            .collect();
        assert_eq!(
            written[..6],
            [
                0x03c0_0005,
                0x1c9a_0cdc,
                0x0200_1f23,
                0x0401_1e72,
                0x2001_03e9,
                0x0123_8000
            ]
        );
        assert_eq!(
            written.len(),
            8,
            "LO1's six power-down words and the switches"
        );
    }

    #[test]
    fn the_attenuator_takes_its_code_bit_reversed_above_a_zero_byte() {
        // `rfsa3713tr7_reg_wr` reverses the byte into the frame's second byte.
        assert_eq!(Exchange::Attenuator(0b0000_0001).word(), 0x8000);
        assert_eq!(Exchange::Attenuator(0x7f).word(), 0xfe00);
        assert_eq!(Exchange::Attenuator(1).config(), 0x4219);
    }

    /// A connector with fe1005000's expander answering its check, whose second LO's
    /// first exchange fails; every SPI word shifted out is recorded.
    #[cfg(feature = "std")]
    #[derive(Default)]
    struct FailingLo {
        /// The SPI-extension configuration last written.
        config: u32,
        /// The SPI words shifted out, with the configuration selected for each.
        words: Vec<(u32, u32)>,
    }

    #[cfg(feature = "std")]
    impl Bus for FailingLo {
        fn read_regs(&mut self, _: u32, out: &mut [u32]) -> Result<(), hal::BusError> {
            out.fill(0);
            Ok(())
        }

        fn write_regs(&mut self, reg: u32, values: &[u32]) -> Result<(), hal::BusError> {
            if reg == REG_SPI_EXT_CONFIG {
                self.config = values[0];
            }
            Ok(())
        }

        fn spi32(&mut self, _: SpiAddr, word: u32) -> Result<u32, hal::BusError> {
            self.words.push((self.config, word));
            match (self.config, word) {
                (0xc319, _) => Err(hal::BusError::Disconnected),
                (_, 0x8cff) => Ok(0x03),
                (_, 0x8eff) => Ok(0xc0),
                _ => Ok(0),
            }
        }

        fn i2c(&mut self, _: hal::I2cAddr, _: &[u8], _: &mut [u8]) -> Result<(), hal::BusError> {
            Ok(())
        }

        fn sleep(&mut self, _: std::time::Duration) {}
    }

    #[test]
    #[cfg(feature = "std")]
    fn a_failed_bring_up_tuning_is_ignored_and_the_status_still_read() {
        // libusdr ignores `tune_rf_path`'s result in `ext_fe_100_5000_init`: the tuning stops
        // at the failed LO word, then `GSR2` is read and the board comes up.
        let mut bus = FailingLo::default();
        let board = Fe1005000::init(&mut bus)
            .expect("the tuning's failure is not the bring-up's")
            .expect("the expander answered");
        assert_eq!(board.frequency_khz(), 900_000);
        let tail: Vec<(u32, u32)> = bus.words[CHECK.len()..].to_vec();
        assert_eq!(
            tail,
            [(0xc319, 0x0140_0005), (0x4719, 0x82ff)],
            "LO2's first word fails; nothing else is tuned before the status read"
        );
    }
}
