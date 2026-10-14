//! Bounded-memory AEAD records for a byte stream.
//!
//! [`Aead::seal`] already accepts an arbitrary-length in-memory message. This
//! module is for input that should be processed incrementally: [`RecordSealer::write_to`] accepts
//! fragments of any size, sends full authenticated records to a caller-selected [`RecordSink`],
//! and retains only the final partial record. [`RecordSealer::finish_to`] consumes the sealer and
//! sends the authenticated end record. [`RecordOpener::open_data_to`] authenticates each record
//! before moving its plaintext into a [`RecordPlaintextSink`].
//!
//! The API exposes the useful state transitions directly. There are no public `Missing` or
//! `Present` marker values:
//!
//! - [`RecordBuilder::new`] returns a stage that can accept a nonce sequence;
//! - [`RecordBuilderWithSequence::record_size`] returns the only stage with `build_*` methods;
//! - [`RecordSealer::finish_to`] consumes the sealer, so no more input can be written afterward;
//! - [`RecordOpener::open_final_to`] consumes the opener after the authenticated end record;
//! - data and final records have distinct [`DataRecord`] and [`FinalRecord`] types.
//!
//! This is a wire-independent record contract, not TLS, SSH, or a file format. It does not encode
//! records, negotiate algorithms, derive keys, choose key lifetimes, provide a replay window, or
//! parse a byte stream of ciphertext. A consuming format must carry enough information to
//! reconstruct each record and must require exactly one successfully opened final record.
//!
//! # Authenticated record metadata
//!
//! Each invocation binds the following unambiguous AAD, in order:
//!
//! | Field | Encoding |
//! | --- | --- |
//! | domain separator | ASCII `rsl-crypto/aead-record/v1`, then `00` |
//! | context length | 64-bit unsigned big-endian byte length |
//! | context | exact caller-supplied bytes |
//! | configured record size | 64-bit unsigned big-endian byte length |
//! | record number | 64-bit unsigned big-endian integer |
//! | record kind | `00` for data, `01` for final |
//! | plaintext length | 64-bit unsigned big-endian byte length |
//!
//! The length prefix makes the variable-length context unambiguous as required by RFC 5116 §3.3.
//! Record number, kind, and length are visible metadata, but changing any of them invalidates the
//! tag. This exact encoding is local to this crate; RFC 5116 supplies the AEAD interface and
//! input-construction requirements, not this record format.
//!
//! # AES-256-GCM example
//!
//! ```
//! use core::convert::Infallible;
//! use rsl_crypto::aead::{
//!     CounterNonceSequence, DataRecord, FinalRecord, RecordBuilder, RecordPlaintextSink,
//!     RecordSink, gcm::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce, Aes256GcmTag},
//! };
//!
//! #[derive(Default)]
//! struct Records {
//!     data: Vec<DataRecord<Aes256GcmTag>>,
//!     final_record: Option<FinalRecord<Aes256GcmTag>>,
//! }
//!
//! #[derive(Default)]
//! struct Plaintext(Vec<u8>);
//!
//! impl RecordPlaintextSink for Plaintext {
//!     type Error = Infallible;
//!
//!     fn write_data(&mut self, plaintext: Vec<u8>) -> Result<(), Self::Error> {
//!         self.0.extend(plaintext);
//!         Ok(())
//!     }
//!
//!     fn write_final(&mut self, plaintext: Vec<u8>) -> Result<(), Self::Error> {
//!         self.0.extend(plaintext);
//!         Ok(())
//!     }
//! }
//!
//! impl RecordSink<Aes256GcmTag> for Records {
//!     type Error = Infallible;
//!
//!     fn write_data(&mut self, record: DataRecord<Aes256GcmTag>) -> Result<(), Self::Error> {
//!         self.data.push(record);
//!         Ok(())
//!     }
//!
//!     fn write_final(&mut self, record: FinalRecord<Aes256GcmTag>) -> Result<(), Self::Error> {
//!         self.final_record = Some(record);
//!         Ok(())
//!     }
//! }
//!
//! let key_bytes = [0x42; 32];
//! // This fixed field must be distinct for every record stream encrypted under this key.
//! let fixed = *b"stream01";
//!
//! let mut sealer = RecordBuilder::new(Aes256Gcm::new(Aes256GcmKey::new(key_bytes)))
//!     .nonce_sequence(CounterNonceSequence::<Aes256GcmNonce>::new(fixed))
//!     .record_size(8)
//!     .context(b"document format v1")
//!     .build_sealer()?;
//!
//! // Fragment boundaries do not affect the resulting 8-byte data records.
//! let mut records = Records::default();
//! sealer.write_to(b"a very ", &mut records).unwrap();
//! sealer.write_to(b"large piece of text", &mut records).unwrap();
//! sealer.finish_to(&mut records).unwrap();
//!
//! let mut opener = RecordBuilder::new(Aes256Gcm::new(Aes256GcmKey::new(key_bytes)))
//!     .nonce_sequence(CounterNonceSequence::<Aes256GcmNonce>::new(fixed))
//!     .record_size(8)
//!     .context(b"document format v1")
//!     .build_opener()?;
//!
//! let mut recovered = Plaintext::default();
//! for record in &records.data {
//!     opener.open_data_to(record, &mut recovered).unwrap();
//! }
//! opener
//!     .open_final_to(records.final_record.as_ref().unwrap(), &mut recovered)
//!     .unwrap();
//! assert_eq!(recovered.0, b"a very large piece of text");
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! [`RecordSealer::write_to`] owns at most one completed record at a time; the sink chooses the
//! wire encoding and output destination. [`RecordSealer::write`] remains a convenience that
//! collects all records completed by one call and therefore allocates output proportional to that
//! call's input. [`RecordOpener::open_data_to`] similarly authenticates and delivers one bounded
//! plaintext chunk at a time; [`RecordOpener::open_data`] returns that chunk directly.

