//! Public round-trip, cleartext-AAD, and authentication-failure evidence.

use rsl_crypto::{
    CryptoError, Result,
    aead::{
        Aead,
        gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce, Aes128GcmTag},
    },
    random::RandomSource,
};

const KEY: [u8; 16] = *b"readable key-128";
const NONCE: [u8; 12] = *b"unique-nonce";

fn algorithm() -> Aes128Gcm {
    Aes128Gcm::new(Aes128GcmKey::new(KEY))
}

/// API-regression evidence that AES-128-GCM satisfies the crate-wide AEAD contract.
fn round_trip_through_trait<C>(
    algorithm: &C,
    nonce: &Aes128GcmNonce,
    associated_data: &[u8],
    plaintext: &[u8],
) where
    C: Aead<Nonce = Aes128GcmNonce, Tag = Aes128GcmTag>,
{
    let sealed = algorithm
        .seal(nonce, associated_data, plaintext)
        .expect("the short regression fixture is valid");
    let opened = algorithm
        .open(nonce, associated_data, sealed.ciphertext(), sealed.tag())
        .expect("the freshly produced tag authenticates");

    assert_eq!(opened, plaintext);
}

#[test]
fn strings_round_trip_as_their_utf8_bytes_through_the_aead_trait() {
    let algorithm = algorithm();
    let nonce = Aes128GcmNonce::new(NONCE);

    round_trip_through_trait(
        &algorithm,
        &nonce,
        "visible record header".as_bytes(),
        "protected payload".as_bytes(),
    );
}

#[test]
fn associated_data_stays_clear_but_is_bound_to_the_ciphertext() {
    let algorithm = algorithm();
    let nonce = Aes128GcmNonce::new(NONCE);
    let cleartext_header = *b"wire header";
    let original_header = cleartext_header;
    let sealed = algorithm
        .seal(&nonce, &cleartext_header, b"encrypted body")
        .expect("the short regression fixture is valid");

    // The caller still owns the exact header bytes and may encode/write them without encryption.
    assert_eq!(cleartext_header, original_header);

    let mut changed_header = cleartext_header;
    changed_header[0] ^= 1;
    assert_eq!(
        algorithm.open(&nonce, &changed_header, sealed.ciphertext(), sealed.tag(),),
        Err(CryptoError::AuthenticationFailed)
    );
}

#[test]
fn every_changed_tag_byte_rejects_without_plaintext() {
    let algorithm = algorithm();
    let nonce = Aes128GcmNonce::new(NONCE);
    let sealed = algorithm
        .seal(&nonce, b"header", b"payload spanning more than one block")
        .expect("the short regression fixture is valid");

    for index in 0..Aes128GcmTag::LEN {
        let mut changed = sealed.tag().into_bytes();
        changed[index] ^= 1;

        assert_eq!(
            algorithm.open(
                &nonce,
                b"header",
                sealed.ciphertext(),
                &Aes128GcmTag::new(changed),
            ),
            Err(CryptoError::AuthenticationFailed)
        );
    }
}

#[test]
fn changed_nonce_or_any_ciphertext_byte_rejects_without_plaintext() {
    let algorithm = algorithm();
    let nonce = Aes128GcmNonce::new(NONCE);
    let sealed = algorithm
        .seal(&nonce, b"header", b"payload spanning more than one block")
        .expect("the short regression fixture is valid");

    let mut changed_nonce = nonce.into_bytes();
    changed_nonce[0] ^= 1;
    assert_eq!(
        algorithm.open(
            &Aes128GcmNonce::new(changed_nonce),
            b"header",
            sealed.ciphertext(),
            sealed.tag(),
        ),
        Err(CryptoError::AuthenticationFailed)
    );

    for index in 0..sealed.ciphertext().len() {
        let mut changed_ciphertext = sealed.ciphertext().to_vec();
        changed_ciphertext[index] ^= 1;

        assert_eq!(
            algorithm.open(&nonce, b"header", &changed_ciphertext, sealed.tag(),),
            Err(CryptoError::AuthenticationFailed)
        );
    }
}

#[test]
fn parsed_wire_nonce_and_tag_open_a_detached_ciphertext() {
    let algorithm = algorithm();
    let sender_nonce = Aes128GcmNonce::new(NONCE);
    let sealed = algorithm
        .seal(&sender_nonce, b"wire header", b"wire payload")
        .expect("the short regression fixture is valid");

    let parsed_nonce = Aes128GcmNonce::try_from(NONCE.as_slice())
        .expect("the wire nonce contains exactly twelve bytes");
    let parsed_tag = Aes128GcmTag::try_from(sealed.tag().as_ref())
        .expect("the detached wire tag contains exactly sixteen bytes");
    let opened = algorithm
        .open(
            &parsed_nonce,
            b"wire header",
            sealed.ciphertext(),
            &parsed_tag,
        )
        .expect("the parsed wire values authenticate");

    assert_eq!(opened, b"wire payload");
}

#[test]
fn caller_selected_source_generates_exact_key_and_nonce_widths() {
    struct RecordingSource {
        next: u8,
        requests: Vec<usize>,
    }

    impl RandomSource for RecordingSource {
        fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
            self.requests.push(output.len());
            for byte in output {
                *byte = self.next;
                self.next = self.next.wrapping_add(1);
            }
            Ok(())
        }
    }

    let mut source = RecordingSource {
        next: 0,
        requests: Vec::new(),
    };
    let generated = Aes128Gcm::new(Aes128GcmKey::generate(&mut source).unwrap());
    let generated_nonce = Aes128GcmNonce::generate(&mut source).unwrap();
    assert_eq!(source.requests, [Aes128GcmKey::LEN, Aes128GcmNonce::LEN]);
    assert_eq!(
        generated_nonce.into_bytes(),
        core::array::from_fn(|index| 16 + u8::try_from(index).unwrap())
    );

    let exact_key = core::array::from_fn(|index| u8::try_from(index).unwrap());
    let exact = Aes128Gcm::new(Aes128GcmKey::new(exact_key));
    let generated_output = generated
        .seal(&generated_nonce, b"header", b"payload")
        .unwrap();
    let exact_output = exact.seal(&generated_nonce, b"header", b"payload").unwrap();
    assert_eq!(generated_output, exact_output);
}

#[test]
fn generation_propagates_entropy_failure_without_returning_a_value() {
    struct PartialFailure;

    impl RandomSource for PartialFailure {
        fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
            output[..3].fill(0xa5);
            Err(CryptoError::EntropyUnavailable)
        }
    }

    assert!(matches!(
        Aes128GcmKey::generate(&mut PartialFailure),
        Err(CryptoError::EntropyUnavailable)
    ));
    assert_eq!(
        Aes128GcmNonce::generate(&mut PartialFailure),
        Err(CryptoError::EntropyUnavailable)
    );
}
