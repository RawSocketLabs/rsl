//! LMS64C, the board firmware's 64-byte control frame.
//!
//! Layout, commands and statuses are libusdr's (`ipblks/lms64c_proto.h`, `lms64c_cmds.h`):
//! `cmd`, `status`, `blockCount`, `periphID`, four reserved bytes, then 56 data bytes. The
//! per-command payload shapes come from the same source's `lms64c_fill_packet` and
//! `lms64c_parse_packet` tables: register writes carry 32-bit big-endian records (address
//! in the upper half, value in the lower), register reads carry 16-bit big-endian addresses
//! and are answered with 32-bit address-then-value records, of which the parser keeps the
//! value half (`MOD_INT32_16`).

use std::fmt;

/// Length of every frame, either direction (`LMS64C_PKT_LENGTH`).
pub const FRAME_LEN: usize = 64;
/// Length of a frame's payload (`LMS64C_DATA_LENGTH`).
pub const DATA_LEN: usize = 56;

/// `CMD_LMS7002_RST`: sets the LMS7002M's reset pin; one data byte, `LMS_RST_*`.
pub(crate) const CMD_LMS7002_RST: u8 = 0x20;
/// `CMD_LMS7002_WR`: writes LMS7002M registers, one 32-bit record per block.
pub(crate) const CMD_LMS7002_WR: u8 = 0x21;
/// `CMD_LMS7002_RD`: reads LMS7002M registers, one 16-bit address per block.
pub(crate) const CMD_LMS7002_RD: u8 = 0x22;
/// `CMD_BRDSPI_WR`: writes gateware registers, one 32-bit record per block.
pub(crate) const CMD_BRDSPI_WR: u8 = 0x55;
/// `CMD_BRDSPI_RD`: reads gateware registers, one 16-bit address per block.
pub(crate) const CMD_BRDSPI_RD: u8 = 0x56;

/// `LMS_RST_PULSE`, the reset-pin level that pulses the chip's reset.
pub(crate) const RST_PULSE: u8 = 2;

/// A reply's `status` byte (`enum eCMD_STATUS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    /// `STATUS_COMPLETED_CMD`.
    Completed = 1,
    /// `STATUS_UNKNOWN_CMD`: a command the sim does not model.
    Unknown = 2,
    /// `STATUS_MANY_BLOCKS_CMD`: more blocks than the payload or its reply can hold.
    ManyBlocks = 4,
    /// `STATUS_ERROR_CMD`: a modelled command with an argument the board cannot honour.
    Error = 5,
}

/// One LMS64C frame, request or reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The command (`CMD_*`); a reply echoes its request's.
    pub cmd: u8,
    /// 0 in a request; the outcome (`STATUS_*`) in a reply.
    pub status: u8,
    /// How many records the payload carries (`blockCount`).
    pub block_count: u8,
    /// The peripheral addressed, such as the LMS7002M's index (`periphID`).
    pub periph: u8,
    /// Reserved; libusdr zeroes them.
    pub reserved: [u8; 4],
    /// The payload.
    pub data: [u8; DATA_LEN],
}

impl Frame {
    /// Splits a frame off the wire.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; FRAME_LEN]) -> Self {
        let [cmd, status, block_count, periph, r0, r1, r2, r3, data @ ..] = *bytes;
        Self {
            cmd,
            status,
            block_count,
            periph,
            reserved: [r0, r1, r2, r3],
            data,
        }
    }

    /// The frame as sent on the wire.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; FRAME_LEN] {
        let mut bytes = [0; FRAME_LEN];
        let (header, data) = bytes.split_at_mut(FRAME_LEN - DATA_LEN);
        header[..4].copy_from_slice(&[self.cmd, self.status, self.block_count, self.periph]);
        header[4..].copy_from_slice(&self.reserved);
        data.copy_from_slice(&self.data);
        bytes
    }

    /// The reply to this request, with an empty payload and `status`.
    pub(crate) fn reply(&self, status: Status) -> Self {
        Self {
            status: status as u8,
            data: [0; DATA_LEN],
            ..self.clone()
        }
    }

    /// The payload's `block_count` records of `size` bytes each, or `None` when they would
    /// not fit, or a register read's 4-byte-per-record reply would not.
    pub(crate) fn records(&self, size: usize) -> Option<impl Iterator<Item = &[u8]>> {
        let count = usize::from(self.block_count);
        (count * size.max(4) <= DATA_LEN).then(|| self.data.chunks_exact(size).take(count))
    }
}

/// `cmd=0xCC st=S blk=B id=P d=HEX`, with `rsv=HEX` after `id` only when a reserved byte
/// is set. `d` holds the payload through its last non-zero 4-byte group (`-` if all zero),
/// so the line is lossless: the rest is zero.
impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cmd={:#04x} st={} blk={} id={}",
            self.cmd, self.status, self.block_count, self.periph
        )?;
        if self.reserved != [0; 4] {
            write!(f, " rsv={}", Hex(&self.reserved))?;
        }
        let used = self
            .data
            .iter()
            .rposition(|&byte| byte != 0)
            .map_or(0, |last| (last / 4 + 1) * 4);
        write!(f, " d={}", Hex(&self.data[..used]))
    }
}

/// Unseparated lowercase hex pairs, or `-` when there are none.
struct Hex<'a>(&'a [u8]);

impl fmt::Display for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("-");
        }
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request carrying `data` from the payload's start.
    fn request(cmd: u8, block_count: u8, data: &[u8]) -> Frame {
        let mut frame = Frame::from_bytes(&[0; FRAME_LEN]);
        frame.cmd = cmd;
        frame.block_count = block_count;
        frame.data[..data.len()].copy_from_slice(data);
        frame
    }

    #[test]
    fn a_frame_survives_the_wire_unchanged() {
        let mut bytes = [0; FRAME_LEN];
        for (byte, value) in bytes.iter_mut().zip(1..) {
            *byte = value;
        }
        assert_eq!(Frame::from_bytes(&bytes).to_bytes(), bytes);
    }

    #[test]
    fn display_trims_only_trailing_zero_groups() {
        let frame = request(
            CMD_BRDSPI_WR,
            1,
            &[0x00, 0x61, 0x00, 0x00, 0, 0, 0, 0, 0, 1],
        );
        assert_eq!(
            frame.to_string(),
            "cmd=0x55 st=0 blk=1 id=0 d=006100000000000000010000"
        );
        assert_eq!(
            request(CMD_BRDSPI_WR, 1, &[]).to_string(),
            "cmd=0x55 st=0 blk=1 id=0 d=-"
        );
    }

    #[test]
    fn display_shows_reserved_bytes_when_set() {
        let mut frame = request(CMD_LMS7002_RST, 1, &[RST_PULSE]);
        frame.reserved[3] = 9;
        assert_eq!(
            frame.to_string(),
            "cmd=0x20 st=0 blk=1 id=0 rsv=00000009 d=02000000"
        );
    }

    #[test]
    fn records_refuse_counts_whose_reply_would_overflow() {
        assert_eq!(
            request(CMD_BRDSPI_RD, 14, &[])
                .records(2)
                .map(Iterator::count),
            Some(14)
        );
        assert!(
            request(CMD_BRDSPI_RD, 15, &[]).records(2).is_none(),
            "15 reads fit the request but not the 4-byte-per-record reply"
        );
        assert!(request(CMD_BRDSPI_WR, 15, &[]).records(4).is_none());
    }
}
