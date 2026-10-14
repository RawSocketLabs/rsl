//! USB simple-I²C cache/control sequence against the pinned, compiled C helpers.

use usdr_oracle as _;

// Compile the driver's private wire encoder directly: no mock or testing API is public.
#[path = "../../src/portable/usb/protocol.rs"]
mod protocol;

unsafe extern "C" {
    fn rsl_oracle_si2c_sequence(requests: *const u32, count: usize, out: *mut u32) -> i32;
}

#[test]
fn wire_headers_match_pinned_usb_fixtures() {
    // usb_async_regread32: direction3, 64 words minus one, first register 0x1234.
    assert_eq!(
        protocol::Command::encode(0x1234, 64, true),
        [0x34, 0x12, 0x3f, 0xc0]
    );
    assert_eq!(
        protocol::Command::encode(0x1234, 63, false),
        [0x34, 0x12, 0x3e, 0]
    );
    // libusb_transfer_ntfy: sequence 0xabcd, one additional payload word, IRQ6.
    let header = protocol::Notification::from_raw(0xabcd_1006);
    assert_eq!(header.sequence(), 0xabcd);
    assert_eq!(header.extra_words().value(), 1);
    assert_eq!(header.event().value(), 6);
}

#[test]
fn complete_notification_packets_validate_before_committing_the_sequence() {
    // libusb_transfer_ntfy's pinned wire format, including wrapping global sequence.
    let packet = [6, 0, 0xff, 0xff, 0x0c, 0x80, 0, 0];
    let mut sequence = Some(0xffff);
    assert_eq!(
        protocol::Notification::completion(&packet, 6, &mut sequence),
        Ok(Some(0x800c))
    );
    assert_eq!(sequence, Some(0));
    // Even a valid target record must not hide a truncated tail.
    let malformed = [6, 0, 0, 0, 0x0c, 0x80, 0, 0, 2, 0, 1, 0];
    assert!(protocol::Notification::completion(&malformed, 6, &mut sequence).is_err());
    assert_eq!(sequence, Some(0));
}

#[test]
fn cache_and_control_words_match_c_for_replacement_hits_and_all_lengths() {
    let mut cache = protocol::I2cCache::default();
    let mut requests = Vec::new();
    let mut actual = Vec::new();
    // Repeated hits, eviction, bus transitions, address zero, and every transfer length.
    for cycle in 0..100_u32 {
        for bus in 0..2_u8 {
            for address in [0, 0x60, 0x6a, 0x75, 0x60, 0x4e, 0x12] {
                for write_count in 0..4_u8 {
                    for read_count in 0..5_u8 {
                        let bytes = [cycle.to_le_bytes()[0], 0x34, 0x56];
                        let write = &bytes[..usize::from(write_count)];
                        let shape = u32::from(address)
                            | u32::from(bus) << 8
                            | u32::from(write_count) << 16
                            | u32::from(read_count) << 24;
                        requests
                            .extend([shape, u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])]);
                        actual.extend(cache.command(address, bus, write, read_count));
                    }
                }
            }
        }
    }
    let mut expected = vec![0; actual.len()];
    // SAFETY: requests and expected each contain two u32 words per request; both live
    // throughout the synchronous C call. C owns only its stack-local cache.
    let status = unsafe {
        rsl_oracle_si2c_sequence(requests.as_ptr(), requests.len() / 2, expected.as_mut_ptr())
    };
    assert_eq!(status, 0);
    assert_eq!(actual, expected);
}
