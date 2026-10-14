//! limesdr's LMS64C codec against libusdr's own builder and parser
//! (`ipblks/lms64c_proto.c`), byte for byte: every record count a frame holds, one past it,
//! and none, for both register files, plus the reset pulse and the reply values.
//!
//! The command codes are `lms64c_cmds.h`'s, restated here so the codec's own constants are
//! not the oracle for themselves.

use limesdr::lms64c::{FRAME_LEN, Frame, MAX_RECORDS, Space};
use limesdr_oracle::{READ_VALUES_MAX, lms64c_fill, lms64c_read_values};

/// `CMD_LMS7002_RST`.
const CMD_LMS7002_RST: u8 = 0x20;
/// `LMS_RST_PULSE`.
const LMS_RST_PULSE: u8 = 2;

/// Each register file with its write and read commands (`CMD_BRDSPI_*`, `CMD_LMS7002_*`).
const SPACES: [(Space, u8, u8); 2] = [(Space::Board, 0x55, 0x56), (Space::Lms7, 0x21, 0x22)];

/// `count` distinct records, the write flag set on odd ones.
fn records(count: usize) -> Vec<u32> {
    (0..count)
        .map(|i| {
            let i = u32::try_from(i).expect("few records");
            0x0102_0304_u32.wrapping_mul(i + 1) ^ ((i & 1) << 31)
        })
        .collect()
}

#[test]
fn register_writes_match_libusdrs_builder_for_every_record_count() {
    for (space, write, _) in SPACES {
        for count in 0..=MAX_RECORDS + 1 {
            let records = records(count);
            let bytes: Vec<u8> = records.iter().flat_map(|r| r.to_ne_bytes()).collect();
            assert_eq!(
                Frame::write(space, &records).map(Frame::to_bytes),
                lms64c_fill(write, &bytes),
                "{space:?}, {count} records"
            );
        }
    }
}

#[test]
fn register_reads_match_libusdrs_builder_for_every_address_count() {
    for (space, _, read) in SPACES {
        for count in 0..=MAX_RECORDS + 1 {
            let addrs: Vec<u16> = records(count)
                .iter()
                .map(|record| u16::try_from(record >> 16).expect("the upper half"))
                .collect();
            let bytes: Vec<u8> = addrs.iter().flat_map(|a| a.to_ne_bytes()).collect();
            assert_eq!(
                Frame::read(space, &addrs).map(Frame::to_bytes),
                lms64c_fill(read, &bytes),
                "{space:?}, {count} addresses"
            );
        }
    }
}

#[test]
fn the_reset_pulse_matches_libusdrs_builder() {
    assert_eq!(
        Some(Frame::lms7_reset_pulse().to_bytes()),
        lms64c_fill(CMD_LMS7002_RST, &[LMS_RST_PULSE])
    );
}

#[test]
fn reply_values_match_libusdrs_parser() {
    let mut reply = [0; FRAME_LEN];
    for (byte, value) in reply.iter_mut().zip(0x11_u8..) {
        *byte = value;
    }
    reply[1] = 1;
    let frame = Frame::from_bytes(reply);
    for (_, _, read) in SPACES {
        let parsed = lms64c_read_values(read, &reply, READ_VALUES_MAX);
        let ours: Vec<u16> = (0..READ_VALUES_MAX)
            .map(|index| frame.read_value(index).expect("within the payload"))
            .collect();
        assert_eq!(ours, parsed, "command {read:#04x}");
    }
    assert_eq!(frame.read_value(READ_VALUES_MAX), None, "past the payload");
}
