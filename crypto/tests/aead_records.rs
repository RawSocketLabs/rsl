//! Public AEAD-record lifecycle, fragmentation, and rejection evidence.

use core::convert::Infallible;

use rsl_crypto::{
    CryptoError, Result,
    aead::{
        CounterNonceSequence, DataRecord, FinalRecord, NonceSequence, RecordBuilder, RecordSink,
        RecordWriteError,
        gcm::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce, Aes256GcmTag},
    },
};

const KEY: [u8; 32] = [0x42; 32];
const FIXED: [u8; 8] = *b"stream01";
const CONTEXT: &[u8] = b"aead-record public test v1";

fn algorithm() -> Aes256Gcm {
    Aes256Gcm::new(Aes256GcmKey::new(KEY))
}

fn builder() -> rsl_crypto::aead::ReadyRecordBuilder<Aes256Gcm, CounterNonceSequence<Aes256GcmNonce>>
{
    RecordBuilder::new(algorithm())
        .nonce_sequence(CounterNonceSequence::new(FIXED))
        .record_size(7)
        .context(CONTEXT)
}

fn seal_fragments(
    fragments: &[&[u8]],
) -> (Vec<DataRecord<Aes256GcmTag>>, FinalRecord<Aes256GcmTag>) {
    let mut sealer = builder().build_sealer().unwrap();
    let mut records = Vec::new();
    for fragment in fragments {
        records.extend(sealer.write(fragment).unwrap());
    }
    (records, sealer.finish().unwrap())
}

#[derive(Default)]
struct RecordingSink {
    data: Vec<DataRecord<Aes256GcmTag>>,
    final_record: Option<FinalRecord<Aes256GcmTag>>,
}

