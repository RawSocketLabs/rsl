//! Published, boundary, streaming, classification, and independent RC4 evidence.

use rc4::{KeyInit as _, Rc4 as ReferenceRc4, StreamCipher as _};
use rsl_crypto::cipher::StreamCipher as StreamCipherContract;
use rsl_crypto_legacy::{
    CryptoError, SecurityStatus,
    cipher::rc4::{Rc4, Rc4Key, SECURITY_STATUS},
};

#[test]
fn rfc_6229_40_bit_key_covers_distant_stream_offsets() {
    let mut cipher = Rc4::new(Rc4Key::try_from_slice(&[1, 2, 3, 4, 5]).unwrap());
    let mut stream = [0_u8; 4_112];
    cipher.apply_keystream(&mut stream).unwrap();

    assert_eq!(
        &stream[0..32],
        &hex::<32>("b2396305f03dc027ccc3524a0a1118a86982944f18fc82d589c403a47a0d0919")
    );
    assert_eq!(
        &stream[240..272],
        &hex::<32>("28cb1132c96ce286421dcaadb8b69eae1cfcf62b03eddb641d77dfcf7f8d8c93")
    );
    assert_eq!(
        &stream[1_520..1_552],
        &hex::<32>("3294f744d8f9790507e70f62e5bbceead8729db41882259bee4f825325f5a130")
    );
    assert_eq!(
        &stream[4_080..4_112],
        &hex::<32>("068326a2118416d21f9d04b2cd1ca050ff25b58995996707e51fbdf08b34d875")
    );
    assert_eq!(cipher.position(), 4_112);
}

#[test]
fn explicit_ssh_era_discard_reaches_rfc_6229_offset_1536() {
    let mut cipher = Rc4::new(Rc4Key::try_from_slice(&[1, 2, 3, 4, 5]).unwrap());
    cipher.discard(1_536).unwrap();
    let mut output = [0_u8; 16];
    cipher.apply_keystream(&mut output).unwrap();
    assert_eq!(output, hex("d8729db41882259bee4f825325f5a130"));
    assert_eq!(cipher.position(), 1_552);
}

#[test]
fn fresh_identical_states_reverse_the_same_bytes() {
    let key = b"one state per direction";
    let mut sender = Rc4::new(Rc4Key::try_from_slice(key).unwrap());
    let mut receiver = Rc4::new(Rc4Key::try_from_slice(key).unwrap());
    let plaintext = *b"historical wire payload";
    let mut protected = plaintext;

    sender.apply_keystream(&mut protected).unwrap();
    assert_ne!(protected, plaintext);
    receiver.apply_keystream(&mut protected).unwrap();
    assert_eq!(protected, plaintext);
}

#[test]
fn fragmented_contract_calls_match_one_shot_and_rustcrypto() {
    for key_len in [1_usize, 5, 16, 32, 256] {
        let key: Vec<u8> = (0..key_len)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(29).wrapping_add(11))
            .collect();
        for length in [0_usize, 1, 15, 16, 255, 256, 257, 1_536, 4_097] {
            let input: Vec<u8> = (0..length)
                .map(|index| index.to_le_bytes()[0].wrapping_mul(73).wrapping_add(3))
                .collect();

            let mut expected = input.clone();
            let mut reference = ReferenceRc4::new_from_slice(&key).unwrap();
            reference.apply_keystream(&mut expected);

            let mut actual = input;
            let mut implementation = Rc4::new(Rc4Key::try_from_slice(&key).unwrap());
            for part in actual.chunks_mut(13) {
                apply_through_contract(&mut implementation, part).unwrap();
            }
            assert_eq!(actual, expected, "key length {key_len}, input {length}");
            assert_eq!(implementation.position(), u64::try_from(length).unwrap());
        }
    }
}

#[test]
fn rc4_is_machine_readable_as_broken() {
    assert_eq!(SECURITY_STATUS, SecurityStatus::Broken);
    assert_ne!(SECURITY_STATUS, SecurityStatus::Recommended);
}

fn apply_through_contract<C: StreamCipherContract>(
    cipher: &mut C,
    bytes: &mut [u8],
) -> Result<(), CryptoError> {
    cipher.apply_keystream(bytes)
}

fn hex<const N: usize>(input: &str) -> [u8; N] {
    assert_eq!(input.len(), N * 2);
    core::array::from_fn(|index| {
        u8::from_str_radix(&input[index * 2..index * 2 + 2], 16).expect("fixture contains hex")
    })
}
