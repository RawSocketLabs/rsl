//! LMS6002D RF transceiver: 7-bit address, 8-bit data, over 16-bit SPI words.

/// Write flag in an SPI word.
const WRITE: u32 = 0x8000;

/// The LMS6002D register file.
#[derive(Debug)]
pub(crate) struct Lms6002d {
    /// Register contents, indexed by 7-bit address.
    pub(crate) regs: [u8; 128],
}

impl Lms6002d {
    /// A chip whose version register (0x04) reads 0x22, the `LMS6002Dr2` datasheet value.
    pub(crate) fn new() -> Self {
        let mut regs = [0; 128];
        regs[0x04] = 0x22;
        Self { regs }
    }

    /// Performs one SPI word: `[15] write, [14:8] address, [7:0] data`. Returns the read
    /// data in the low byte (zero for writes).
    pub(crate) fn transact(&mut self, word: u32) -> u32 {
        let [data, addr, ..] = word.to_le_bytes();
        let addr = usize::from(addr & 0x7f);
        if word & WRITE != 0 {
            self.regs[addr] = data;
            return 0;
        }
        u32::from(self.regs[addr])
    }
}