use alloc::vec::Vec;
use core::{convert::Infallible, fmt, marker::PhantomData};

use zeroize::Zeroize;

use super::{Aead, Sealed};
use crate::{CryptoError, Result};

const DOMAIN_SEPARATOR: &[u8] = b"rsl-crypto/aead-record/v1\0";
const DATA_RECORD: u8 = 0;
const FINAL_RECORD: u8 = 1;
const RECORD_METADATA_BYTES: usize = 8 + 8 + 8 + 1 + 8;

/// Construct a typed 96-bit AEAD nonce from its exact byte representation.
///
/// The trait is implemented by this crate's AES-GCM and ChaCha20-Poly1305 nonce types. It exists
/// so [`CounterNonceSequence`] can be shared by those algorithms; a protocol-specific nonce rule
/// can instead implement [`NonceSequence`] directly.
pub trait Nonce96: AsRef<[u8]> {
    /// Construct the algorithm's nonce type from twelve bytes.
    fn from_bytes(bytes: [u8; 12]) -> Self;
}

impl Nonce96 for super::gcm::Aes128GcmNonce {
    fn from_bytes(bytes: [u8; 12]) -> Self {
        Self::new(bytes)
    }
}

impl Nonce96 for super::gcm::Aes256GcmNonce {
    fn from_bytes(bytes: [u8; 12]) -> Self {
        Self::new(bytes)
    }
}

impl Nonce96 for super::chacha20poly1305::ChaCha20Poly1305Nonce {
    fn from_bytes(bytes: [u8; 12]) -> Self {
        Self::new(bytes)
    }
}

/// A deterministic, non-repeating nonce rule indexed by record number.
///
/// For all record numbers accepted under one key, implementations must return the same nonce for
/// repeated calls with the same number and distinct nonces for distinct numbers. Returning
/// [`CryptoError::CounterExhausted`] ends the record stream before a nonce could repeat.
///
/// The trait takes `&self` deliberately: sequence advancement belongs to [`RecordSealer`] and
/// [`RecordOpener`], so merely deriving parameters cannot silently consume nonce state.
pub trait NonceSequence<N> {
    /// Derive the nonce for `record_number` or reject an exhausted sequence.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::CounterExhausted`] when the sequence cannot represent another
    /// distinct nonce, or another public configuration error defined by the implementation.
    fn nonce(&self, record_number: u64) -> Result<N>;
}

/// RFC 5116 §3.2's 96-bit `Fixed || Counter` nonce formation with a 32-bit counter.
///
/// The eight-byte fixed field stays constant within one stream. It must be distinct from the
/// fixed field of every other encryptor using the same key. The final four bytes are the record
/// number encoded as an unsigned big-endian integer, so this sequence accepts record numbers
/// `0..=u32::MAX` and rejects the next value.
pub struct CounterNonceSequence<N> {
    fixed: [u8; 8],
    nonce: PhantomData<fn() -> N>,
}

impl<N> CounterNonceSequence<N> {
    /// Construct a nonce sequence from its stream-unique fixed field.
    #[must_use]
    pub const fn new(fixed: [u8; 8]) -> Self {
        Self {
            fixed,
            nonce: PhantomData,
        }
    }

    /// Borrow the fixed field.
    #[must_use]
    pub const fn fixed_field(&self) -> &[u8; 8] {
        &self.fixed
    }
}

impl<N> Copy for CounterNonceSequence<N> {}

impl<N> Clone for CounterNonceSequence<N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<N> fmt::Debug for CounterNonceSequence<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CounterNonceSequence")
            .field("fixed", &self.fixed)
            .finish_non_exhaustive()
    }
}

impl<N: Nonce96> NonceSequence<N> for CounterNonceSequence<N> {
    fn nonce(&self, record_number: u64) -> Result<N> {
        let counter = u32::try_from(record_number).map_err(|_| CryptoError::CounterExhausted)?;
        let mut bytes = [0_u8; 12];
        bytes[..8].copy_from_slice(&self.fixed);
        bytes[8..].copy_from_slice(&counter.to_be_bytes());
        Ok(N::from_bytes(bytes))
    }
}

/// The first builder stage, containing only the selected AEAD.
///
/// Call [`nonce_sequence`](Self::nonce_sequence) to advance to the next configuration stage.
/// This type has no `build_sealer` or `build_opener` method.
pub struct RecordBuilder<A> {
    algorithm: A,
}

impl<A> RecordBuilder<A> {
    /// Begin configuring record protection around `algorithm`.
    #[must_use]
    pub const fn new(algorithm: A) -> Self {
        Self { algorithm }
    }

    /// Supply the nonce rule and advance to the record-size stage.
    #[must_use]
    pub fn nonce_sequence<S>(self, nonces: S) -> RecordBuilderWithSequence<A, S> {
        RecordBuilderWithSequence {
            algorithm: self.algorithm,
            nonces,
        }
    }
}

