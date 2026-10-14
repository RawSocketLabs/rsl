//! Differential GHASH evidence against `RustCrypto` `ghash` 0.6.0.
//!
//! This module is compiled only for tests. The independent implementation is a development
//! dependency and never participates in the readable reference path.

use ghash::{Block, GHash as ReferenceGhash, Key, universal_hash::UniversalHash};

use super::state::{Ghash, HashSubkey};

/// Differential evidence over varied hash subkeys, block counts, and block contents.
///
/// Each of the 32 cases uses between one and eight complete input blocks. The deterministic byte
/// formulas are local test generation, not published vectors. Agreement supplements the NIST
/// known answer and layer tests; it is not a standards authority or production-validation claim.
#[test]
fn complete_block_recurrence_matches_rustcrypto() {
    for case_index in 0_u8..32 {
        let key_bytes = core::array::from_fn(|byte_index| {
            let byte_index =
                u8::try_from(byte_index).expect("every GHASH subkey byte index fits in u8");
            case_index
                .wrapping_mul(0x3d)
                .wrapping_add(byte_index.wrapping_mul(0x71))
                .wrapping_add(0x29)
        });
        let input_blocks: [[u8; 16]; 8] = core::array::from_fn(|block_index| {
            let block_index =
                u8::try_from(block_index).expect("every GHASH test block index fits in u8");
            core::array::from_fn(|byte_index| {
                let byte_index =
                    u8::try_from(byte_index).expect("every GHASH block byte index fits in u8");
                case_index
                    .wrapping_mul(0x53)
                    .wrapping_add(block_index.wrapping_mul(0x97))
                    .wrapping_add(byte_index.wrapping_mul(0x2f))
                    .wrapping_add(0x0b)
            })
        });
        let block_count = usize::from(case_index % 8) + 1;

        let mut ours = Ghash::new(HashSubkey::new(key_bytes));
        for block in &input_blocks[..block_count] {
            ours.update_block(block);
        }

        let reference_key = Key::from(key_bytes);
        let mut reference = ReferenceGhash::new(&reference_key);
        let reference_blocks: [Block; 8] = input_blocks.map(Block::from);
        reference.update(&reference_blocks[..block_count]);
        let reference_result = reference.finalize();

        assert_eq!(
            ours.finalize().as_block().as_slice(),
            &reference_result[..],
            "case {case_index}, block count {block_count}"
        );
    }
}
