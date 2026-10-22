//! fe4ch4007200, the four-channel 400–7200 MHz front end, on I2C core 1 of the front-end
//! connector (`device/ext_fe_ch4_400_7200/ext_fe_ch4_400_7200.c`): the TMP114 sensor U69
//! (bus 0, 0x4e), the `DACx0501` (bus 0, 0x48) and four TCA6424A expanders (U114 and U110
//! on bus 0, U300 and U301 on bus 1, at 0x22 and 0x23).
//!
//! No datasheet of these chips is vendored; the frames are libusdr's (`hw/tmp114`,
//! `hw/dac80501`, `hw/tca6424a`). Assumptions, each a sim choice:
//! - TMP114 and `DACx0501`: 16-bit registers behind an 8-bit pointer, most significant
//!   byte first (libusdr's `{reg, hi, lo}` writes and two-byte reads). The sensor reads
//!   `Device_ID` 0x1114 (what libusdr requires), unique ID `1234 5678 9abc`, and 25 °C
//!   (0x0C80, 1/128 °C); the DAC reads `DEVID` 0x0195 (a DAC80501M, as `sdr-frontend-sim`'s).
//!   Other registers are plain storage, resetting to 0.
//! - TCA6424A: byte registers, the input, output, inversion and configuration ports at
//!   0–2, 4–6, 8–10 and 12–14 (`enum tca6424a_regs`); bit 7 of the register byte makes
//!   the pointer advance past each byte (libusdr's `0x80 | reg`), otherwise it stays.
//!   Outputs and configurations power on as 0xFF, inversions as 0; the input ports read
//!   0xFF (pulled up; libusdr never reads them) and ignore writes.
//! - The fits in [`Fe4ch4007200Fit`] take a chip off the bus or change its ID; an absent
//!   chip reads the idle bus, as the board's other absent chips do.

use std::collections::BTreeMap;

use super::I2cChip;

/// The I2C devices of the board, as `(bus, address)` on core 1.
const SENSOR: (u8, u16) = (0, 0x4e);
/// See [`SENSOR`].
const DAC: (u8, u16) = (0, 0x48);
/// U114, U110, U300 and U301, as [`SENSOR`].
const EXPANDERS: [(u8, u16); 4] = [(0, 0x22), (0, 0x23), (1, 0x22), (1, 0x23)];

/// Which fe4ch4007200 the connector carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fe4ch4007200Fit {
    /// Every chip answers as libusdr expects.
    Whole,
    /// U301 is off the bus, so its test vector reads back all ones.
    DeadExpander,
    /// The DAC is off the bus (its ID reads 0xFFFF, which libusdr takes for an unknown
    /// DAC).
    WithoutDac,
    /// The DAC reads ID 0x0000, which libusdr takes for no DAC.
    SilentDac,
}

/// 16-bit registers behind an 8-bit pointer, most significant byte first.
#[derive(Debug)]
struct WordRegisters(BTreeMap<u8, u16>);

impl I2cChip for WordRegisters {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let [pointer, data @ ..] = write else {
            return vec![0xff; read_len];
        };
        if let [high, low] = data {
            self.0.insert(*pointer, u16::from_be_bytes([*high, *low]));
        }
        let word = self.0.get(pointer).copied().unwrap_or(0).to_be_bytes();
        word.into_iter().cycle().take(read_len).collect()
    }
}

/// One TCA6424A.
#[derive(Debug)]
struct Expander([u8; 16]);

impl Expander {
    /// The register byte's auto-increment flag.
    const AUTO_INCREMENT: u8 = 0x80;

    /// A chip at power-on.
    fn new() -> Self {
        let mut regs = [0; 16];
        for port in 0..3 {
            regs[port] = 0xff;
            regs[4 + port] = 0xff;
            regs[12 + port] = 0xff;
        }
        Self(regs)
    }
}