/// The builder stage containing an AEAD and nonce sequence but no record size.
///
/// Call [`record_size`](Self::record_size) to advance to [`ReadyRecordBuilder`].
pub struct RecordBuilderWithSequence<A, S> {
    algorithm: A,
    nonces: S,
}

impl<A, S> RecordBuilderWithSequence<A, S> {
    /// Set the maximum plaintext bytes in one record and enable the `build_*` methods.
    ///
    /// Zero is rejected by those methods as [`CryptoError::InvalidLength`].
    #[must_use]
    pub fn record_size(self, bytes: usize) -> ReadyRecordBuilder<A, S> {
        ReadyRecordBuilder {
            algorithm: self.algorithm,
            nonces: self.nonces,
            record_size: bytes,
            context: Vec::new(),
        }
    }
}

/// A complete record builder that can produce either a sealer or an opener.
pub struct ReadyRecordBuilder<A, S> {
    algorithm: A,
    nonces: S,
    record_size: usize,
    context: Vec<u8>,
}

impl<A, S> ReadyRecordBuilder<A, S> {
    /// Bind public stream context into every record tag.
    ///
    /// Typical context identifies the format version, direction, and stream. It does not make
    /// nonce reuse safe: each key/fixed-field pair must still be unique.
    #[must_use]
    pub fn context(mut self, context: impl AsRef<[u8]>) -> Self {
        self.context.clear();
        self.context.extend_from_slice(context.as_ref());
        self
    }
}

impl<A, S> ReadyRecordBuilder<A, S>
where
    A: Aead,
    S: NonceSequence<A::Nonce>,
{
    /// Validate the configuration and build an incremental record sealer.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for a zero record size,
    /// [`CryptoError::OutputTooLong`] when configuration storage cannot be represented or
    /// reserved, or the nonce sequence's error for record zero.
    pub fn build_sealer(self) -> Result<RecordSealer<A, S>> {
        validate_configuration(self.record_size, &self.context, &self.nonces)?;
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(self.record_size)
            .map_err(|_| CryptoError::OutputTooLong)?;

        Ok(RecordSealer {
            algorithm: self.algorithm,
            nonces: self.nonces,
            record_size: self.record_size,
            context: self.context,
            pending,
            next_record: 0,
            failure: None,
        })
    }

    /// Validate the configuration and build a matching incremental record opener.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for a zero record size,
    /// [`CryptoError::OutputTooLong`] when a length is not representable, or the nonce sequence's
    /// error for record zero.
    pub fn build_opener(self) -> Result<RecordOpener<A, S>> {
        validate_configuration(self.record_size, &self.context, &self.nonces)?;
        Ok(RecordOpener {
            algorithm: self.algorithm,
            nonces: self.nonces,
            record_size: self.record_size,
            context: self.context,
            next_record: 0,
            failure: None,
        })
    }
}

/// One full, non-final protected record.
///
/// The record is a structured value, not a prescribed wire encoding. Protocol code chooses how
/// to encode its number, plaintext length, ciphertext, and detached tag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataRecord<Tag> {
    record_number: u64,
    plaintext_len: usize,
    ciphertext: Vec<u8>,
    tag: Tag,
}

impl<Tag> DataRecord<Tag> {
    /// Reconstruct a parsed data record before authenticated opening.
    #[must_use]
    pub fn from_parts(
        record_number: u64,
        plaintext_len: usize,
        ciphertext: Vec<u8>,
        tag: Tag,
    ) -> Self {
        Self {
            record_number,
            plaintext_len,
            ciphertext,
            tag,
        }
    }

    /// Return the expected record number.
    #[must_use]
    pub const fn record_number(&self) -> u64 {
        self.record_number
    }

    /// Return the authenticated plaintext length.
    #[must_use]
    pub const fn plaintext_len(&self) -> usize {
        self.plaintext_len
    }

    /// Borrow the ciphertext.
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// Borrow the detached authentication tag.
    #[must_use]
    pub const fn tag(&self) -> &Tag {
        &self.tag
    }

    /// Consume the record into fields for a caller-selected wire encoder.
    #[must_use]
    pub fn into_parts(self) -> (u64, usize, Vec<u8>, Tag) {
        (
            self.record_number,
            self.plaintext_len,
            self.ciphertext,
            self.tag,
        )
    }
}

/// The final protected record in a record stream.
///
/// A final record may contain the last partial plaintext or be empty when the stream ended on an
/// exact record boundary. A complete consuming format requires exactly one successfully opened
/// value of this type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalRecord<Tag> {
    record_number: u64,
    plaintext_len: usize,
    ciphertext: Vec<u8>,
    tag: Tag,
}

impl<Tag> FinalRecord<Tag> {
    /// Reconstruct a parsed final record before authenticated opening.
    #[must_use]
    pub fn from_parts(
        record_number: u64,
        plaintext_len: usize,
        ciphertext: Vec<u8>,
        tag: Tag,
    ) -> Self {
        Self {
            record_number,
            plaintext_len,
            ciphertext,
            tag,
        }
    }

    /// Return the expected record number.
    #[must_use]
    pub const fn record_number(&self) -> u64 {
        self.record_number
    }

    /// Return the authenticated plaintext length.
    #[must_use]
    pub const fn plaintext_len(&self) -> usize {
        self.plaintext_len
    }

