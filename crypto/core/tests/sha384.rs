//! Public evidence for the readable SHA-384 implementation.
//!
//! Provenance: `tests/vectors/sha384/README.md`.

use rsl_crypto::digest::sha2::sha384::Sha384;
use sha2::{Digest as _, Sha384 as ReferenceSha384};

/// Published NIST one-block and two-block SHA-384 examples.
#[test]
fn nist_published_examples() {
    let cases = [
        (
            "abc",
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed\
             8086072ba1e7cc2358baeca134c825a7",
        ),
        (
            "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmn\
             hijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "09330c33f71147e83d192fc782cd1b4753111b173b3b05d22fa08086e3b0f712\
             fcc7c71a557e2db966c3e9fa91746039",
        ),
    ];
    for (message, expected) in cases {
        assert_eq!(Sha384::digest(message).unwrap().into_bytes(), hex(expected));
    }
}

/// Published NIST CAVP `SHA384ShortMsg.rsp` cases at the padding and block boundaries.
#[test]
fn cavp_boundary_lengths() {
    let cases = [
        (
            0,
            "",
            "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b",
        ),
        (
            8,
            "c5",
            "b52b72da75d0666379e20f9b4a79c33a329a01f06a2fb7865c9062a28c1de860ba432edfd86b4cb1cb8a75b46076e3b1",
        ),
        (
            888,
            "a04f390a9cc2effad05db80d9076a8d4b6cc8bba97b27b423670b290b8e69c2b187230011c1481ac88d090f39154659494db5e410851c6e8b2b8a93717cae76037e0881978124fe7e1a0929d8891491f4e99646cc94062dc82411fa66130eda46560e75b98048236439465125e737b",
            "e7089d72945cef851e689b4409cfb63d135f0b5cdfb0dac6c3a292dd70371ab4b79da1997d7992906ac7213502662920",
        ),
        (
            896,
            "f419494c3c6d0727b3395a483a2167182a7252f4fd099c2d4b71b053f94bb8b3adf3b51e8460cfec084ce9415c95798fbae4975c208c544645b54c44d2b97f2ecfce5c805be61f5ba1d35dcc07afdd51a87baa990506668cf710e18be9b0ebf943f366fa29c69f7a6616de72a3353b66",
            "aead8688c58c6ba4e9cadb4756b465dce0fb06f1cfaa478197f2ea89414e47e9572034adfed160703c79b82b3fd7ab78",
        ),
        (
            1016,
            "dbed7612448d46cbe0a384d1c93233f02ffd1c984ba765299518656d3723b766c1658d4b1e7047cdc729459e366ef9349efc40cbd990f2a9a24db7a5045e1dea12dce8f9d9f2aaed933f93031e7b8959ac5e7bf6bbbdf30b48f7eb783f8fe292371a2f245c5c94b4acae160767a20ce7c0ea7723d97691d8eedda9efd1fe2d",
            "fb531a1ed181c732311e56f4b56ed91dcacc0dd6bf1eb4a44be6f87dd7cb1ef9dfb0310f4a79eaaa3f32bf3914d8624e",
        ),
        (
            1024,
            "3bf52cc5ee86b9a0190f390a5c0366a560b557000dbe5115fd9ee11630a62769011575f15881198f227876e8fe685a6939bc8b89fd48a34ec5e71e131462b2886794dffa68ccc6d564733e67ffef25e627c6f4b5460796e3bce67bf58ca6e8e555bc916a8531697ac948b90dc8616f25101db90b50c3d3dbc9e21e42ff387187",
            "12b6cb35eda92ee37356ddee77781a17b3d90e563824a984faffc6fdd1693bd7626039635563cfc3b9a2b00f9c65eefd",
        ),
    ];
    for (bits, message, expected) in cases {
        let message = decode(message);
        assert_eq!(message.len() * 8, bits);
        assert_eq!(
            Sha384::digest(&message).unwrap().into_bytes(),
            hex(expected),
            "Len = {bits}"
        );
    }
}

/// Differential evidence over every important padding and block boundary, with fragmentation.
#[test]
fn varied_and_fragmented_messages_match_rustcrypto() {
    for length in [
        0_usize, 1, 2, 7, 31, 110, 111, 112, 113, 127, 128, 129, 255, 256, 1024, 4096,
    ] {
        let message: Vec<u8> = (0..length)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(73).wrapping_add(19))
            .collect();
        let expected = ReferenceSha384::digest(&message);
        let actual = Sha384::digest(&message).unwrap();
        assert_eq!(actual.as_ref(), expected.as_slice(), "length {length}");

        let mut fragmented = Sha384::new();
        for part in message.chunks(17) {
            fragmented.update(part).unwrap();
        }
        assert_eq!(
            fragmented.finalize().as_ref(),
            expected.as_slice(),
            "fragmented {length}"
        );
    }
}

fn hex(input: &str) -> [u8; 48] {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    core::array::from_fn(|index| {
        u8::from_str_radix(&compact[index * 2..index * 2 + 2], 16).expect("fixture is hex")
    })
}

fn decode(input: &str) -> Vec<u8> {
    (0..input.len() / 2)
        .map(|i| u8::from_str_radix(&input[i * 2..i * 2 + 2], 16).expect("fixture is hex"))
        .collect()
}
