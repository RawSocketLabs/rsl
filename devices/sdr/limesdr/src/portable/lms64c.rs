//! LMS64C, the 64-byte control frame the `LimeSDR`'s firmware answers: the requests a driver
//! sends and the replies it reads.
//!
//! **Unstable.** Layout and encodings are libusdr's (`ipblks/lms64c_proto.{c,h}`,
//! `lms64c_cmds.h`): `cmd`, `status`, `blockCount`, `periphID`, four reserved bytes, then
//! [`DATA_LEN`] data bytes. Register writes carry 32-bit big-endian records, the address in
//! the upper half and the value in the lower, one per block; register reads carry 16-bit
//! big-endian addresses, one per block, and are answered with one address-then-value record
//! per address, whose value half is the register (`MOD_INT32_16`). Only what the
//! `LimeSDR`'s open uses is built: single-frame register writes and reads, and the LMS7002M
//! reset pulse. `rsl-limesdr-oracle` compares each with libusdr's `lms64c_fill_packet` and
//! `lms64c_parse_packet`.

/// Length of every frame, either direction (`LMS64C_PKT_LENGTH`).
pub const FRAME_LEN: usize = 64;

/// Length of a frame's payload (`LMS64C_DATA_LENGTH`).
pub const DATA_LEN: usize = 56;

/// Where the payload starts: after `cmd`, `status`, `blockCount`, `periphID` and four
/// reserved bytes.
const HEADER_LEN: usize = FRAME_LEN - DATA_LEN;

/// Most records one register frame carries: the payload's fourteen 4-byte write records, or
/// the fourteen addresses whose 4-byte reply records fill a reply (`lms64c_fill_packet`
/// caps a read's addresses at half the payload).
pub const MAX_RECORDS: usize = DATA_LEN / 4;

/// The reply status of a command the firmware carried out (`STATUS_COMPLETED_CMD`).
pub const STATUS_COMPLETED: u8 = 1;

/// `CMD_LMS7002_RST`: sets the LMS7002M's reset pin; one data byte, the level.
const CMD_LMS7002_RST: u8 = 0x20;
/// `CMD_LMS7002_WR`: writes LMS7002M registers.
const CMD_LMS7002_WR: u8 = 0x21;
/// `CMD_LMS7002_RD`: reads LMS7002M registers.
const CMD_LMS7002_RD: u8 = 0x22;
/// `CMD_BRDSPI_WR`: writes gateware registers.
const CMD_BRDSPI_WR: u8 = 0x55;
/// `CMD_BRDSPI_RD`: reads gateware registers.
const CMD_BRDSPI_RD: u8 = 0x56;

/// `LMS_RST_PULSE`, the reset-pin level that pulses the LMS7002M's reset.
const RST_PULSE: u8 = 2;

/// A register file the register commands address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// The gateware's 16-bit registers (`CMD_BRDSPI_WR`, `CMD_BRDSPI_RD`).
    Board,

    /// The LMS7002M's registers (`CMD_LMS7002_WR`, `CMD_LMS7002_RD`).
    Lms7,
}

impl Space {
    /// The command that writes this space's registers.
    const fn write_command(self) -> u8 {
        match self {
            Self::Board => CMD_BRDSPI_WR,
            Self::Lms7 => CMD_LMS7002_WR,
        }
    }

    /// The command that reads this space's registers.
    const fn read_command(self) -> u8 {
        match self {
            Self::Board => CMD_BRDSPI_RD,
            Self::Lms7 => CMD_LMS7002_RD,
        }
    }
}

/// One LMS64C frame, request or reply, as it travels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame([u8; FRAME_LEN]);

impl Frame {
    /// A request writing `records` to `space`, each sent unchanged as `address << 16 |
    /// value` (an LMS7002M record is its SPI word, write flag included). `None` for no
    /// records or more than [`MAX_RECORDS`]: libusdr's builder makes no frame for no data
    /// and fails data that needs a second frame.
    #[must_use]
    pub fn write(space: Space, records: &[u32]) -> Option<Self> {
        let mut frame = Self::request(space.write_command(), records.len())?;
        for (slot, record) in frame.data_mut().chunks_exact_mut(4).zip(records) {
            slot.copy_from_slice(&record.to_be_bytes());
        }
        Some(frame)
    }

    /// A request reading the registers at `addrs` in `space`. `None` for no addresses or
    /// more than [`MAX_RECORDS`], as [`Frame::write`].
    #[must_use]
    pub fn read(space: Space, addrs: &[u16]) -> Option<Self> {
        let mut frame = Self::request(space.read_command(), addrs.len())?;
        for (slot, addr) in frame.data_mut().chunks_exact_mut(2).zip(addrs) {
            slot.copy_from_slice(&addr.to_be_bytes());
        }
        Some(frame)
    }

