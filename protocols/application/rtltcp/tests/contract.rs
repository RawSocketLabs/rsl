//! Golden wire vectors: the byte-identity anchor for the greeting and every typed command.

mod contract {
    use rtltcp::{Command, Greeting, Opcode};

    #[test]
    fn greeting_is_magic_then_two_big_endian_words() {
        let wire = Greeting::new(1, 29).to_bytes().unwrap();
        assert_eq!(
            wire,
            [
                b'R', b'T', b'L', b'0', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1d
            ]
        );
    }

    #[test]
    fn typed_commands_encode_opcode_then_big_endian_param() {
        let cases: [(Command, [u8; 5]); 8] = [
            (
                Command::set_center_frequency(462_562_500),
                [0x01, 0x1b, 0x92, 0x24, 0xc4],
            ),
            (
                Command::set_sample_rate(2_048_000),
                [0x02, 0x00, 0x1f, 0x40, 0x00],
            ),
            (Command::set_gain_mode(true), [0x03, 0x00, 0x00, 0x00, 0x01]),
            (
                Command::set_gain_mode(false),
                [0x03, 0x00, 0x00, 0x00, 0x00],
            ),
            (Command::set_gain(250), [0x04, 0x00, 0x00, 0x00, 0xfa]),
            (Command::set_agc_mode(false), [0x08, 0x00, 0x00, 0x00, 0x00]),
            (
                Command::set_tuner_gain_by_index(28),
                [0x0d, 0x00, 0x00, 0x00, 0x1c],
            ),
            (
                Command::new(Opcode::SetBiasTee, 1),
                [0x0e, 0x00, 0x00, 0x00, 0x01],
            ),
        ];
        for (command, wire) in cases {
            assert_eq!(command.to_bytes().unwrap(), wire, "{command:?}");
            assert_eq!(Command::decode_exact(&wire).unwrap(), command);
        }
    }
}