    /// Borrow the ciphertext.
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// Borrow the detached authentication tag.
    #[must_use]
    pub const fn tag(&self) -> &Tag {
        &self.tag
    }

    /// Consume the record into fields for a caller-selected wire encoder.
    #[must_use]
    pub fn into_parts(self) -> (u64, usize, Vec<u8>, Tag) {
        (
            self.record_number,
            self.plaintext_len,
            self.ciphertext,
            self.tag,
        )
    }
}

/// A fallible destination for protected records.
///
/// Implementations choose storage, transport, and wire encoding. A successful method call means
/// the sink has accepted ownership of that record; this crate does not define whether acceptance
/// means buffering, durable storage, or delivery to a peer.
pub trait RecordSink<Tag> {
    /// The destination-specific output error.
    type Error;

    /// Accept one full, non-final record.
    ///
    /// # Errors
    ///
    /// Returns the sink's error when it cannot accept the record.
    fn write_data(&mut self, record: DataRecord<Tag>) -> core::result::Result<(), Self::Error>;

    /// Accept the one final record that terminates the stream.
    ///
    /// # Errors
    ///
    /// Returns the sink's error when it cannot accept the record.
    fn write_final(&mut self, record: FinalRecord<Tag>) -> core::result::Result<(), Self::Error>;
}

/// A cryptographic or destination error from streamed record output.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RecordWriteError<E> {
    /// Record protection failed.
    Crypto(CryptoError),
    /// The destination rejected a protected record.
    Sink(E),
}

impl<E> From<CryptoError> for RecordWriteError<E> {
    fn from(error: CryptoError) -> Self {
        Self::Crypto(error)
    }
}

impl<E: fmt::Display> fmt::Display for RecordWriteError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Crypto(error) => write!(formatter, "record protection failed: {error}"),
            Self::Sink(error) => write!(formatter, "record sink failed: {error}"),
        }
    }
}

impl<E> core::error::Error for RecordWriteError<E>
where
    E: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Crypto(error) => Some(error),
            Self::Sink(error) => Some(error),
        }
    }
}

/// A fallible destination for authenticated record plaintext.
///
/// The opener calls these methods only after authenticating the record and its metadata. A
/// successful call means the sink has accepted ownership of the plaintext; this crate does not
/// define whether acceptance means processing, buffering, durable storage, or delivery onward.
pub trait RecordPlaintextSink {
    /// The destination-specific output error.
    type Error;

    /// Accept the plaintext of one full, non-final record.
    ///
    /// # Errors
    ///
    /// Returns the sink's error when it cannot accept the authenticated plaintext.
    fn write_data(&mut self, plaintext: Vec<u8>) -> core::result::Result<(), Self::Error>;

    /// Accept the plaintext of the one final record that terminates the stream.
    ///
    /// # Errors
    ///
    /// Returns the sink's error when it cannot accept the authenticated plaintext.
    fn write_final(&mut self, plaintext: Vec<u8>) -> core::result::Result<(), Self::Error>;
}

/// A cryptographic or destination error from streamed record opening.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RecordOpenError<E> {
    /// Record authentication or opening failed.
    Crypto(CryptoError),
    /// The destination rejected authenticated plaintext.
    Sink(E),
}

impl<E> From<CryptoError> for RecordOpenError<E> {
    fn from(error: CryptoError) -> Self {
        Self::Crypto(error)
    }
}

impl<E: fmt::Display> fmt::Display for RecordOpenError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Crypto(error) => write!(formatter, "record opening failed: {error}"),
            Self::Sink(error) => write!(formatter, "plaintext sink failed: {error}"),
        }
    }
}

impl<E> core::error::Error for RecordOpenError<E>
where
    E: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Crypto(error) => Some(error),
            Self::Sink(error) => Some(error),
        }
    }
}

struct DataRecordCollector<Tag> {
    records: Vec<DataRecord<Tag>>,
}

impl<Tag> RecordSink<Tag> for DataRecordCollector<Tag> {
    type Error = Infallible;

    fn write_data(&mut self, record: DataRecord<Tag>) -> core::result::Result<(), Self::Error> {
        self.records.push(record);
        Ok(())
    }

    fn write_final(&mut self, _record: FinalRecord<Tag>) -> core::result::Result<(), Self::Error> {
        Ok(())
    }
}

struct PlaintextCollector {
    plaintext: Vec<u8>,
}

impl RecordPlaintextSink for PlaintextCollector {
    type Error = Infallible;

    fn write_data(&mut self, plaintext: Vec<u8>) -> core::result::Result<(), Self::Error> {
        self.plaintext = plaintext;
        Ok(())
    }

    fn write_final(&mut self, plaintext: Vec<u8>) -> core::result::Result<(), Self::Error> {
        self.plaintext = plaintext;
        Ok(())
    }
}

/// Incrementally split plaintext and seal bounded authenticated records.
///
/// Use [`write_to`](Self::write_to) to deliver each completed record directly to a [`RecordSink`],
/// then call [`finish_to`](Self::finish_to) exactly once. [`write`](Self::write) and
/// [`finish`](Self::finish) are collecting conveniences. The object owns an AEAD key and is
/// therefore non-`Clone`; its diagnostic formatting redacts the algorithm.
pub struct RecordSealer<A, S> {
    algorithm: A,
    nonces: S,
    record_size: usize,
    context: Vec<u8>,
    pending: Vec<u8>,
    next_record: u64,
    failure: Option<CryptoError>,
}

