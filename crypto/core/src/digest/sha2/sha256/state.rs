//! SHA-256 preprocessing, incremental state, and digest serialization.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 180-4 §5.1.1][fips-180-4] defines SHA-256 padding, including the appended `1` bit,
//! the zero bits needed to reach the length field, and the 64-bit representation of the original
//! message length. Section 5.2.1 defines division into 512-bit blocks, §5.3.3 supplies the initial
//! chaining value, and §6.2.1–§6.2.2 define preprocessing and concatenation of the final eight
//! words into the 256-bit digest.
//!
//! ## Layer boundary
//!
//! This layer owns the chaining value, a partial 64-byte block, and checked message-length
//! accounting. It calls `compression` only with complete blocks. Final padding construction and
//! digest serialization remain named operations with focused tests. Arbitrary input
//! fragmentation must not alter the resulting digest.
//!
//! [fips-180-4]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf

use core::fmt;
use zeroize::Zeroize;

use crate::{CryptoError, Result, digest::Digest};

use super::{compression::compress_block, constants::INITIAL_HASH_VALUE, schedule::BLOCK_LEN};

/// The SHA-256 digest length in bytes.
const DIGEST_LEN: usize = 32;

/// The index at which the final 64-bit message-length field begins in a block.
///
/// FIPS 180-4 §5.1.1 places this field in the last 64 bits of a 512-bit block. Eight bytes are
/// therefore reserved beginning at byte index 56.
const LENGTH_FIELD_START: usize = BLOCK_LEN - 8;

/// The largest byte-aligned message length representable by SHA-256's 64-bit bit-length field.
///
/// FIPS 180-4 §5.1.1 requires the message length in bits to be less than `2^64`. For a byte API,
/// dividing the largest representable bit count by eight yields this byte limit.
const MAX_MESSAGE_LEN_BYTES: u64 = u64::MAX / 8;

/// The one or two final blocks produced by SHA-256 padding.
#[derive(Clone, Debug, Eq, PartialEq)]
enum FinalBlocks {
    /// The partial message, padding marker, and length all fit in one block.
    One([u8; BLOCK_LEN]),
    /// The length field requires a second block.
    Two {
        /// The partial message followed by the padding marker and zeroes.
        first: [u8; BLOCK_LEN],
        /// Zeroes followed by the message length in the last eight bytes.
        second: [u8; BLOCK_LEN],
    },
}

/// Construct the final one or two padded blocks for a byte-aligned message.
///
/// **Standard mapping:** FIPS 180-4 §5.1.1 appends one `1` bit, the minimum number of zero bits
/// needed to leave 64 bits at the end of the final block, and the original message length as a
/// big-endian 64-bit integer. Because all public input is byte-aligned, the `1` bit is represented
/// by byte `0x80` followed by zero bits.
///
/// **Boundary:** `tail` contains only bytes not already compressed and is therefore shorter than
/// one block. `message_bit_length` describes the entire original message, not merely `tail`.
#[must_use]
fn build_final_blocks(tail: &[u8], message_bit_length: u64) -> FinalBlocks {
    debug_assert!(tail.len() < BLOCK_LEN);

    let mut first = [0_u8; BLOCK_LEN];
    first[..tail.len()].copy_from_slice(tail);
    first[tail.len()] = 0x80;

    let encoded_length = message_bit_length.to_be_bytes();

    if tail.len() < LENGTH_FIELD_START {
        first[LENGTH_FIELD_START..].copy_from_slice(&encoded_length);
        FinalBlocks::One(first)
    } else {
        let mut second = [0_u8; BLOCK_LEN];
        second[LENGTH_FIELD_START..].copy_from_slice(&encoded_length);
        FinalBlocks::Two { first, second }
    }
}

/// Serialize eight SHA-256 hash words as a 32-byte digest.
///
/// FIPS 180-4 §6.2.2 forms the final digest by concatenating `H_0` through `H_7`. Section 3.1's
/// big-endian convention means the most-significant byte of each word is emitted first.
#[must_use]
fn serialize_digest(hash_words: [u32; 8]) -> [u8; DIGEST_LEN] {
    let mut output = [0_u8; DIGEST_LEN];

    for (word_index, word) in hash_words.into_iter().enumerate() {
        let byte_index = word_index * 4;
        output[byte_index..byte_index + 4].copy_from_slice(&word.to_be_bytes());
    }

    output
}