impl RecordSink<Aes256GcmTag> for RecordingSink {
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

#[test]
fn arbitrary_write_fragmentation_produces_identical_records() {
    let message = b"fragmentation does not change authenticated record boundaries";
    let one_shot = seal_fragments(&[message]);
    let fragmented = seal_fragments(&[
        &message[..1],
        &message[1..8],
        &message[8..19],
        &message[19..37],
        &message[37..],
    ]);

    assert_eq!(fragmented, one_shot);
    assert!(one_shot.0.iter().all(|record| record.plaintext_len() == 7));
    assert!(one_shot.1.plaintext_len() < 7);
}

#[test]
fn write_to_streams_the_same_records_as_collecting_write() {
    let message = b"fallible output does not change authenticated records";
    let expected = seal_fragments(&[message]);
    let mut sealer = builder().build_sealer().unwrap();
    let mut sink = RecordingSink::default();

    sealer.write_to(&message[..5], &mut sink).unwrap();
    sealer.write_to(&message[5..17], &mut sink).unwrap();
    sealer.write_to(&message[17..], &mut sink).unwrap();
    sealer.finish_to(&mut sink).unwrap();

    assert_eq!(sink.data, expected.0);
    assert_eq!(sink.final_record.unwrap(), expected.1);
}

#[test]
fn matching_opener_recovers_only_authenticated_ordered_records() {
    let message = b"a large value arrives in unrelated source fragments";
    let (records, final_record) = seal_fragments(&[&message[..5], &message[5..22], &message[22..]]);
    let mut opener = builder().build_opener().unwrap();
    let mut recovered = Vec::new();

    for record in &records {
        recovered.extend(opener.open_data(record).unwrap());
    }
    recovered.extend(opener.open_final(&final_record).unwrap());

    assert_eq!(recovered, message);
}

#[test]
fn an_exact_record_boundary_uses_an_empty_authenticated_final_record() {
    let (records, final_record) = seal_fragments(&[b"12345671234567"]);

    assert_eq!(records.len(), 2);
    assert_eq!(final_record.record_number(), 2);
    assert_eq!(final_record.plaintext_len(), 0);
    assert!(final_record.ciphertext().is_empty());

    let mut opener = builder().build_opener().unwrap();
    for record in &records {
        let _ = opener.open_data(record).unwrap();
    }
    assert_eq!(opener.open_final(&final_record).unwrap(), b"");
}

#[test]
fn wrong_order_is_rejected_without_advancing_the_opener() {
    let (records, _) = seal_fragments(&[b"12345671234567"]);
    let mut opener = builder().build_opener().unwrap();

    assert_eq!(
        opener.open_data(&records[1]),
        Err(CryptoError::AuthenticationFailed)
    );
    assert_eq!(opener.next_record_number(), 0);
    assert_eq!(opener.open_data(&records[0]).unwrap(), b"1234567");
    assert_eq!(opener.next_record_number(), 1);
}

#[test]
fn context_ciphertext_tag_number_and_length_changes_are_rejected() {
    let (records, _) = seal_fragments(&[b"1234567"]);
    let record = &records[0];

    let wrong_context = RecordBuilder::new(algorithm())
        .nonce_sequence(CounterNonceSequence::new(FIXED))
        .record_size(7)
        .context(b"different context")
        .build_opener()
        .unwrap();
    assert_eq!(
        wrong_context.open_final(&FinalRecord::from_parts(
            record.record_number(),
            0,
            record.ciphertext().to_vec(),
            *record.tag(),
        )),
        Err(CryptoError::AuthenticationFailed)
    );

    let mut changed_ciphertext = record.ciphertext().to_vec();
    changed_ciphertext[0] ^= 1;
    let changed_ciphertext = DataRecord::from_parts(
        record.record_number(),
        record.plaintext_len(),
        changed_ciphertext,
        *record.tag(),
    );
    assert_eq!(
        builder()
            .build_opener()
            .unwrap()
            .open_data(&changed_ciphertext),
        Err(CryptoError::AuthenticationFailed)
    );

    let mut changed_tag = record.tag().into_bytes();
    changed_tag[0] ^= 1;
    let changed_tag = DataRecord::from_parts(
        record.record_number(),
        record.plaintext_len(),
        record.ciphertext().to_vec(),
        Aes256GcmTag::new(changed_tag),
    );
    assert_eq!(
        builder().build_opener().unwrap().open_data(&changed_tag),
        Err(CryptoError::AuthenticationFailed)
    );

    let changed_number = DataRecord::from_parts(
        1,
        record.plaintext_len(),
        record.ciphertext().to_vec(),
        *record.tag(),
    );
    assert_eq!(
        builder().build_opener().unwrap().open_data(&changed_number),
        Err(CryptoError::AuthenticationFailed)
    );

    let changed_length = DataRecord::from_parts(
        record.record_number(),
        6,
        record.ciphertext().to_vec(),
        *record.tag(),
    );
    assert_eq!(
        builder().build_opener().unwrap().open_data(&changed_length),
        Err(CryptoError::AuthenticationFailed)
    );
}

#[test]
fn zero_record_size_is_rejected_at_build_time() {
    let result = RecordBuilder::new(algorithm())
        .nonce_sequence(CounterNonceSequence::<Aes256GcmNonce>::new(FIXED))
        .record_size(0)
        .build_sealer();

    assert!(matches!(
        result,
        Err(CryptoError::InvalidLength {
            name: "AEAD record size",
            expected: 1,
            actual: 0,
        })
    ));
}

struct TwoNonceSequence;

impl NonceSequence<Aes256GcmNonce> for TwoNonceSequence {
    fn nonce(&self, record_number: u64) -> Result<Aes256GcmNonce> {
        let counter = u8::try_from(record_number).map_err(|_| CryptoError::CounterExhausted)?;
        if counter > 1 {
            return Err(CryptoError::CounterExhausted);
        }
        Ok(Aes256GcmNonce::new([counter; 12]))
    }
}

#[test]
fn write_reserves_a_nonce_for_the_final_record_before_consuming_input() {
    let mut sealer = RecordBuilder::new(algorithm())
        .nonce_sequence(TwoNonceSequence)
        .record_size(1)
        .build_sealer()
        .unwrap();

    assert_eq!(sealer.write(b"a").unwrap().len(), 1);
    assert_eq!(sealer.next_record_number(), 1);
    assert_eq!(sealer.write(b"b"), Err(CryptoError::CounterExhausted));
    assert_eq!(sealer.next_record_number(), 1);

    let final_record = sealer.finish().unwrap();
    assert_eq!(final_record.record_number(), 1);
    assert_eq!(final_record.plaintext_len(), 0);
}

struct FailingSink {
    accepted: Vec<DataRecord<Aes256GcmTag>>,
    calls: usize,
    fail_on: usize,
}

impl RecordSink<Aes256GcmTag> for FailingSink {
    type Error = &'static str;

