//! Every digest, MAC, and XOF must give the same output for a message fed in one call and in
//! arbitrary fragments, and must never panic on any input length.
//!
//! Run: `cargo +nightly fuzz run digest_fragmentation --fuzz-dir crypto/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rsl_crypto::{
    digest::{
        sha2::{sha256::Sha256, sha384::Sha384, sha512::Sha512},
        sha3::{Sha3_256, Shake256},
    },
    kdf::hkdf::{sha256 as hkdf256, sha384 as hkdf384},
    mac::{
        hmac::{sha256::HmacSha256, sha384::HmacSha384},
        poly1305::{Poly1305, Poly1305Key},
    },
};

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let chunk = usize::from(data[0]).max(1);
    let message = &data[1..];

    macro_rules! check_digest {
        ($type:ty) => {{
            let mut fragmented = <$type>::new();
            for part in message.chunks(chunk) {
                fragmented.update(part).unwrap();
            }
            assert_eq!(fragmented.finalize(), <$type>::digest(message).unwrap());
        }};
    }
    check_digest!(Sha256);
    check_digest!(Sha384);
    check_digest!(Sha512);
    check_digest!(Sha3_256);

    let mut whole = [0_u8; 200];
    Shake256::digest_into(message, &mut whole);
    let mut xof = Shake256::new();
    for part in message.chunks(chunk) {
        xof.update(part);
    }
    let mut pieces = [0_u8; 200];
    let (first, second) = pieces.split_at_mut(chunk.min(200));
    xof.squeeze(first);
    xof.squeeze(second);
    assert_eq!(pieces, whole);

    let key = &message[..message.len().min(200)];
    let mut mac = HmacSha256::new(key).unwrap();
    for part in message.chunks(chunk) {
        mac.update(part).unwrap();
    }
    assert_eq!(mac.finalize(), HmacSha256::authenticate(key, message).unwrap());
    let mut mac = HmacSha384::new(key).unwrap();
    for part in message.chunks(chunk) {
        mac.update(part).unwrap();
    }
    assert_eq!(mac.finalize(), HmacSha384::authenticate(key, message).unwrap());

    let mut poly = Poly1305::new(Poly1305Key::new([0x42; 32]));
    for part in message.chunks(chunk) {
        poly.update(part);
    }
    assert_eq!(poly.finalize(), Poly1305::authenticate(Poly1305Key::new([0x42; 32]), message));

    let mut out256 = [0_u8; 64];
    hkdf256::derive(Some(key), message, key, &mut out256).unwrap();
    let mut out384 = [0_u8; 64];
    hkdf384::derive(None, message, key, &mut out384).unwrap();
});
