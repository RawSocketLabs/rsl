//! The 100–5000 MHz front end (libusdr's `fe1005000`, `device/ext_fe_100_5000`), on the
//! connector's SPI bus behind the SPI-extension core: an XRA1405 GPIO expander switching
//! the RF path, two MAX2871 synthesizers and an RFSA3713 attenuator. Only the `LSDr`
//! admits the board.
//!
//! libusdr writes the core's configuration register before every exchange
//! (`s_spi_ext_op`); its chip-select field (bits 10:8, `MAKE_SPI_CFG`) picks the chip:
//! 7 the expander, 0 the first LO, 3 the second, 2 the attenuator. Another chip select
//! reaches nothing, and reads the floating bus.
//!
//! Modelling assumptions (no datasheet of these chips is vendored; libusdr's
//! `ext_fe_100_5000.c` and `hw/xra1405/xra1405.yaml` are the evidence):
//! - XRA1405: a 16-bit frame in the word's low half, bit 15 set to read, the register
//!   (the YAML's addresses, 0x00 to 0x2A) in bits 14:8 and the data in 7:0, as libusdr
//!   packs it (`xra1405il24_reg_wr`/`_rd`, `MAKE_XRA1405_REG_RD`). A read returns the
//!   register in bits 7:0 and zeros above; a write returns 0. Every register is plain
//!   storage, powering on as 0xFF (all pins inputs, outputs high; an assumption only the
//!   check's first, ignored `GCR1` read sees), except the input status registers
//!   (`GSR1`, `GSR2`): an output pin (its `GCR` bit 0) reads its `OCR` latch and an input
//!   pin reads 1, as if pulled up. libusdr only logs `GSR2`.
//! - MAX2871 and RFSA3713: write-only (MISO not driven: the floating bus, all ones). The
//!   MAX2871 keeps each 32-bit register by its address (bits 2:0); the attenuator keeps
//!   the code libusdr sends bit-reversed in the frame's second byte.

/// The expander's input status registers, `GSR1` and `GSR2`.
const STATUS: [u8; 2] = [0x00, 0x02];
/// The expander's direction registers, `GCR1` and `GCR2`, a 1 bit an input.
const DIRECTION: [u8; 2] = [0x0c, 0x0e];
/// The expander's output registers, `OCR1` and `OCR2`.
const OUTPUT: [u8; 2] = [0x04, 0x06];
/// The read flag in the expander's frame.
const READ: u32 = 0x8000;

/// The chip a configuration's chip select reaches (`CS_IDX_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Select {
    /// `CS_IDX_EXPANDER`, 7.
    Expander,
    /// `CS_IDX_LO1` (0) or `CS_IDX_LO2` (3): which LO.
    Synthesizer(usize),
    /// `CS_IDX_ATTN`, 2.
    Attenuator,
}

impl Select {
    /// The chip the configuration word `config` selects, if any.
    fn of(config: u32) -> Option<Self> {
        match (config >> 8) & 7 {
            7 => Some(Self::Expander),
            0 => Some(Self::Synthesizer(0)),
            3 => Some(Self::Synthesizer(1)),
            2 => Some(Self::Attenuator),
            _ => None,
        }
    }
}

/// fe1005000's chips at their power-on state.
#[derive(Debug)]
pub(crate) struct Fe1005000 {
    /// The expander's registers, by address (bits 14:8 of its frame).
    expander: [u8; 128],
    /// Each MAX2871's registers, by address; `None` until written.
    synthesizers: [[Option<u32>; 8]; 2],
    /// The attenuator's code, once written.
    attenuation: Option<u8>,
}

impl Fe1005000 {
    /// The board at power-on.
    pub(crate) fn new() -> Self {
        Self {
            expander: [0xff; 128],
            synthesizers: [[None; 8]; 2],
            attenuation: None,
        }
    }

    /// Shifts `word` through the chip `config` selects; the word read back.
    pub(crate) fn transact(&mut self, config: u32, word: u32) -> u32 {
        match Select::of(config) {
            Some(Select::Expander) => self.expander(word),
            Some(Select::Synthesizer(lo)) => {
                self.synthesizers[lo][(word & 7) as usize] = Some(word);
                u32::MAX
            }
            Some(Select::Attenuator) => {
                let [_, code, ..] = word.to_le_bytes();
                self.attenuation = Some(code.reverse_bits());
                u32::MAX
            }
            None => u32::MAX,
        }
    }

    /// One expander frame.
    fn expander(&mut self, word: u32) -> u32 {
        let [data, reg, ..] = word.to_le_bytes();
        let reg = reg & 0x7f;
        if word & READ == 0 {
            self.expander[usize::from(reg)] = data;
            return 0;
        }
        u32::from(self.expander_reg(reg))
    }

    /// An expander register as read: the status registers computed from the pins.
    pub(crate) fn expander_reg(&self, reg: u8) -> u8 {
        let Some(port) = STATUS.iter().position(|&status| status == reg) else {
            return self.expander[usize::from(reg & 0x7f)];
        };
        let inputs = self.expander[usize::from(DIRECTION[port])];
        self.expander[usize::from(OUTPUT[port])] & !inputs | inputs
    }

    /// Register `addr` of LO `lo` (0 the first, 1 the second), once written.
    pub(crate) fn synthesizer_reg(&self, lo: usize, addr: u8) -> Option<u32> {
        *self.synthesizers.get(lo)?.get(usize::from(addr))?
    }

    /// The attenuator's code, in 0.25 dB steps, once written.
    pub(crate) fn attenuation(&self) -> Option<u8> {
        self.attenuation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// libusdr's configuration word for the expander (`MAKE_SPI_CFG(1, 7, 25)`).
    const EXPANDER: u32 = 0x4719;

    #[test]
    fn the_expander_keeps_what_libusdrs_check_writes() {
        let mut board = Fe1005000::new();
        assert_eq!(board.transact(EXPANDER, 0x0c03), 0);
        assert_eq!(board.transact(EXPANDER, 0x0ec0), 0);
        assert_eq!(board.transact(EXPANDER, 0x8cff), 0x03);
        assert_eq!(board.transact(EXPANDER, 0x8eff), 0xc0);
    }

    #[test]
    fn status_reads_outputs_where_driven_and_high_where_inputs() {
        let mut board = Fe1005000::new();
        board.transact(EXPANDER, 0x0ec0);
        board.transact(EXPANDER, 0x0607);
        assert_eq!(board.transact(EXPANDER, 0x82ff), 0xc7);
    }

    #[test]
    fn the_los_and_attenuator_store_their_words_and_float() {
        let mut board = Fe1005000::new();
        assert_eq!(board.transact(0xc319, 0x0140_0005), u32::MAX);
        assert_eq!(board.synthesizer_reg(1, 5), Some(0x0140_0005));
        assert_eq!(board.synthesizer_reg(0, 5), None);
        assert_eq!(board.transact(0x4219, 0x8000), u32::MAX);
        assert_eq!(board.attenuation(), Some(1));
        assert_eq!(
            board.transact(0x4119, 0x1234),
            u32::MAX,
            "chip select 1 is empty"
        );
    }
}
