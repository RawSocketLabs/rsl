//! uSDR USB wire fields, from libusdr at `3670f15de8e49691db455da03166c17f6dd5770a`.
//!
//! Authority: `lowlevel/usb_uram/usb_uram_libusb.c` (`usb_async_regwrite32`,
//! `usb_async_regread32`, `libusb_transfer_ntfy`) and `ipblks/si2c.{c,h}`.
//! C uses native `uint32_t` storage on its little-endian reference ABI; we encode explicitly
//! little-endian. Readback is raw words, not a length-prefixed response.

use bnb::{bitfield, u2, u3, u4, u6, u24};

/// The register endpoint's command: 1–64 words starting at a 16-bit register address.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Command {
    /// First physical register.
    #[bits(0..=15)]
    address: u16,
    /// Number of words minus one, `usb_async_reg{read,write}32`.
    #[bits(16..=21)]
    count_minus_one: u6,
    /// Zero for write, three for read.
    #[bits(30..=31)]
    operation: u2,
}

impl Command {
    /// Encodes a request already checked to contain 1–64 words.
    pub(crate) fn encode(address: u16, count: u8, read: bool) -> [u8; 4] {
        Self::new()
            .with_address(address)
            .with_count_minus_one(u6::new(count - 1))
            .with_operation(u2::new(if read { 3 } else { 0 }))
            .to_raw()
            .to_le_bytes()
    }
}

/// Notification record header, followed by `extra_words + 1` payload words.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Notification {
    /// IRQ number; scalar SPI and I²C replies use 2 and 6 respectively.
    #[bits(0..=5)]
    pub(crate) event: u6,
    /// Additional words after the first payload word; zero for scalar completions.
    #[bits(12..=15)]
    pub(crate) extra_words: u4,
    /// Global wrapping notification sequence.
    #[bits(16..=31)]
    pub(crate) sequence: u16,
}

impl Notification {
    /// Validates a complete IRQ transfer and advances the session's global sequence.
    /// Scalar replies for `event` are returned only after the entire packet is valid.
    pub(crate) fn completion(
        packet: &[u8],
        event: u8,
        next_sequence: &mut Option<u16>,
    ) -> Result<Option<u32>, &'static str> {
        if packet.is_empty() || packet.len() > 64 || packet.len() % 4 != 0 {
            return Err("invalid USB notification transfer length");
        }
        let mut next = *next_sequence;
        let mut rest = packet;
        let mut value = None;
        while !rest.is_empty() {
            if rest.len() < 8 {
                return Err("truncated USB notification");
            }
            let header = Self::from_raw(u32::from_le_bytes(
                rest[..4]
                    .try_into()
                    .expect("invariant: notification header checked"),
            ));
            let sequence = header.sequence();
            if next.is_some_and(|expected| sequence != expected) {
                return Err("USB notification sequence gap");
            }
            next = Some(sequence.wrapping_add(1));
            let length = (usize::from(header.extra_words().value()) + 2) * 4;
            if length > rest.len() || header.event().value() >= 32 {
                return Err("invalid USB notification record");
            }
            if header.event().value() == event {
                if header.extra_words().value() != 0 || value.is_some() {
                    return Err("ambiguous USB control completion");
                }
                value = Some(u32::from_le_bytes(
                    rest[4..8]
                        .try_into()
                        .expect("invariant: scalar payload checked"),
                ));
            }
            rest = &rest[length..];
        }
        *next_sequence = next;
        Ok(value)
    }
}

/// The simple I²C core's control word, `MAKE_I2C_CMD` in `ipblks/si2c.h`.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug)]
struct I2cCommand {
    /// Up to three outgoing bytes, first byte in bits 7:0.
    #[bits(0..=23)]
    data: u24,
    /// Address lookup-table slot.
    #[bits(24..=25)]
    slot: u2,
    /// Number of outgoing bytes.
    #[bits(26..=27)]
    write_count: u2,
    /// Incoming length minus one; seven for a write-only operation, as in C.
    #[bits(28..=30)]
    read_count_minus_one: u3,
    /// Whether incoming bytes are requested.
    #[bits(31..=31)]
    read: bool,
}

/// One address-cache entry; zero initialization matches `usb_uram_plugin_create`.
#[derive(Clone, Copy, Debug, Default)]
struct Entry {
    /// I²C address with the bus number in bit 7.
    address: u8,
    /// The C core's 8-bit LRU rank.
    rank: u8,
}

/// Four slots used by the FPGA's simple I²C core.
#[derive(Debug, Default)]
pub(crate) struct I2cCache([Entry; 4]);

impl I2cCache {
    /// Selects a slot and encodes LUT/control words for an already bounded I²C request.
    pub(crate) fn command(&mut self, address: u8, bus: u8, write: &[u8], read: u8) -> [u32; 2] {
        let encoded = address | bus << 7;
        // Bit 31 is the LUT-update flag, so bus 0 cannot use the fourth slot.
        let count = if bus == 0 { 3 } else { 4 };
        let entries = &mut self.0[..count];
        let slot = if let Some(index) = entries.iter().position(|entry| entry.address == encoded) {
            let rank = entries[index].rank;
            if let Some(next) = entries
                .iter()
                .position(|entry| u16::from(entry.rank) == u16::from(rank) + 1)
            {
                entries[next].rank = rank;
                entries[index].rank = rank.wrapping_add(1);
            }
            index
        } else {
            let index = entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.rank)
                .map_or(0, |(index, _)| index);
            if entries[index].rank == 0 {
                for entry in entries.iter_mut() {
                    if entry.rank > 0 {
                        entry.rank = entry.rank.wrapping_add(1);
                    }
                }
                entries[index].rank = 1;
            }
            entries[index].address = encoded;
            index
        };
        let mut bytes = [0; 4];
        bytes[..write.len()].copy_from_slice(write);
        let lut = 0x8000_0000 | u32::from_le_bytes(self.0.map(|entry| entry.address));
        // Slot and lengths are bounded by the caller and the four-slot table.
        let control = I2cCommand::new()
            .with_slot(u2::new(u8::try_from(slot).expect("four I2C slots")))
            .with_data(u24::new(u32::from_le_bytes(bytes)))
            .with_write_count(u2::new(u8::try_from(write.len()).expect("three I2C bytes")))
            .with_read_count_minus_one(u3::new(read.wrapping_sub(1) & 7))
            .with_read(read > 0)
            .to_raw();
        [lut, control]
    }
}