impl I2cChip for Expander {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let [command, data @ ..] = write else {
            return vec![0xff; read_len];
        };
        let step = u8::from(command & Self::AUTO_INCREMENT != 0);
        let mut reg = command & !Self::AUTO_INCREMENT;
        for &byte in data {
            if let Some(slot) = self.0.get_mut(usize::from(reg)).filter(|_| reg >= 4) {
                *slot = byte;
            }
            reg = reg.wrapping_add(step);
        }
        (0..read_len)
            .map(|_| {
                let byte = self.0.get(usize::from(reg)).copied().unwrap_or(0);
                reg = reg.wrapping_add(step);
                byte
            })
            .collect()
    }
}

/// The board's chips.
#[derive(Debug)]
pub(crate) struct Fe4ch4007200 {
    /// The TMP114.
    sensor: WordRegisters,
    /// The DAC, unless off the bus.
    dac: Option<WordRegisters>,
    /// U114, U110, U300 and U301, unless off the bus.
    expanders: [Option<Expander>; 4],
}

impl Fe4ch4007200 {
    /// The board as `fit` says, at power-on.
    pub(crate) fn new(fit: Fe4ch4007200Fit) -> Self {
        let sensor = WordRegisters(BTreeMap::from([
            (0x00, 0x0c80),
            (0x08, 0x1234),
            (0x09, 0x5678),
            (0x0a, 0x9abc),
            (0x0b, 0x1114),
        ]));
        let dac_id = match fit {
            Fe4ch4007200Fit::SilentDac => 0,
            _ => 0x0195,
        };
        let dac = (fit != Fe4ch4007200Fit::WithoutDac)
            .then(|| WordRegisters(BTreeMap::from([(0x01, dac_id)])));
        let mut expanders = [(); 4].map(|()| Some(Expander::new()));
        if fit == Fe4ch4007200Fit::DeadExpander {
            expanders[3] = None;
        }
        Self {
            sensor,
            dac,
            expanders,
        }
    }

    /// The chip at `addr` on `bus` of core 1, if one answers there.
    pub(crate) fn chip(&mut self, bus: u8, addr: u16) -> Option<&mut dyn I2cChip> {
        match (bus, addr) {
            SENSOR => Some(&mut self.sensor),
            DAC => self.dac.as_mut().map(|dac| dac as &mut dyn I2cChip),
            at => {
                let index = EXPANDERS.iter().position(|&expander| expander == at)?;
                self.expanders[index]
                    .as_mut()
                    .map(|expander| expander as &mut dyn I2cChip)
            }
        }
    }

    /// Register `reg` of expander `index` (0 U114, 1 U110, 2 U300, 3 U301), unless off the
    /// bus.
    pub(crate) fn expander_reg(&self, index: usize, reg: u8) -> Option<u8> {
        let expander = self.expanders.get(index)?.as_ref()?;
        expander.0.get(usize::from(reg)).copied()
    }

    /// The DAC's register `reg`, unless off the bus.
    pub(crate) fn dac_reg(&self, reg: u8) -> Option<u16> {
        Some(self.dac.as_ref()?.0.get(&reg).copied().unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expanders_auto_increment_only_when_asked_and_keep_their_inputs() {
        let mut board = Fe4ch4007200::new(Fe4ch4007200Fit::Whole);
        let u114 = board.chip(0, 0x22).expect("fitted");
        u114.transfer(&[0x84, 0x70, 0xf0], 0);
        assert_eq!(u114.transfer(&[0x84], 2), vec![0x70, 0xf0]);
        assert_eq!(u114.transfer(&[0x04], 2), vec![0x70, 0x70]);
        u114.transfer(&[0x00, 0x12], 0);
        assert_eq!(board.expander_reg(0, 0), Some(0xff));
    }

    #[test]
    fn the_sensor_and_dac_read_most_significant_byte_first() {
        let mut board = Fe4ch4007200::new(Fe4ch4007200Fit::Whole);
        let sensor = board.chip(0, 0x4e).expect("fitted");
        assert_eq!(sensor.transfer(&[0x0b], 2), vec![0x11, 0x14]);
        let dac = board.chip(0, 0x48).expect("fitted");
        dac.transfer(&[0x04, 0x01, 0x01], 0);
        assert_eq!(board.dac_reg(0x04), Some(0x0101));
        assert!(
            Fe4ch4007200::new(Fe4ch4007200Fit::DeadExpander)
                .chip(1, 0x23)
                .is_none()
        );
    }
}
