//! Adversarial decoding — the untrusted-input boundary. A DNS parser reads bytes from
//! the network; hostile input must produce a clean error, never a panic, hang, or
//! unbounded allocation.

mod adversarial {
    use dns::{Message, Name};

    #[test]
    fn self_referential_pointer_is_bounded() {
        // A name that is a pointer to itself would loop forever without the hop bound.
        let wire = [0xC0, 0x00];
        assert!(Name::decode_exact(&wire).is_err());
    }

    #[test]
    fn two_pointer_cycle_is_bounded() {
        // offset 0 → pointer to 2; offset 2 → pointer to 0.
        let wire = [0xC0, 0x02, 0xC0, 0x00];
        assert!(Name::decode_exact(&wire).is_err());
    }

    #[test]
    fn truncated_message_is_a_clean_error() {
        // Header claims one answer, but the buffer ends after the header.
        let wire = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(Message::decode_exact(&wire).is_err());
    }

    #[test]
    fn rdlength_past_the_buffer_is_a_clean_error() {
        // A record whose RDLENGTH (0xFFFF) far exceeds the remaining bytes.
        let wire = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, // header
            0x00, // root name
            0x00, 0x10, // TYPE = TXT
            0x00, 0x01, // CLASS = IN
            0x00, 0x00, 0x00, 0x00, // TTL
            0xFF, 0xFF, // RDLENGTH = 65535
            0xAA, // only 1 byte of RDATA
        ];
        assert!(Message::decode_exact(&wire).is_err());
    }

    #[test]
    fn a_pointer_out_of_range_is_a_clean_error() {
        // Header, then a question whose name points far past the (short) buffer.
        let wire = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // header
            0xC0, 0xFF, // name = pointer to offset 0x00FF (out of range)
            0x00, 0x01, 0x00, 0x01,
        ];
        assert!(Message::decode_exact(&wire).is_err());
    }

    #[test]
    fn a_name_over_255_bytes_is_rejected() {
        // Five 63-byte labels = 5 × 64 = 320 bytes of labels (> 255) then a terminator.
        let mut wire = Vec::new();
        for _ in 0..5 {
            wire.push(63u8);
            wire.extend(std::iter::repeat_n(b'a', 63));
        }
        wire.push(0x00);
        let err = Name::decode_exact(&wire).unwrap_err();
        assert!(
            matches!(&err.kind, bnb::bitstream::ErrorKind::Convert { message } if message.contains("255")),
            "expected a 255-byte-limit error, got {err:?}"
        );
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        // A range of malformed inputs — the contract is "error, not panic".
        let cases: &[&[u8]] = &[
            &[],
            &[0xFF],
            &[0xFF; 12],
            &[0x00; 13],
            &[0xC0; 64],
            &[0x3F; 40],
        ];
        for bytes in cases {
            let _ = Message::decode_exact(bytes); // must not panic
            let _ = Name::decode_exact(bytes);
        }
    }

    #[test]
    fn srv_rdlength_under_six_does_not_underflow_panic() {
        // an=1; SRV (TYPE=33) record with RDLENGTH=2 but 6 rdata bytes present, so the
        // `count = rdlength - 6` expression is reached. Must not panic (saturates to 0).
        let wire = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, // header
            0x00, // root name
            0x00, 0x21, // TYPE = SRV
            0x00, 0x01, // CLASS = IN
            0x00, 0x00, 0x00, 0x00, // TTL
            0x00, 0x02, // RDLENGTH = 2  (< 6)
            0x00, 0x0a, 0x00, 0x05, 0x13, 0x88, // priority/weight/port
        ];
        // A clean decode outcome (Ok or Err), never a panic.
        let _ = Message::decode_exact(&wire);
    }

    #[test]
    fn caa_tag_length_over_rdlength_does_not_underflow_panic() {
        // an=1; CAA (TYPE=257) with tag_length=200 > rdlength, and enough trailing bytes to
        // read the 200-byte tag, so `count = rdlength - tag_length - 2` is reached.
        let mut wire = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, // header
            0x00, // root name
            0x01, 0x01, // TYPE = CAA (257)
            0x00, 0x01, // CLASS = IN
            0x00, 0x00, 0x00, 0x00, // TTL
            0x00, 0x03, // RDLENGTH = 3
            0x00, 0xC8, // flags=0, tag_length=200
        ];
        wire.extend(std::iter::repeat_n(0x61, 200)); // 200 tag bytes so the tag read succeeds
        let _ = Message::decode_exact(&wire); // must not panic
    }
}
