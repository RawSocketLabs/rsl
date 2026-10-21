//! The TMP108 temperature sensor at I2C bus 1, 0x4b (`I2C_BUS_TMP108`,
//! `hw/tmp108/tmp108.c`).
//!
//! libusdr writes the pointer byte 0 and reads two bytes, which the transport packs so its
//! `int16_t` is the first byte read times 256 plus the second: the sensor's word, most
//! significant byte first. No TMP108 datasheet or register description is vendored, so the
//! model is that one read: the temperature register (pointer 0) holds a fixed word, in
//! 1/256 °C as libusdr's callers read it, 25 °C (0x1900) unless set
//! ([`SimBoard::with_temperature`](crate::SimBoard::with_temperature)); every other
//! pointer reads 0 and writes are ignored (sim choices).

use std::iter;

use super::I2cChip;

/// The temperature register's pointer.
const TEMPERATURE: u8 = 0;

/// The word the sensor reads by default: 25 °C in 1/256 °C.
pub(crate) const ROOM: i16 = 25 * 256;

/// One TMP108.
#[derive(Debug)]
pub(crate) struct Tmp108 {
    /// The temperature register.
    word: i16,
}

impl Tmp108 {
    /// A sensor reading `word`.
    pub(crate) fn new(word: i16) -> Self {
        Self { word }
    }
}

impl I2cChip for Tmp108 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let register = match write.first() {
            Some(&TEMPERATURE) => self.word.to_be_bytes(),
            Some(_) => [0; 2],
            None => return vec![0xff; read_len],
        };
        // Past the register's two bytes the sim reads 0 (libusdr reads exactly two).
        register
            .into_iter()
            .chain(iter::repeat(0))
            .take(read_len)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temperature_reads_most_significant_byte_first() {
        let mut sensor = Tmp108::new(-0x0180);
        assert_eq!(sensor.transfer(&[0], 2), vec![0xfe, 0x80]);
        assert_eq!(Tmp108::new(ROOM).transfer(&[0], 2), vec![0x19, 0x00]);
        assert_eq!(sensor.transfer(&[1], 2), vec![0, 0]);
    }
}