/// A finalized 256-bit SHA-256 digest.
///
/// This distinct type prevents a SHA-256 digest from being accidentally interchanged with an
/// encryption key, authentication tag, nonce, or unrelated 32-byte value.
///
/// # Examples
///
/// ```
/// use rsl_crypto::digest::sha2::sha256::{Sha256, Sha256Digest};
///
/// let digest: Sha256Digest = Sha256::digest(b"typed output")?;
/// assert_eq!(digest.as_bytes().len(), Sha256Digest::LEN);
/// let bytes: [u8; 32] = digest.into_bytes();
/// assert_eq!(bytes.len(), 32);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "a SHA-256 digest should be compared, stored, or otherwise consumed"]
pub struct Sha256Digest([u8; DIGEST_LEN]);

impl Sha256Digest {
    /// The serialized digest length in bytes.
    pub const LEN: usize = DIGEST_LEN;

    /// Borrow the complete digest as a fixed-size byte array.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
        &self.0
    }

    /// Consume the digest and return its fixed-size byte array.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; DIGEST_LEN] {
        self.0
    }
}

impl AsRef<[u8]> for Sha256Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Sha256Digest(")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

/// Incremental SHA-256 message state.
///
/// Inputs may be supplied in any fragmentation. Complete 64-byte blocks are compressed
/// immediately; at most 63 trailing bytes remain buffered. Message length is checked before an
/// update mutates the state, so [`CryptoError::MessageTooLong`] never leaves a partially applied
/// update. Internal words and buffered bytes are zeroized on drop because higher constructions,
/// such as HMAC, can place secret-derived values in an otherwise general-purpose digest state.
///
/// # Examples
///
/// ```
/// use rsl_crypto::digest::sha2::sha256::Sha256;
///
/// let mut state = Sha256::new();
/// state.update(b"first fragment")?;
/// state.update(b" and second")?;
/// assert_eq!(state.finalize(), Sha256::digest(b"first fragment and second")?);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone)]
pub struct Sha256 {
    /// The eight hash words carried between compressed blocks.
    chaining_value: [u32; 8],
    /// Bytes received but not yet forming a complete block.
    buffer: [u8; BLOCK_LEN],
    /// Number of meaningful bytes currently stored in `buffer`.
    buffer_len: usize,
    /// Total original message length accepted so far, in bytes.
    message_len_bytes: u64,
}

impl Sha256 {
    /// Construct a SHA-256 state using the initial words from FIPS 180-4 §5.3.3.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            chaining_value: INITIAL_HASH_VALUE,
            buffer: [0; BLOCK_LEN],
            buffer_len: 0,
            message_len_bytes: 0,
        }
    }

    /// Incorporate more message bytes.
    ///
    /// Any type exposing an existing byte slice through [`AsRef<[u8]>`] is accepted without an
    /// allocation. The implementation borrows the bytes only for this call. Text is hashed as
    /// its exact UTF-8 byte representation. See [`Sha256`] for an incremental example.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] before changing the state if the resulting message
    /// length cannot be represented by SHA-256's 64-bit bit-length field.
    pub fn update(&mut self, input: impl AsRef<[u8]>) -> Result<()> {
        self.update_bytes(input.as_ref())
    }

    /// Apply SHA-256 padding and return the finalized digest.
    ///
    /// Excessive message length is rejected during [`update`](Self::update), so every valid state
    /// can be finalized without another failure path. See [`Sha256`] for a complete example.
    pub fn finalize(mut self) -> Sha256Digest {
        let message_bit_length = self.message_len_bytes * 8;
        let final_blocks = build_final_blocks(&self.buffer[..self.buffer_len], message_bit_length);

        match final_blocks {
            FinalBlocks::One(block) => self.compress(&block),
            FinalBlocks::Two { first, second } => {
                self.compress(&first);
                self.compress(&second);
            }
        }

        Sha256Digest(serialize_digest(self.chaining_value))
    }

    /// Digest one complete byte string.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::digest::sha2::sha256::Sha256;
    ///
    /// let from_str = Sha256::digest("same bytes")?;
    /// let from_slice = Sha256::digest(b"same bytes")?;
    /// assert_eq!(from_str, from_slice);
    /// # Ok::<(), rsl_crypto::CryptoError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if `input` exceeds SHA-256's length limit.
    pub fn digest(input: impl AsRef<[u8]>) -> Result<Sha256Digest> {
        let mut state = Self::new();
        state.update(input)?;
        Ok(state.finalize())
    }

    /// Validate length, buffer partial input, and compress every complete block.
    fn update_bytes(&mut self, input: &[u8]) -> Result<()> {
        let input_len = u64::try_from(input.len()).map_err(|_| CryptoError::MessageTooLong)?;
        let message_len_bytes = self
            .message_len_bytes
            .checked_add(input_len)
            .filter(|length| *length <= MAX_MESSAGE_LEN_BYTES)
            .ok_or(CryptoError::MessageTooLong)?;

        let mut remaining = input;

        if self.buffer_len != 0 {
            let available = BLOCK_LEN - self.buffer_len;
            let copied = available.min(remaining.len());
            let buffer_end = self.buffer_len + copied;

            self.buffer[self.buffer_len..buffer_end].copy_from_slice(&remaining[..copied]);
            self.buffer_len = buffer_end;
            remaining = &remaining[copied..];

            if self.buffer_len == BLOCK_LEN {
                let block = self.buffer;
                self.compress(&block);
                self.buffer = [0; BLOCK_LEN];
                self.buffer_len = 0;
            } else {
                self.message_len_bytes = message_len_bytes;
                return Ok(());
            }
        }

        while remaining.len() >= BLOCK_LEN {
            let (block_bytes, rest) = remaining.split_at(BLOCK_LEN);
            let block = <&[u8; BLOCK_LEN]>::try_from(block_bytes)
                .expect("split_at(BLOCK_LEN) returns one complete SHA-256 block");
            self.compress(block);
            remaining = rest;
        }

        self.buffer[..remaining.len()].copy_from_slice(remaining);
        self.buffer_len = remaining.len();
        self.message_len_bytes = message_len_bytes;

        Ok(())
    }

    /// Compress one complete block and replace the chaining value.
    fn compress(&mut self, block: &[u8; BLOCK_LEN]) {
        self.chaining_value = compress_block(self.chaining_value, block);
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sha256 {
    fn drop(&mut self) {
        // A digest state can contain message bytes and secret-derived chaining values when used
        // inside constructions such as HMAC. Zeroizing every state is the conservative behavior;
        // callers hashing only public data pay no semantic cost.
        self.chaining_value.zeroize();
        self.buffer.zeroize();
        self.buffer_len.zeroize();
        self.message_len_bytes.zeroize();
    }
}

impl Digest for Sha256 {
    type Output = Sha256Digest;

    const BLOCK_LEN: usize = super::schedule::BLOCK_LEN;
    const OUTPUT_LEN: usize = DIGEST_LEN;

    fn new() -> Self {
        Self::new()
    }

    fn update(&mut self, input: &[u8]) -> Result<()> {
        self.update_bytes(input)
    }

    fn finalize(self) -> Self::Output {
        self.finalize()
    }
}

#[cfg(test)]
mod unit {
    use super::{
        BLOCK_LEN, FinalBlocks, LENGTH_FIELD_START, MAX_MESSAGE_LEN_BYTES, Sha256, Sha256Digest,
        build_final_blocks, serialize_digest,
    };
    use crate::CryptoError;

    /// Standard-derived padding evidence from FIPS 180-4 §5.1.1.
    #[test]
    fn empty_message_padding_is_one_block() {
        let FinalBlocks::One(block) = build_final_blocks(&[], 0) else {
            panic!("an empty SHA-256 message requires one final block");
        };

        assert_eq!(block[0], 0x80);
        assert!(block[1..].iter().all(|byte| *byte == 0));
    }

    /// Standard-derived boundary evidence from FIPS 180-4 §5.1.1.
    #[test]
    fn fifty_five_tail_bytes_leave_room_for_one_block_length_field() {
        let tail = [0x61; LENGTH_FIELD_START - 1];
        let bit_length = u64::try_from(tail.len()).expect("test tail length fits u64") * 8;
        let FinalBlocks::One(block) = build_final_blocks(&tail, bit_length) else {
            panic!("55 tail bytes must fit in one final SHA-256 block");
        };

        assert_eq!(&block[..tail.len()], &tail);
        assert_eq!(block[tail.len()], 0x80);
        assert_eq!(&block[LENGTH_FIELD_START..], &bit_length.to_be_bytes());
    }

    /// Standard-derived boundary evidence from FIPS 180-4 §5.1.1.
    #[test]
    fn fifty_six_tail_bytes_require_a_second_block_for_the_length() {
        let tail = [0x61; LENGTH_FIELD_START];
        let bit_length = u64::try_from(tail.len()).expect("test tail length fits u64") * 8;
        let FinalBlocks::Two { first, second } = build_final_blocks(&tail, bit_length) else {
            panic!("56 tail bytes must require two final SHA-256 blocks");
        };

        assert_eq!(&first[..tail.len()], &tail);
        assert_eq!(first[tail.len()], 0x80);
        assert!(first[tail.len() + 1..].iter().all(|byte| *byte == 0));
        assert!(second[..LENGTH_FIELD_START].iter().all(|byte| *byte == 0));
        assert_eq!(&second[LENGTH_FIELD_START..], &bit_length.to_be_bytes());
    }

    /// Standard-derived serialization evidence from FIPS 180-4 §3.1 and §6.2.2.
    #[test]
    fn digest_serialization_preserves_word_order_and_big_endian_bytes() {
        let words = [
            0x0001_0203,
            0x0405_0607,
            0x0809_0a0b,
            0x0c0d_0e0f,
            0x1011_1213,
            0x1415_1617,
            0x1819_1a1b,
            0x1c1d_1e1f,
        ];
        let expected = core::array::from_fn(|index| {
            u8::try_from(index).expect("every SHA-256 digest byte index fits u8")
        });

        assert_eq!(serialize_digest(words), expected);
    }

    /// Published known-answer evidence from NIST's SHA-256 one-block `abc` sample.
    #[test]
    fn hashes_abc() {
        let digest = Sha256::digest("abc").expect("three bytes are within SHA-256 limits");

        assert_eq!(
            digest.into_bytes(),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
    }

    /// Published known-answer evidence from NIST's SHA-256 two-block sample.
    #[test]
    fn hashes_nists_two_block_message() {
        let message = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let digest = Sha256::digest(message).expect("the NIST sample is within SHA-256 limits");

        assert_eq!(
            digest.into_bytes(),
            [
                0x24, 0x8d, 0x6a, 0x61, 0xd2, 0x06, 0x38, 0xb8, 0xe5, 0xc0, 0x26, 0x93, 0x0c, 0x3e,
                0x60, 0x39, 0xa3, 0x3c, 0xe4, 0x59, 0x64, 0xff, 0x21, 0x67, 0xf6, 0xec, 0xed, 0xd4,
                0x19, 0xdb, 0x06, 0xc1,
            ]
        );
    }

    /// Regression evidence that input fragmentation does not affect the message represented.
    #[test]
    fn streaming_one_byte_at_a_time_matches_one_shot_input() {
        let message = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let expected = Sha256::digest(message).expect("the test message is within SHA-256 limits");
        let mut state = Sha256::new();

        for byte in message {
            state
                .update([*byte])
                .expect("the test message is within SHA-256 limits");
        }

        assert_eq!(state.finalize(), expected);
    }

    /// Boundary evidence that an excessive update is rejected before state mutation.
    #[test]
    fn message_length_exhaustion_does_not_partially_apply_an_update() {
        let mut state = Sha256::new();
        state.message_len_bytes = MAX_MESSAGE_LEN_BYTES;
        let before = state.clone();

        assert_eq!(state.update([0]), Err(CryptoError::MessageTooLong));
        assert_eq!(state.chaining_value, before.chaining_value);
        assert_eq!(state.buffer, before.buffer);
        assert_eq!(state.buffer_len, before.buffer_len);
        assert_eq!(state.message_len_bytes, before.message_len_bytes);
    }

    /// Regression evidence for the digest newtype's byte access and debug output.
    #[test]
    fn digest_value_exposes_exact_bytes_and_names_itself_in_debug_output() {
        use alloc::format;

        let bytes = [0xabu8; Sha256Digest::LEN];
        let digest = Sha256Digest(bytes);

        assert_eq!(digest.as_bytes(), &bytes);
        assert_eq!(digest.as_ref(), bytes.as_slice());
        assert_eq!(
            format!("{digest:?}"),
            "Sha256Digest(abababababababababababababababababababababababababababababababab)"
        );
    }

    /// Regression evidence that the buffer invariant leaves at most one partial block.
    #[test]
    fn complete_blocks_are_compressed_and_only_the_tail_remains_buffered() {
        let input = [0x5a; BLOCK_LEN * 2 + 7];
        let mut state = Sha256::new();

        state
            .update(input)
            .expect("the test input is within SHA-256 limits");

        assert_eq!(state.buffer_len, 7);
        assert_eq!(&state.buffer[..7], &[0x5a; 7]);
        assert_eq!(
            state.message_len_bytes,
            u64::try_from(BLOCK_LEN * 2 + 7).expect("test input length fits u64")
        );
    }
}
