//! TMP114 temperature sensor: 8-bit pointer, 16-bit registers sent MSB first.

use super::I2cChip;

/// Temperature result register.
const TEMP_RESULT: u8 = 0x00;
/// Device ID register; reads 0x1114.
const DEVICE_ID: u8 = 0x0b;

/// A TMP114 reporting a fixed temperature.
#[derive(Debug)]
pub(crate) struct Tmp114 {
    /// Register contents, indexed by pointer.
    regs: [u16; 16],
}

impl Tmp114 {
    /// A sensor reading `millicelsius`, with datasheet reset values elsewhere.
    pub(crate) fn new(millicelsius: i32) -> Self {
        let mut regs = [0; 16];
        regs[0x03] = 0x0004;
        regs[0x04] = 0xf380;
        regs[0x05] = 0x2a80;
        regs[0x06] = 0x0a0a;
        regs[0x07] = 0x0500;
        regs[usize::from(DEVICE_ID)] = 0x1114;
        regs[usize::from(TEMP_RESULT)] = Self::encode(millicelsius);
        Self { regs }
    }

    /// Encodes a temperature as the result register's two's-complement 1/128 °C value.
    fn encode(millicelsius: i32) -> u16 {
        let lsb = (millicelsius * 128 / 1000).clamp(i16::MIN.into(), i16::MAX.into());
        let lsb = i16::try_from(lsb).expect("invariant: clamped to the i16 range");
        u16::from_ne_bytes(lsb.to_ne_bytes())
    }
}

impl I2cChip for Tmp114 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let Some((&pointer, data)) = write.split_first() else {
            return vec![0xff; read_len];
        };
        let index = usize::from(pointer & 0x0f);
        if let [hi, lo] = *data {
            self.regs[index] = u16::from_be_bytes([hi, lo]);
        }
        self.regs[index]
            .to_be_bytes()
            .into_iter()
            .cycle()
            .take(read_len)
            .collect()
    }
}
