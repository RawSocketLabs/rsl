//! Authenticated decryption fed arbitrary nonce, AAD, ciphertext, and tag bytes must return
//! `Ok`/`Err` and never panic, read out of bounds, or release plaintext when the tag is wrong.
//! The target also checks that a genuine seal/open round trip recovers the plaintext.
//!
//! Run: `cargo +nightly fuzz run aead_open --fuzz-dir crypto/fuzz`.
#![no_main]

use core::convert::Infallible;

use libfuzzer_sys::fuzz_target;
use rsl_crypto::{
    CryptoError,
    aead::{
        Aead, CounterNonceSequence, DataRecord, FinalRecord, RecordBuilder, RecordSink,
        RecordWriteError,
        chacha20poly1305::{
            ChaCha20Poly1305, ChaCha20Poly1305Key, ChaCha20Poly1305Nonce, ChaCha20Poly1305Tag,
        },
        gcm::{
            Aes128Gcm, Aes128GcmKey, Aes128GcmNonce, Aes128GcmTag, Aes256Gcm, Aes256GcmKey,
            Aes256GcmNonce, Aes256GcmTag,
        },
    },
};

#[derive(Default)]
struct CollectingSink {
    data: Vec<DataRecord<Aes256GcmTag>>,
    final_record: Option<FinalRecord<Aes256GcmTag>>,
}

impl RecordSink<Aes256GcmTag> for CollectingSink {
    type Error = Infallible;

    fn write_data(
        &mut self,
        record: DataRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        self.data.push(record);
        Ok(())
    }

    fn write_final(
        &mut self,
        record: FinalRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        self.final_record = Some(record);
        Ok(())
    }
}

struct RejectingSink {
    calls: usize,
    reject_on: usize,
}

impl RecordSink<Aes256GcmTag> for RejectingSink {
    type Error = ();

    fn write_data(
        &mut self,
        _record: DataRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        let call = self.calls;
        self.calls += 1;
        if call == self.reject_on {
            Err(())
        } else {
            Ok(())
        }
    }

    fn write_final(
        &mut self,
        _record: FinalRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        Ok(())
    }
}

fn split(data: &[u8]) -> Option<([u8; 32], [u8; 12], [u8; 16], usize, &[u8])> {
    if data.len() < 32 + 12 + 16 + 1 {
        return None;
    }
    let key: [u8; 32] = data[..32].try_into().ok()?;
    let nonce: [u8; 12] = data[32..44].try_into().ok()?;
    let tag: [u8; 16] = data[44..60].try_into().ok()?;
    let aad_len = usize::from(data[60]);
    Some((key, nonce, tag, aad_len, &data[61..]))
}