impl<A, S> RecordSealer<A, S>
where
    A: Aead,
    S: NonceSequence<A::Nonce>,
{
    /// Return the configured maximum plaintext bytes per record.
    #[must_use]
    pub const fn record_size(&self) -> usize {
        self.record_size
    }

    /// Return the number that will be assigned to the next record.
    #[must_use]
    pub const fn next_record_number(&self) -> u64 {
        self.next_record
    }

    /// Buffer arbitrary plaintext fragments and return every newly completed data record.
    ///
    /// Output depends only on the concatenated bytes, not on call boundaries. The returned vector
    /// contains `floor((previously_buffered + input.len()) / record_size)` records. Feed bounded
    /// input buffers when bounded output allocation matters.
    ///
    /// If sealing fails after any record in the call, all records produced by that call are
    /// discarded and the sealer retains the same error. Later calls return that error; drop the
    /// sealer and start a fresh stream with a fresh key/fixed-field pair.
    ///
    /// # Errors
    ///
    /// Returns a length, allocation, nonce-sequence, counter, or AEAD error.
    pub fn write(&mut self, input: impl AsRef<[u8]>) -> Result<Vec<DataRecord<A::Tag>>> {
        let input = input.as_ref();
        let record_count = self.preflight_write(input.len())?;

        let mut records = Vec::new();
        records
            .try_reserve_exact(record_count)
            .map_err(|_| CryptoError::OutputTooLong)?;
        let mut collector = DataRecordCollector { records };

        match self.write_to(input, &mut collector) {
            Ok(()) => Ok(collector.records),
            Err(RecordWriteError::Crypto(error)) => Err(error),
            Err(RecordWriteError::Sink(error)) => match error {},
        }
    }

    /// Buffer arbitrary plaintext fragments and send each completed record to `sink`.
    ///
    /// Output depends only on the concatenated bytes, not on call boundaries. At most one
    /// completed record is owned by this method at a time, in addition to the final partial
    /// plaintext retained by the sealer.
    ///
    /// The stream is invalidated before external sink code runs. If a sink call returns an error
    /// or panics, the sealer cannot safely retry that record; subsequent operations return
    /// [`CryptoError::StateInvalidated`]. Drop it and start a fresh stream with a fresh
    /// key/fixed-field pair.
    ///
    /// # Errors
    ///
    /// Returns [`RecordWriteError::Crypto`] for a length, allocation, nonce-sequence, counter, or
    /// AEAD failure. Returns [`RecordWriteError::Sink`] when `sink` rejects a completed record.
    pub fn write_to<R>(
        &mut self,
        input: impl AsRef<[u8]>,
        sink: &mut R,
    ) -> core::result::Result<(), RecordWriteError<R::Error>>
    where
        R: RecordSink<A::Tag>,
    {
        let input = input.as_ref();
        self.preflight_write(input.len())
            .map_err(RecordWriteError::Crypto)?;

        match self.write_to_inner(input, sink) {
            Ok(()) => Ok(()),
            Err(RecordWriteError::Crypto(error)) => {
                self.pending.zeroize();
                self.pending.clear();
                self.failure = Some(error);
                Err(RecordWriteError::Crypto(error))
            }
            Err(error @ RecordWriteError::Sink(_)) => Err(error),
        }
    }

    fn preflight_write(&self, input_len: usize) -> Result<usize> {
        if let Some(error) = self.failure {
            return Err(error);
        }

        let total = self
            .pending
            .len()
            .checked_add(input_len)
            .ok_or(CryptoError::MessageTooLong)?;
        let record_count = total / self.record_size;
        let record_count_u64 =
            u64::try_from(record_count).map_err(|_| CryptoError::CounterExhausted)?;
        let final_record_number = self
            .next_record
            .checked_add(record_count_u64)
            .ok_or(CryptoError::CounterExhausted)?;

        // Reserve one usable nonce for the final record before mutating stream state.
        let _ = self.nonces.nonce(final_record_number)?;
        Ok(record_count)
    }

    fn write_to_inner<R>(
        &mut self,
        mut input: &[u8],
        sink: &mut R,
    ) -> core::result::Result<(), RecordWriteError<R::Error>>
    where
        R: RecordSink<A::Tag>,
    {
        if !self.pending.is_empty() {
            let needed = self.record_size - self.pending.len();
            let copied = needed.min(input.len());
            self.pending.extend_from_slice(&input[..copied]);
            input = &input[copied..];

            if self.pending.len() == self.record_size {
                let sequence = self.next_record;
                let sealed = seal_payload(
                    &self.algorithm,
                    &self.nonces,
                    &self.context,
                    self.record_size,
                    sequence,
                    DATA_RECORD,
                    &self.pending,
                )
                .map_err(RecordWriteError::Crypto)?;
                let (ciphertext, tag) = sealed.into_parts();
                let record = DataRecord::from_parts(sequence, self.record_size, ciphertext, tag);
                self.next_record = sequence
                    .checked_add(1)
                    .expect("write preflight reserved the final record number");
                self.pending.zeroize();
                self.pending.clear();
                self.emit_data(record, sink)?;
            }
        }

        while input.len() >= self.record_size {
            let (plaintext, rest) = input.split_at(self.record_size);
            let sequence = self.next_record;
            let sealed = seal_payload(
                &self.algorithm,
                &self.nonces,
                &self.context,
                self.record_size,
                sequence,
                DATA_RECORD,
                plaintext,
            )
            .map_err(RecordWriteError::Crypto)?;
            let (ciphertext, tag) = sealed.into_parts();
            let record = DataRecord::from_parts(sequence, self.record_size, ciphertext, tag);
            self.next_record = sequence
                .checked_add(1)
                .expect("write preflight reserved the final record number");
            input = rest;
            self.emit_data(record, sink)?;
        }

        self.pending.extend_from_slice(input);
        Ok(())
    }

    fn emit_data<R>(
        &mut self,
        record: DataRecord<A::Tag>,
        sink: &mut R,
    ) -> core::result::Result<(), RecordWriteError<R::Error>>
    where
        R: RecordSink<A::Tag>,
    {
        // Set this before invoking external code so a caught panic cannot leave reusable state.
        self.failure = Some(CryptoError::StateInvalidated);
        match sink.write_data(record) {
            Ok(()) => {
                self.failure = None;
                Ok(())
            }
            Err(error) => Err(RecordWriteError::Sink(error)),
        }
    }

    /// Seal the final partial record and consume the sealer.
    ///
    /// An empty final record is emitted when no plaintext remains. Consuming `self` is the
    /// lifecycle guarantee: code that has called `finish` cannot call `write` again.
    ///
    /// # Errors
    ///
    /// Returns a retained write error or a nonce-sequence, length, allocation, or AEAD error.
    pub fn finish(self) -> Result<FinalRecord<A::Tag>> {
        if let Some(error) = self.failure {
            return Err(error);
        }

        let sequence = self.next_record;
        let plaintext_len = self.pending.len();
        let sealed = seal_payload(
            &self.algorithm,
            &self.nonces,
            &self.context,
            self.record_size,
            sequence,
            FINAL_RECORD,
            &self.pending,
        )?;
        let (ciphertext, tag) = sealed.into_parts();
        Ok(FinalRecord::from_parts(
            sequence,
            plaintext_len,
            ciphertext,
            tag,
        ))
    }

    /// Seal the final partial record, send it to `sink`, and consume the sealer.
    ///
    /// The sealer is consumed before external sink code runs, so a rejected final record cannot
    /// be retried with the same nonce. An empty final record is sent when no plaintext remains.
    ///
    /// # Errors
    ///
    /// Returns [`RecordWriteError::Crypto`] for a retained write error or a nonce-sequence,
    /// length, allocation, or AEAD error. Returns [`RecordWriteError::Sink`] when `sink` rejects
    /// the final record.
    pub fn finish_to<R>(self, sink: &mut R) -> core::result::Result<(), RecordWriteError<R::Error>>
    where
        R: RecordSink<A::Tag>,
    {
        let record = self.finish().map_err(RecordWriteError::Crypto)?;
        sink.write_final(record).map_err(RecordWriteError::Sink)
    }
}