    /// A request pulsing the LMS7002M's reset pin (`CMD_LMS7002_RST`, `LMS_RST_PULSE`).
    #[must_use]
    pub const fn lms7_reset_pulse() -> Self {
        let mut bytes = [0; FRAME_LEN];
        bytes[0] = CMD_LMS7002_RST;
        bytes[2] = 1;
        bytes[HEADER_LEN] = RST_PULSE;
        Self(bytes)
    }

    /// A request frame for `cmd` with `blocks` blocks and an empty payload, peripheral 0;
    /// `None` for no blocks or more than [`MAX_RECORDS`].
    fn request(cmd: u8, blocks: usize) -> Option<Self> {
        if blocks == 0 {
            return None;
        }
        let blocks = u8::try_from(blocks)
            .ok()
            .filter(|&blocks| usize::from(blocks) <= MAX_RECORDS)?;
        let mut bytes = [0; FRAME_LEN];
        bytes[..4].copy_from_slice(&[cmd, 0, blocks, 0]);
        Some(Self(bytes))
    }

    /// The frame's payload.
    fn data(&self) -> &[u8] {
        &self.0[HEADER_LEN..]
    }

    /// The frame's payload, to fill.
    fn data_mut(&mut self) -> &mut [u8] {
        &mut self.0[HEADER_LEN..]
    }

    /// A frame received.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; FRAME_LEN]) -> Self {
        Self(bytes)
    }

    /// The frame as sent.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; FRAME_LEN] {
        self.0
    }

    /// The command (`cmd`); a reply echoes its request's.
    #[must_use]
    pub const fn command(&self) -> u8 {
        self.0[0]
    }

    /// The outcome a reply reports (`status`; [`STATUS_COMPLETED`] for success), 0 in a
    /// request.
    #[must_use]
    pub const fn status(&self) -> u8 {
        self.0[1]
    }

    /// How many records the payload carries (`blockCount`).
    #[must_use]
    pub const fn block_count(&self) -> u8 {
        self.0[2]
    }

    /// The register value a read's reply holds for its `index`th address: the value half
    /// of reply record `index`. `None` past [`MAX_RECORDS`]; the payload holds no more
    /// (libusdr's parser reads past it).
    #[must_use]
    pub fn read_value(&self, index: usize) -> Option<u16> {
        let record = self.data().chunks_exact(4).nth(index)?;
        Some(u16::from_be_bytes([record[2], record[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_record_goes_out_big_endian_after_the_header() {
        let frame = Frame::write(Space::Board, &[0x0061_0004]).expect("one record fits");
        let bytes = frame.to_bytes();
        assert_eq!(
            bytes[..12],
            [0x55, 0, 1, 0, 0, 0, 0, 0, 0x00, 0x61, 0x00, 0x04]
        );
        assert!(
            bytes[12..].iter().all(|&byte| byte == 0),
            "the rest is zero"
        );
    }

    #[test]
    fn a_read_carries_one_big_endian_address_per_block() {
        let frame = Frame::read(Space::Lms7, &[0x002f, 0x0123]).expect("two addresses fit");
        assert_eq!((frame.command(), frame.block_count()), (0x22, 2));
        assert_eq!(frame.data()[..4], [0x00, 0x2f, 0x01, 0x23]);
    }

    #[test]
    fn frames_hold_one_to_fourteen_records() {
        assert_eq!(Frame::write(Space::Lms7, &[]), None);
        assert_eq!(Frame::read(Space::Board, &[]), None);
        assert!(Frame::write(Space::Lms7, &[0; MAX_RECORDS]).is_some());
        assert!(Frame::read(Space::Board, &[0; MAX_RECORDS]).is_some());
        assert_eq!(Frame::write(Space::Lms7, &[0; MAX_RECORDS + 1]), None);
        assert_eq!(Frame::read(Space::Board, &[0; MAX_RECORDS + 1]), None);
        assert_eq!(Frame::read(Space::Board, &[0; 256]), None, "no u8 wrap");
    }

    #[test]
    fn a_reply_value_is_the_low_half_of_its_record() {
        let mut bytes = [0; FRAME_LEN];
        bytes[HEADER_LEN..HEADER_LEN + 8].copy_from_slice(&[0, 0x72, 0x66, 0x64, 0, 0x73, 0, 0x66]);
        let reply = Frame::from_bytes(bytes);
        assert_eq!(reply.read_value(0), Some(0x6664));
        assert_eq!(reply.read_value(1), Some(0x0066));
        assert_eq!(reply.read_value(MAX_RECORDS - 1), Some(0));
        assert_eq!(reply.read_value(MAX_RECORDS), None);
        assert_eq!(reply.read_value(usize::MAX), None, "no overflow");
    }

    #[test]
    fn the_reset_pulse_is_one_byte_at_level_two() {
        let frame = Frame::lms7_reset_pulse();
        assert_eq!(frame.to_bytes()[..9], [0x20, 0, 1, 0, 0, 0, 0, 0, 2]);
    }
}