fuzz_target!(|data: &[u8]| {
    let Some((key, nonce, tag, aad_len, rest)) = split(data) else {
        return;
    };
    let aad_len = aad_len.min(rest.len());
    let (aad, payload) = rest.split_at(aad_len);

    // Arbitrary tag over arbitrary ciphertext: must be rejected or, vanishingly rarely, accepted
    // — never a panic.
    let gcm128 = Aes128Gcm::new(Aes128GcmKey::new(key[..16].try_into().unwrap()));
    let _ = gcm128.open(&Aes128GcmNonce::new(nonce), aad, payload, &Aes128GcmTag::new(tag));
    let gcm256 = Aes256Gcm::new(Aes256GcmKey::new(key));
    let _ = gcm256.open(&Aes256GcmNonce::new(nonce), aad, payload, &Aes256GcmTag::new(tag));
    let chacha = ChaCha20Poly1305::new(ChaCha20Poly1305Key::new(key));
    let _ = chacha.open(
        &ChaCha20Poly1305Nonce::new(nonce),
        aad,
        payload,
        &ChaCha20Poly1305Tag::new(tag),
    );

    // Genuine round trips must succeed, and a flipped tag bit must fail.
    let sealed = gcm256.seal(&Aes256GcmNonce::new(nonce), aad, payload).unwrap();
    assert_eq!(
        gcm256
            .open(&Aes256GcmNonce::new(nonce), aad, sealed.ciphertext(), sealed.tag())
            .unwrap(),
        payload
    );
    let mut wrong = sealed.tag().into_bytes();
    wrong[0] ^= 1;
    assert!(
        gcm256
            .open(&Aes256GcmNonce::new(nonce), aad, sealed.ciphertext(), &Aes256GcmTag::new(wrong))
            .is_err()
    );
    let sealed = Aead::seal(&chacha, &ChaCha20Poly1305Nonce::new(nonce), aad, payload).unwrap();
    assert_eq!(
        Aead::open(&chacha, &ChaCha20Poly1305Nonce::new(nonce), aad, sealed.ciphertext(), sealed.tag()).unwrap(),
        payload
    );

    // The record layer must preserve plaintext across arbitrary source fragmentation and reject
    // arbitrary parsed records without panicking or advancing before authentication.
    let record_size = aad_len % 31 + 1;
    let fixed: [u8; 8] = nonce[..8].try_into().unwrap();
    let mut sealer = RecordBuilder::new(Aes256Gcm::new(Aes256GcmKey::new(key)))
        .nonce_sequence(CounterNonceSequence::<Aes256GcmNonce>::new(fixed))
        .record_size(record_size)
        .context(aad)
        .build_sealer()
        .unwrap();
    let split = payload.len() / 2;
    let mut sink = CollectingSink::default();
    sealer.write_to(&payload[..split], &mut sink).unwrap();
    sealer.write_to(&payload[split..], &mut sink).unwrap();
    sealer.finish_to(&mut sink).unwrap();
    let final_record = sink.final_record.as_ref().unwrap();

    let mut opener = RecordBuilder::new(Aes256Gcm::new(Aes256GcmKey::new(key)))
        .nonce_sequence(CounterNonceSequence::<Aes256GcmNonce>::new(fixed))
        .record_size(record_size)
        .context(aad)
        .build_opener()
        .unwrap();
    let mut recovered = Vec::new();
    for record in &sink.data {
        recovered.extend(opener.open_data(record).unwrap());
    }
    recovered.extend(opener.open_final(final_record).unwrap());
    assert_eq!(recovered, payload);

    let completed_records = payload.len() / record_size;
    if completed_records != 0 {
        let mut failing_sealer =
            RecordBuilder::new(Aes256Gcm::new(Aes256GcmKey::new(key)))
                .nonce_sequence(CounterNonceSequence::<Aes256GcmNonce>::new(fixed))
                .record_size(record_size)
                .context(aad)
                .build_sealer()
                .unwrap();
        let mut failing_sink = RejectingSink {
            calls: 0,
            reject_on: usize::from(tag[0]) % completed_records,
        };
        assert!(matches!(
            failing_sealer.write_to(payload, &mut failing_sink),
            Err(RecordWriteError::Sink(()))
        ));
        assert!(matches!(
            failing_sealer.write_to([], &mut failing_sink),
            Err(RecordWriteError::Crypto(CryptoError::StateInvalidated))
        ));
    }

    let arbitrary_data = DataRecord::from_parts(
        0,
        record_size,
        payload.to_vec(),
        Aes256GcmTag::new(tag),
    );
    let mut arbitrary_opener = RecordBuilder::new(Aes256Gcm::new(Aes256GcmKey::new(key)))
        .nonce_sequence(CounterNonceSequence::<Aes256GcmNonce>::new(fixed))
        .record_size(record_size)
        .context(aad)
        .build_opener()
        .unwrap();
    let _ = arbitrary_opener.open_data(&arbitrary_data);

    let arbitrary_final = FinalRecord::from_parts(0, 0, payload.to_vec(), Aes256GcmTag::new(tag));
    let arbitrary_opener = RecordBuilder::new(Aes256Gcm::new(Aes256GcmKey::new(key)))
        .nonce_sequence(CounterNonceSequence::<Aes256GcmNonce>::new(fixed))
        .record_size(record_size)
        .context(aad)
        .build_opener()
        .unwrap();
    let _ = arbitrary_opener.open_final(&arbitrary_final);
});