impl<A, S> fmt::Debug for RecordSealer<A, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordSealer")
            .field("algorithm", &"[REDACTED]")
            .field("record_size", &self.record_size)
            .field("buffered", &self.pending.len())
            .field("next_record", &self.next_record)
            .field("failed", &self.failure.is_some())
            .finish_non_exhaustive()
    }
}

impl<A, S> Drop for RecordSealer<A, S> {
    fn drop(&mut self) {
        self.pending.zeroize();
        self.next_record.zeroize();
    }
}

/// Authenticate and open a sequence of [`DataRecord`] values followed by one [`FinalRecord`].
///
/// Use [`open_data_to`](Self::open_data_to) to deliver each authenticated plaintext chunk directly
/// to a [`RecordPlaintextSink`], then call [`open_final_to`](Self::open_final_to) exactly once.
/// [`open_data`](Self::open_data) and [`open_final`](Self::open_final) are collecting conveniences.
/// Final opening consumes the opener whether authentication or output succeeds or fails.
pub struct RecordOpener<A, S> {
    algorithm: A,
    nonces: S,
    record_size: usize,
    context: Vec<u8>,
    next_record: u64,
    failure: Option<CryptoError>,
}

impl<A, S> RecordOpener<A, S>
where
    A: Aead,
    S: NonceSequence<A::Nonce>,
{
    /// Return the configured full-record plaintext size.
    #[must_use]
    pub const fn record_size(&self) -> usize {
        self.record_size
    }

    /// Return the next record number required by this opener.
    #[must_use]
    pub const fn next_record_number(&self) -> u64 {
        self.next_record
    }

    /// Authenticate and open one full data record.
    ///
    /// Record number and full-record length are checked before cryptographic work. Plaintext is
    /// returned only after the AEAD tag and the authenticated metadata verify.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::AuthenticationFailed`] for wrong order, kind, length, context,
    /// ciphertext, or tag; [`CryptoError::StateInvalidated`] after an earlier plaintext-sink
    /// failure; otherwise returns a nonce-sequence or AEAD configuration error.
    pub fn open_data(&mut self, record: &DataRecord<A::Tag>) -> Result<Vec<u8>> {
        let mut collector = PlaintextCollector {
            plaintext: Vec::new(),
        };
        match self.open_data_to(record, &mut collector) {
            Ok(()) => Ok(collector.plaintext),
            Err(RecordOpenError::Crypto(error)) => Err(error),
            Err(RecordOpenError::Sink(error)) => match error {},
        }
    }

    /// Authenticate one full data record and send its plaintext to `sink`.
    ///
    /// Record number, full-record length, metadata, and tag are verified before `sink` receives
    /// plaintext. Authentication failures do not invoke the sink or advance the opener, allowing
    /// a caller to reject one invalid candidate and supply the required record.
    ///
    /// After authentication succeeds, the opener advances and invalidates itself before external
    /// sink code runs. If the sink returns an error or panics, the plaintext cannot safely be
    /// delivered again; subsequent operations return [`CryptoError::StateInvalidated`].
    ///
    /// # Errors
    ///
    /// Returns [`RecordOpenError::Crypto`] for wrong order, kind, length, context, ciphertext,
    /// tag, nonce-sequence, or AEAD configuration. Returns [`RecordOpenError::Sink`] when `sink`
    /// rejects authenticated plaintext.
    pub fn open_data_to<R>(
        &mut self,
        record: &DataRecord<A::Tag>,
        sink: &mut R,
    ) -> core::result::Result<(), RecordOpenError<R::Error>>
    where
        R: RecordPlaintextSink,
    {
        if let Some(error) = self.failure {
            return Err(RecordOpenError::Crypto(error));
        }

        if record.record_number != self.next_record || record.plaintext_len != self.record_size {
            return Err(RecordOpenError::Crypto(CryptoError::AuthenticationFailed));
        }

        let next = self
            .next_record
            .checked_add(1)
            .ok_or(RecordOpenError::Crypto(CryptoError::CounterExhausted))?;
        // A data record is accepted only if the sequence still has room for the final record.
        let _ = self.nonces.nonce(next).map_err(RecordOpenError::Crypto)?;

        let mut plaintext = open_payload(
            &self.algorithm,
            &self.nonces,
            &self.context,
            self.record_size,
            record.record_number,
            DATA_RECORD,
            record.plaintext_len,
            &record.ciphertext,
            &record.tag,
        )
        .map_err(RecordOpenError::Crypto)?;
        if plaintext.len() != record.plaintext_len {
            plaintext.zeroize();
            return Err(RecordOpenError::Crypto(CryptoError::AuthenticationFailed));
        }

        self.next_record = next;
        self.emit_plaintext(plaintext, sink)
    }

    fn emit_plaintext<R>(
        &mut self,
        plaintext: Vec<u8>,
        sink: &mut R,
    ) -> core::result::Result<(), RecordOpenError<R::Error>>
    where
        R: RecordPlaintextSink,
    {
        // Set this before invoking external code so a caught panic cannot redeliver plaintext.
        self.failure = Some(CryptoError::StateInvalidated);
        match sink.write_data(plaintext) {
            Ok(()) => {
                self.failure = None;
                Ok(())
            }
            Err(error) => Err(RecordOpenError::Sink(error)),
        }
    }

    /// Authenticate and open the final partial record, consuming the opener.
    ///
    /// The final plaintext length must be strictly smaller than the configured record size; an
    /// exact-boundary stream therefore ends with an authenticated empty record.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::AuthenticationFailed`] for wrong order, kind, length, context,
    /// ciphertext, or tag; [`CryptoError::StateInvalidated`] after an earlier plaintext-sink
    /// failure; otherwise returns a nonce-sequence or AEAD configuration error.
    pub fn open_final(self, record: &FinalRecord<A::Tag>) -> Result<Vec<u8>> {
        let mut collector = PlaintextCollector {
            plaintext: Vec::new(),
        };
        match self.open_final_to(record, &mut collector) {
            Ok(()) => Ok(collector.plaintext),
            Err(RecordOpenError::Crypto(error)) => Err(error),
            Err(RecordOpenError::Sink(error)) => match error {},
        }
    }

    /// Authenticate the final record, send its plaintext to `sink`, and consume the opener.
    ///
    /// The final plaintext length must be strictly smaller than the configured record size; an
    /// exact-boundary stream therefore sends an authenticated empty chunk. The opener is consumed
    /// before external sink code runs, so rejected final plaintext cannot be delivered twice.
    ///
    /// # Errors
    ///
    /// Returns [`RecordOpenError::Crypto`] for a retained sink failure, wrong order, kind, length,
    /// context, ciphertext, tag, nonce-sequence, or AEAD configuration. Returns
    /// [`RecordOpenError::Sink`] when `sink` rejects authenticated final plaintext.
    pub fn open_final_to<R>(
        self,
        record: &FinalRecord<A::Tag>,
        sink: &mut R,
    ) -> core::result::Result<(), RecordOpenError<R::Error>>
    where
        R: RecordPlaintextSink,
    {
        if let Some(error) = self.failure {
            return Err(RecordOpenError::Crypto(error));
        }

        if record.record_number != self.next_record || record.plaintext_len >= self.record_size {
            return Err(RecordOpenError::Crypto(CryptoError::AuthenticationFailed));
        }

        let mut plaintext = open_payload(
            &self.algorithm,
            &self.nonces,
            &self.context,
            self.record_size,
            record.record_number,
            FINAL_RECORD,
            record.plaintext_len,
            &record.ciphertext,
            &record.tag,
        )
        .map_err(RecordOpenError::Crypto)?;
        if plaintext.len() != record.plaintext_len {
            plaintext.zeroize();
            return Err(RecordOpenError::Crypto(CryptoError::AuthenticationFailed));
        }

        sink.write_final(plaintext).map_err(RecordOpenError::Sink)
    }
}

