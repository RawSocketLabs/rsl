//! TCA6416/TCA9555-style 16-bit I2C GPIO expander: eight byte registers in four port
//! pairs.

use super::I2cChip;

/// Input port 0 (`IN_P0`); input port 1 follows.
const INPUT: usize = 0;
/// Output port 0 (`OUT_P0`).
const OUTPUT: usize = 2;
/// Polarity inversion port 0 (`POL_INV_P0`).
const POLARITY: usize = 4;
/// Configuration port 0 (`CONFIG_P0`); a 0 bit makes the pin an output.
const CONFIG: usize = 6;

/// A TCA6416 (the `PCIe` front end's) or TCA9555-compatible (XMASS's) expander whose
/// input-configured pins are pulled high. libusdr gives both the same register map
/// (`enum tca6416_regs`, `enum tca9555_regs`).
///
/// Modelling assumptions, from the TCA6416A and TCA9555 datasheets' register descriptions
/// (TI, not vendored here), as libusdr's `ext_pciefe.c` and `tca9555.c` rely on them:
/// - The command byte selects one of eight registers; each further byte read or written
///   moves to the other register of the same pair, so a two-byte write sets both ports
///   (`tca6416_reg_wr2`) and a four-byte read returns port 0, port 1, port 0, port 1.
/// - Reset: outputs 0xFF, polarity 0x00, configuration 0xFF (all inputs).
/// - An input register reads the pin levels, inverted where the polarity bit is set. An
///   output pin reads back its latch; an input pin reads 1, as if pulled up, since nothing
///   drives the board's pins in the sim.
/// - Writes to the input registers are ignored.
#[derive(Debug)]
pub(crate) struct Expander {
    /// Register contents, by command byte; the input registers are computed on read.
    regs: [u8; 8],
}

impl Expander {
    /// An expander at its reset state.
    pub(crate) fn new() -> Self {
        let mut regs = [0; 8];
        regs[OUTPUT] = 0xff;
        regs[OUTPUT + 1] = 0xff;
        regs[CONFIG] = 0xff;
        regs[CONFIG + 1] = 0xff;
        Self { regs }
    }

    /// The pin levels of `port` as its input register reports them.
    fn input(&self, port: usize) -> u8 {
        let config = self.regs[CONFIG + port];
        let pins = (self.regs[OUTPUT + port] & !config) | config;
        pins ^ self.regs[POLARITY + port]
    }

    /// A register's contents as read.
    fn read(&self, reg: usize) -> u8 {
        if reg < OUTPUT {
            self.input(reg - INPUT)
        } else {
            self.regs[reg]
        }
    }
}

impl I2cChip for Expander {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let Some((&command, data)) = write.split_first() else {
            return vec![0xff; read_len];
        };
        let mut reg = usize::from(command & 0x07);
        for &byte in data {
            if reg >= OUTPUT {
                self.regs[reg] = byte;
            }
            reg ^= 1;
        }
        (0..read_len)
            .map(|_| {
                let byte = self.read(reg);
                reg ^= 1;
                byte
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_write_sets_both_ports_and_reads_alternate() {
        let mut expander = Expander::new();
        expander.transfer(&[0x06, 0x00, 0x00], 0);
        expander.transfer(&[0x02, 0x12, 0x34], 0);
        assert_eq!(expander.transfer(&[0x00], 4), vec![0x12, 0x34, 0x12, 0x34]);
    }

    #[test]
    fn input_pins_read_high_until_made_outputs() {
        let mut expander = Expander::new();
        expander.transfer(&[0x02, 0x00], 0);
        assert_eq!(expander.transfer(&[0x00], 1), vec![0xff]);
        expander.transfer(&[0x06, 0x0f], 0);
        assert_eq!(expander.transfer(&[0x00], 1), vec![0x0f]);
    }
}
