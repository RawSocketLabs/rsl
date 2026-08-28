//! GCM's counter-mode confidentiality primitive.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §6.5, Algorithm 3][sp-800-38d] defines `GCTR_K(ICB, X)`. Empty input returns
//! empty output. Otherwise, step 4 uses the supplied initial counter block as `CB_1`, step 5
//! derives later blocks with `inc32`, step 6 XORs complete input blocks with `CIPH_K(CB_i)`, and
//! step 7 XORs the final partial block with only the leftmost required cipher-output bits.
//!
//! Because every RSL cryptographic input is byte-oriented, this layer supports the byte-aligned
//! subset of Algorithm 3: the final partial length may be any whole number of bytes from one
//! through fifteen. TLS and SSH payloads are byte strings, so this does not discard a required
//! protocol use case. Any future bit-level caller must add an explicit, separately tested boundary
//! instead of silently truncating bits here.
//!
//! This module does not construct `ICB`/`J_0`, enforce GCM invocation or input-length limits,
//! authenticate ciphertext, format GHASH input, or release plaintext. It only applies the
//! reversible GCTR transform in place. A later authenticated-decryption layer must verify the tag
//! before calling it on ciphertext whose resulting plaintext could become caller-visible.
//!
//! ## Representation and lifetime
//!
//! [`CounterBlock`] retains counter semantics until each block is copied into an [`Aes128Block`].
//! The AES block then owns and clears the counter or keystream bytes when dropped. The caller owns
//! the transformed slice and remains responsible for its lifetime and destruction.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the private SP 800-38D GCTR layer lands before authenticated GCM composition"
    )
)]

use super::counter::CounterBlock;
use crate::cipher::aes::aes128::{Aes128, Aes128Block};

/// Number of bytes transformed by one AES counter block.
const BLOCK_BYTES: usize = 16;

/// Apply SP 800-38D `GCTR_K(initial_counter, input_output)` in place.
///
/// Each chunk is one `X_i` on entry and the corresponding `Y_i` on return. The first chunk uses
/// the supplied counter unchanged; incrementing happens immediately before each later chunk. For
/// the final partial chunk, `zip` consumes only the leftmost cipher-output bytes required by
/// Algorithm 3 step 7.
pub(super) fn apply(cipher: &Aes128, mut counter: CounterBlock, input_output: &mut [u8]) {
    for (block_index, chunk) in input_output.chunks_mut(BLOCK_BYTES).enumerate() {
        if block_index != 0 {
            counter.increment();
        }

        let mut encrypted_counter = Aes128Block::new(*counter.as_block());
        cipher.encrypt_block(&mut encrypted_counter);

        for (data_byte, key_stream_byte) in
            chunk.iter_mut().zip(encrypted_counter.as_bytes().iter())
        {
            *data_byte ^= key_stream_byte;
        }
    }
}

#[cfg(test)]
mod unit {
    use super::{Aes128, Aes128Block, CounterBlock, apply};
    use crate::cipher::aes::aes128::Aes128Key;

    /// Construct NIST `AES_GCM.pdf`'s common GCM-AES128 example cipher and first counter block.
    fn nist_example_cipher_and_counter() -> (Aes128, CounterBlock) {
        let cipher = Aes128::new(Aes128Key::new([
            0xfe, 0xff, 0xe9, 0x92, 0x86, 0x65, 0x73, 0x1c, 0x6d, 0x6a, 0x8f, 0x94, 0x67, 0x30,
            0x83, 0x08,
        ]));
        let mut counter = CounterBlock::new([
            0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88, 0x00, 0x00,
            0x00, 0x01,
        ]);
        counter.increment();

        (cipher, counter)
    }