impl<A, S> fmt::Debug for RecordOpener<A, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordOpener")
            .field("algorithm", &"[REDACTED]")
            .field("record_size", &self.record_size)
            .field("next_record", &self.next_record)
            .field("failed", &self.failure.is_some())
            .finish_non_exhaustive()
    }
}

fn validate_configuration<N, S>(record_size: usize, context: &[u8], nonces: &S) -> Result<()>
where
    S: NonceSequence<N>,
{
    if record_size == 0 {
        return Err(CryptoError::InvalidLength {
            name: "AEAD record size",
            expected: 1,
            actual: 0,
        });
    }
    let _ = u64::try_from(record_size).map_err(|_| CryptoError::OutputTooLong)?;
    let _ = u64::try_from(context.len()).map_err(|_| CryptoError::OutputTooLong)?;
    let _ = nonces.nonce(0)?;
    Ok(())
}

fn seal_payload<A, S>(
    algorithm: &A,
    nonces: &S,
    context: &[u8],
    record_size: usize,
    record_number: u64,
    kind: u8,
    plaintext: &[u8],
) -> Result<Sealed<A::Tag>>
where
    A: Aead,
    S: NonceSequence<A::Nonce>,
{
    let nonce = nonces.nonce(record_number)?;
    let aad = record_aad(context, record_size, record_number, kind, plaintext.len())?;
    algorithm.seal(&nonce, &aad, plaintext)
}

