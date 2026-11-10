//! The `NESDR SMArt v5`'s open and close, as its captures record them; no other board's.
//!
//! Each table is transcribed by script from a capture trace (`tests/fixtures/`), one
//! [`Step`] per access, a paged write's read-back folded into it; the replay tests hold the
//! tables to the traces. Which steps do what is not evidenced, only that librtlsdr sends
//! them, in this order, in every session (`RTL-OBS-09`, `RTL-OBS-10`).

use super::r820t2::registers::I2C_ADDRESS;
use super::rtl2832u::{Block, PAGE_0, PAGE_1, Page};

/// The tuner's 7-bit I2C address. prov: R820-DS-01
const TUNER: u8 = I2C_ADDRESS;

/// One access of a sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// A paged-block write, with its read-back (RTL-OBS-03, RTL-OBS-04).
    Paged {
        /// The page.
        page: Page,
        /// The first register.
        register: u8,
        /// The bytes.
        data: &'static [u8],
    },

    /// An addressed-block write (RTL-OBS-05).
    Block {
        /// The block.
        block: Block,
        /// The address.
        address: u16,
        /// The bytes.
        data: &'static [u8],
    },

    /// The tuner gate, opened or closed, with its read-back (RTL-OBS-07).
    Gate {
        /// Whether it opens.
        open: bool,
    },

    /// A one-byte I2C write and a one-byte read at an address where the V5 has nothing:
    /// both stall, and the open goes on; an answer means another board (`RTL-GAP-07`). The
    /// byte is the pcap's (a trace does not record a stalled transfer's data).
    Probe {
        /// The 7-bit address.
        address: u8,
        /// The byte written.
        byte: u8,
    },

    /// Reads R0 and requires the R820T2's chip id (R820-DS-05): it guards only against a
    /// tuner that answers at the address with another R0 (`RTL-GAP-07`).
    Identify {
        /// The tuner's 7-bit address.
        address: u8,
    },

    /// An I2C write.
    I2cWrite {
        /// The 7-bit address.
        address: u8,
        /// The bytes: a register, then its values.
        data: &'static [u8],
    },

    /// An I2C read whose answer the captures show no use of.
    I2cRead {
        /// The 7-bit address.
        address: u8,
        /// How many bytes.
        len: usize,
    },
}

/// The open: GPLOBS-V5-0001 transfers #1 to #142, the same in all four V5 captures.
/// prov: RTL-OBS-09
pub const OPEN: &[Step] = &[
    Step::Block {
        block: Block::One,
        address: 0x2000,
        data: &[0x09],
    },
    Step::Block {
        block: Block::One,
        address: 0x2000,
        data: &[0x09],
    },
    Step::Block {
        block: Block::One,
        address: 0x2158,
        data: &[0x00, 0x02],
    },
    Step::Block {
        block: Block::One,
        address: 0x2148,
        data: &[0x10, 0x02],
    },
    Step::Block {
        block: Block::Two,
        address: 0x300b,
        data: &[0x22],
    },
    Step::Block {
        block: Block::Two,
        address: 0x3000,
        data: &[0xe8],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x01,
        data: &[0x14],
    },
    Step::Gate { open: false },
    Step::Paged {
        page: PAGE_1,
        register: 0x15,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x16,
        data: &[0x00, 0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x16,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x17,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x18,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x19,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x1a,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x1b,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x1c,
        data: &[0xca],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x1d,
        data: &[0xdc],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x1e,
        data: &[0xd7],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x1f,
        data: &[0xd8],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x20,
        data: &[0xe0],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x21,
        data: &[0xf2],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x22,
        data: &[0x0e],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x23,
        data: &[0x35],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x24,
        data: &[0x06],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x25,
        data: &[0x50],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x26,
        data: &[0x9c],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x27,
        data: &[0x0d],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x28,
        data: &[0x71],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x29,
        data: &[0x11],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x2a,
        data: &[0x14],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x2b,
        data: &[0x71],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x2c,
        data: &[0x74],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x2d,
        data: &[0x19],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x2e,
        data: &[0x41],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x2f,
        data: &[0xa5],
    },
    Step::Paged {
        page: PAGE_0,
        register: 0x19,
        data: &[0x05],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x93,
        data: &[0xf0],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x94,
        data: &[0x0f],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x11,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x04,
        data: &[0x00],
    },
    Step::Paged {
        page: PAGE_0,
        register: 0x61,
        data: &[0x60],
    },
    Step::Paged {
        page: PAGE_0,
        register: 0x06,
        data: &[0x80],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0xb1,
        data: &[0x1b],
    },
    Step::Paged {
        page: PAGE_0,
        register: 0x0d,
        data: &[0x83],
    },
    Step::Gate { open: true },
    Step::Probe {
        address: 0x64,
        byte: 0x02,
    },
    Step::Probe {
        address: 0x63,
        byte: 0x00,
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x00],
    },
    Step::Identify { address: TUNER },
    Step::Paged {
        page: PAGE_1,
        register: 0xb1,
        data: &[0x1a],
    },
    Step::Paged {
        page: PAGE_0,
        register: 0x08,
        data: &[0x4d],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x19,
        data: &[0x38],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x1a,
        data: &[0x11],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x1b,
        data: &[0x12],
    },
    Step::Paged {
        page: PAGE_1,
        register: 0x15,
        data: &[0x01],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x05, 0x83, 0x32, 0x75, 0xc0, 0x40, 0xd6, 0x6c],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0c, 0xf5, 0x63, 0x75, 0x68, 0x6c, 0x83, 0x80],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x13, 0x00, 0x0f, 0x00, 0xc0, 0x30, 0x48, 0xcc],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1a, 0x60, 0x00, 0x54, 0xae, 0x4a, 0xc0, 0x00],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x21, 0x00, 0x00],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0c, 0xf0],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x13, 0x31],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1d, 0x86],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0f, 0x6c],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x00],
    },
    Step::I2cRead {
        address: TUNER,
        len: 5,
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x10, 0x8c, 0x83, 0x80, 0x31, 0x84, 0x72, 0x1c],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x00],
    },
    Step::I2cRead {
        address: TUNER,
        len: 3,
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1a, 0x68],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0b, 0x7c],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0b, 0x6c],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0f, 0x68],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x00],
    },
    Step::I2cRead {
        address: TUNER,
        len: 5,
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0b, 0x6b],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x06, 0x12],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1e, 0x6a],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x05, 0x03],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1f, 0x40],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x19, 0xec],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1d, 0xc5],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1c, 0x24],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0d, 0x53],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x11, 0xbb],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1c, 0x20],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1a, 0x78],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1d, 0xdd],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1c, 0x24],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1e, 0x6e],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x1a, 0x68],
    },
    Step::Gate { open: false },
];

/// The close of the tools that do not stream: GPLOBS-V5-0001 transfers #184 to #198, as in
/// GPLOBS-V5-0004. prov: RTL-OBS-10
pub const CLOSE: &[Step] = &[
    Step::Gate { open: true },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x06, 0xb1],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x05, 0xa0],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x07, 0x3a],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x08, 0x40],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x09, 0xc0],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0a, 0x36],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x0c, 0x35],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x11, 0x03],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x17, 0xf4],
    },
    Step::I2cWrite {
        address: TUNER,
        data: &[0x19, 0x0c],
    },
    Step::Gate { open: false },
    Step::Block {
        block: Block::Two,
        address: 0x3000,
        data: &[0x20],
    },
];