    fn write_data(
        &mut self,
        record: DataRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        let call = self.calls;
        self.calls += 1;
        if call == self.fail_on {
            return Err("selected data-record failure");
        }
        self.accepted.push(record);
        Ok(())
    }

    fn write_final(
        &mut self,
        _record: FinalRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn every_sink_failure_boundary_invalidates_the_sealer() {
    let message = b"123456712345671234567";

    for fail_on in 0..3 {
        let mut sealer = builder().build_sealer().unwrap();
        let mut sink = FailingSink {
            accepted: Vec::new(),
            calls: 0,
            fail_on,
        };

        assert_eq!(
            sealer.write_to(message, &mut sink),
            Err(RecordWriteError::Sink("selected data-record failure"))
        );
        assert_eq!(sink.accepted.len(), fail_on);
        assert_eq!(sink.calls, fail_on + 1);
        assert_eq!(sealer.next_record_number(), (fail_on + 1) as u64);

        let mut later_sink = RecordingSink::default();
        assert_eq!(
            sealer.write_to(b"later", &mut later_sink),
            Err(RecordWriteError::Crypto(CryptoError::StateInvalidated))
        );
        assert_eq!(sealer.write(b"later"), Err(CryptoError::StateInvalidated));
        assert_eq!(sealer.finish(), Err(CryptoError::StateInvalidated));
    }
}

struct PanickingSink;

impl RecordSink<Aes256GcmTag> for PanickingSink {
    type Error = Infallible;

    fn write_data(
        &mut self,
        _record: DataRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        panic!("selected sink panic")
    }

    fn write_final(
        &mut self,
        _record: FinalRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn a_caught_sink_panic_cannot_leave_reusable_nonce_state() {
    let mut sealer = builder().build_sealer().unwrap();
    let mut sink = PanickingSink;

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = sealer.write_to(b"1234567", &mut sink);
    }));

    assert!(panic.is_err());
    assert_eq!(sealer.next_record_number(), 1);
    assert_eq!(sealer.write(b"later"), Err(CryptoError::StateInvalidated));
}

struct FinalFailingSink;

impl RecordSink<Aes256GcmTag> for FinalFailingSink {
    type Error = &'static str;

    fn write_data(
        &mut self,
        _record: DataRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        Ok(())
    }

    fn write_final(
        &mut self,
        _record: FinalRecord<Aes256GcmTag>,
    ) -> core::result::Result<(), Self::Error> {
        Err("selected final-record failure")
    }
}

#[test]
fn finish_to_reports_final_sink_failure_after_consuming_the_sealer() {
    let mut sealer = builder().build_sealer().unwrap();
    let mut sink = FinalFailingSink;
    sealer.write_to(b"tail", &mut sink).unwrap();

    assert_eq!(
        sealer.finish_to(&mut sink),
        Err(RecordWriteError::Sink("selected final-record failure"))
    );
}

#[test]
fn secret_owning_record_states_redact_the_algorithm() {
    let sealer = builder().build_sealer().unwrap();
    let opener = builder().build_opener().unwrap();

    assert!(format!("{sealer:?}").contains("algorithm: \"[REDACTED]\""));
    assert!(format!("{opener:?}").contains("algorithm: \"[REDACTED]\""));
}