#[allow(clippy::too_many_arguments)]
fn open_payload<A, S>(
    algorithm: &A,
    nonces: &S,
    context: &[u8],
    record_size: usize,
    record_number: u64,
    kind: u8,
    plaintext_len: usize,
    ciphertext: &[u8],
    tag: &A::Tag,
) -> Result<Vec<u8>>
where
    A: Aead,
    S: NonceSequence<A::Nonce>,
{
    let nonce = nonces.nonce(record_number)?;
    let aad = record_aad(context, record_size, record_number, kind, plaintext_len)?;
    algorithm.open(&nonce, &aad, ciphertext, tag)
}

fn record_aad(
    context: &[u8],
    record_size: usize,
    record_number: u64,
    kind: u8,
    plaintext_len: usize,
) -> Result<Vec<u8>> {
    let context_len = u64::try_from(context.len()).map_err(|_| CryptoError::OutputTooLong)?;
    let record_size = u64::try_from(record_size).map_err(|_| CryptoError::OutputTooLong)?;
    let plaintext_len = u64::try_from(plaintext_len).map_err(|_| CryptoError::MessageTooLong)?;
    let capacity = DOMAIN_SEPARATOR
        .len()
        .checked_add(RECORD_METADATA_BYTES)
        .and_then(|length| length.checked_add(context.len()))
        .ok_or(CryptoError::OutputTooLong)?;
    let mut aad = Vec::new();
    aad.try_reserve_exact(capacity)
        .map_err(|_| CryptoError::OutputTooLong)?;
    aad.extend_from_slice(DOMAIN_SEPARATOR);
    aad.extend_from_slice(&context_len.to_be_bytes());
    aad.extend_from_slice(context);
    aad.extend_from_slice(&record_size.to_be_bytes());
    aad.extend_from_slice(&record_number.to_be_bytes());
    aad.push(kind);
    aad.extend_from_slice(&plaintext_len.to_be_bytes());
    Ok(aad)
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::aead::chacha20poly1305::ChaCha20Poly1305Nonce;
    use crate::aead::gcm::{Aes128GcmNonce, Aes256GcmNonce};

    #[test]
    fn counter_nonce_is_fixed_field_then_big_endian_counter() {
        let sequence = CounterNonceSequence::<Aes256GcmNonce>::new(*b"fixed123");
        assert_eq!(
            sequence.nonce(0x0102_0304).unwrap().as_bytes(),
            b"fixed123\x01\x02\x03\x04"
        );
        assert_eq!(
            sequence.nonce(u64::from(u32::MAX) + 1),
            Err(CryptoError::CounterExhausted)
        );
    }

    #[test]
    fn all_builtin_aead_nonces_implement_nonce96() {
        let bytes = [0x42; 12];
        assert_eq!(Aes128GcmNonce::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(Aes256GcmNonce::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(ChaCha20Poly1305Nonce::from_bytes(bytes).as_bytes(), &bytes);
    }

    #[test]
    fn aad_encoding_has_fixed_width_boundaries() {
        let aad = record_aad(b"ctx", 0x0102, 0x0304, FINAL_RECORD, 0x0506).unwrap();
        let prefix = DOMAIN_SEPARATOR.len();
        assert_eq!(&aad[..prefix], DOMAIN_SEPARATOR);
        assert_eq!(&aad[prefix..prefix + 8], &3_u64.to_be_bytes());
        assert_eq!(&aad[prefix + 8..prefix + 11], b"ctx");
        assert_eq!(&aad[prefix + 11..prefix + 19], &0x0102_u64.to_be_bytes());
        assert_eq!(&aad[prefix + 19..prefix + 27], &0x0304_u64.to_be_bytes());
        assert_eq!(aad[prefix + 27], FINAL_RECORD);
        assert_eq!(&aad[prefix + 28..], &0x0506_u64.to_be_bytes());
    }
}