    /// Published complete-block evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 2.
    #[test]
    fn four_complete_blocks_reach_the_published_ciphertext() {
        let (cipher, counter) = nist_example_cipher_and_counter();
        let mut data = [
            0xd9, 0x31, 0x32, 0x25, 0xf8, 0x84, 0x06, 0xe5, 0xa5, 0x59, 0x09, 0xc5, 0xaf, 0xf5,
            0x26, 0x9a, 0x86, 0xa7, 0xa9, 0x53, 0x15, 0x34, 0xf7, 0xda, 0x2e, 0x4c, 0x30, 0x3d,
            0x8a, 0x31, 0x8a, 0x72, 0x1c, 0x3c, 0x0c, 0x95, 0x95, 0x68, 0x09, 0x53, 0x2f, 0xcf,
            0x0e, 0x24, 0x49, 0xa6, 0xb5, 0x25, 0xb1, 0x6a, 0xed, 0xf5, 0xaa, 0x0d, 0xe6, 0x57,
            0xba, 0x63, 0x7b, 0x39, 0x1a, 0xaf, 0xd2, 0x55,
        ];
        let expected = [
            0x42, 0x83, 0x1e, 0xc2, 0x21, 0x77, 0x74, 0x24, 0x4b, 0x72, 0x21, 0xb7, 0x84, 0xd0,
            0xd4, 0x9c, 0xe3, 0xaa, 0x21, 0x2f, 0x2c, 0x02, 0xa4, 0xe0, 0x35, 0xc1, 0x7e, 0x23,
            0x29, 0xac, 0xa1, 0x2e, 0x21, 0xd5, 0x14, 0xb2, 0x54, 0x66, 0x93, 0x1c, 0x7d, 0x8f,
            0x6a, 0x5a, 0xac, 0x84, 0xaa, 0x05, 0x1b, 0xa3, 0x0b, 0x39, 0x6a, 0x0a, 0xac, 0x97,
            0x3d, 0x58, 0xe0, 0x91, 0x47, 0x3f, 0x59, 0x85,
        ];

        apply(&cipher, counter, &mut data);

        assert_eq!(data, expected);
    }

    /// Published final-partial-block evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 5.
    #[test]
    fn partial_final_block_uses_only_the_published_leftmost_key_stream_bytes() {
        let (cipher, counter) = nist_example_cipher_and_counter();
        let mut data = [
            0xd9, 0x31, 0x32, 0x25, 0xf8, 0x84, 0x06, 0xe5, 0xa5, 0x59, 0x09, 0xc5, 0xaf, 0xf5,
            0x26, 0x9a, 0x86, 0xa7, 0xa9, 0x53, 0x15, 0x34, 0xf7, 0xda, 0x2e, 0x4c, 0x30, 0x3d,
            0x8a, 0x31, 0x8a, 0x72, 0x1c, 0x3c, 0x0c, 0x95, 0x95, 0x68, 0x09, 0x53, 0x2f, 0xcf,
            0x0e, 0x24, 0x49, 0xa6, 0xb5, 0x25, 0xb1, 0x6a, 0xed, 0xf5, 0xaa, 0x0d, 0xe6, 0x57,
            0xba, 0x63, 0x7b, 0x39,
        ];
        let expected = [
            0x42, 0x83, 0x1e, 0xc2, 0x21, 0x77, 0x74, 0x24, 0x4b, 0x72, 0x21, 0xb7, 0x84, 0xd0,
            0xd4, 0x9c, 0xe3, 0xaa, 0x21, 0x2f, 0x2c, 0x02, 0xa4, 0xe0, 0x35, 0xc1, 0x7e, 0x23,
            0x29, 0xac, 0xa1, 0x2e, 0x21, 0xd5, 0x14, 0xb2, 0x54, 0x66, 0x93, 0x1c, 0x7d, 0x8f,
            0x6a, 0x5a, 0xac, 0x84, 0xaa, 0x05, 0x1b, 0xa3, 0x0b, 0x39, 0x6a, 0x0a, 0xac, 0x97,
            0x3d, 0x58, 0xe0, 0x91,
        ];

        apply(&cipher, counter, &mut data);

        assert_eq!(data, expected);
    }

    /// Standard-derived symmetry evidence from Algorithm 3's XOR operation.
    #[test]
    fn applying_gctr_twice_with_the_same_initial_counter_recovers_input() {
        let plaintext = *b"partial GCTR input crosses two blocks";
        let (cipher, first_counter) = nist_example_cipher_and_counter();
        let (_, second_counter) = nist_example_cipher_and_counter();
        let mut data = plaintext;

        apply(&cipher, first_counter, &mut data);
        assert_ne!(data, plaintext);
        apply(&cipher, second_counter, &mut data);

        assert_eq!(data, plaintext);
    }

    /// Published empty-input rule from SP 800-38D §6.5 Algorithm 3 step 1.
    #[test]
    fn empty_input_returns_without_a_transformed_byte() {
        let (cipher, counter) = nist_example_cipher_and_counter();
        let mut empty = [];

        apply(&cipher, counter, &mut empty);

        assert!(empty.is_empty());
    }

    /// API-regression evidence: GCTR consumes AES blocks only as temporary key-stream owners.
    ///
    /// Keeping this type assertion close to GCTR catches accidental substitution of raw key or
    /// counter arrays at the cipher boundary.
    #[test]
    fn aes_block_type_remains_the_counter_encryption_boundary() {
        let (cipher, counter) = nist_example_cipher_and_counter();
        let mut block = Aes128Block::new(*counter.as_block());

        cipher.encrypt_block(&mut block);

        assert_eq!(block.as_bytes().len(), 16);
    }
}
