//! Adversarial decoding: server bytes are untrusted, so hostile or truncated input must be a
//! clean error or a preserved value, never a panic.

mod adversarial {
    use rtltcp::{Command, Greeting, Opcode};

    #[test]
    fn truncated_greeting_is_a_clean_error() {
        assert!(Greeting::decode_exact(b"RTL0\0\0\0\x05\0\0").is_err());
    }

    #[test]
    fn foreign_magic_is_a_bad_magic_error() {
        let err = Greeting::decode_exact(b"HTTP/1.1 200").unwrap_err();
        assert!(matches!(err.kind, bnb::ErrorKind::BadMagic { .. }), "{err}");
    }

    #[test]
    fn unknown_opcode_is_preserved() {
        let wire = [0x7f, 0xde, 0xad, 0xbe, 0xef];
        let command = Command::decode_exact(&wire).unwrap();
        assert_eq!(command.opcode, Opcode::Other(0x7f));
        assert_eq!(command.param, 0xdead_beef);
        assert_eq!(command.to_bytes().unwrap(), wire);
    }

    #[test]
    fn truncated_command_is_a_clean_error() {
        assert!(Command::decode_exact(&[0x01, 0x00, 0x00]).is_err());
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        for len in 0u8..=16 {
            let wire: Vec<u8> = (0..len)
                .map(|i| i.wrapping_mul(37).wrapping_add(11))
                .collect();
            let _ = Greeting::decode_exact(&wire);
            let _ = Command::decode_exact(&wire);
        }
    }
}
