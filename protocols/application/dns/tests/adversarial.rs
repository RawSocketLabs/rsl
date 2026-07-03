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
}
