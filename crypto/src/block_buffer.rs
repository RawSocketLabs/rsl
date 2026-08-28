//! Internal fixed-size block buffering for incremental primitive implementations.
//!
//! This is deliberately not a public caller API. Digest and MAC implementations use it to
//! preserve their standards-defined block boundaries across arbitrary `update` fragmentation;
//! callers use those algorithms' ordinary `update` methods.

use core::fmt;

use zeroize::Zeroize;

/// A zeroizing buffer that forwards complete blocks and retains one partial block.
pub(crate) struct BlockBuffer<const BLOCK_LEN: usize> {
    bytes: [u8; BLOCK_LEN],
    buffered: usize,
}

impl<const BLOCK_LEN: usize> BlockBuffer<BLOCK_LEN> {
    /// Construct an empty block buffer.
    ///
    /// # Panics
    ///
    /// Panics when `BLOCK_LEN` is zero.
    pub(crate) const fn new() -> Self {
        assert!(BLOCK_LEN != 0, "block length must be nonzero");
        Self {
            bytes: [0; BLOCK_LEN],
            buffered: 0,
        }
    }

    /// Forward every complete block formed by `input`, retaining the final partial block.
    pub(crate) fn push(
        &mut self,
        input: impl AsRef<[u8]>,
        mut process: impl FnMut(&[u8; BLOCK_LEN]),
    ) {
        let mut input = input.as_ref();

        if self.buffered != 0 {
            let copied = (BLOCK_LEN - self.buffered).min(input.len());
            let end = self.buffered + copied;
            self.bytes[self.buffered..end].copy_from_slice(&input[..copied]);
            self.buffered = end;
            input = &input[copied..];

            if self.buffered != BLOCK_LEN {
                return;
            }

            process(&self.bytes);
            self.bytes.zeroize();
            self.buffered = 0;
        }

        let mut chunks = input.chunks_exact(BLOCK_LEN);
        for chunk in &mut chunks {
            let block = <&[u8; BLOCK_LEN]>::try_from(chunk)
                .expect("chunks_exact yields one complete fixed-size block");
            process(block);
        }

        let remainder = chunks.remainder();
        self.bytes[..remainder.len()].copy_from_slice(remainder);
        self.buffered = remainder.len();
    }

    /// Borrow the bytes that do not yet form one complete block.
    pub(crate) fn remainder(&self) -> &[u8] {
        &self.bytes[..self.buffered]
    }

    /// Whether no partial block is retained.
    pub(crate) const fn is_empty(&self) -> bool {
        self.buffered == 0
    }

    /// Discard and zeroize the retained partial block.
    pub(crate) fn clear(&mut self) {
        self.bytes.zeroize();
        self.buffered = 0;
    }
}

impl<const BLOCK_LEN: usize> fmt::Debug for BlockBuffer<BLOCK_LEN> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockBuffer")
            .field("block_len", &BLOCK_LEN)
            .field("buffered", &self.buffered)
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

impl<const BLOCK_LEN: usize> Drop for BlockBuffer<BLOCK_LEN> {
    fn drop(&mut self) {
        self.bytes.zeroize();
        self.buffered.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn fragmentation_preserves_complete_blocks_and_tail() {
        let input = b"the same byte stream has the same blocks";
        let expected_blocks: Vec<[u8; 7]> = input
            .chunks_exact(7)
            .map(|block| block.try_into().unwrap())
            .collect();
        let expected_tail = input.chunks_exact(7).remainder();

        for split in 0..=input.len() {
            let mut buffer = BlockBuffer::<7>::new();
            let mut blocks = Vec::new();
            buffer.push(&input[..split], |block| blocks.push(*block));
            buffer.push(&input[split..], |block| blocks.push(*block));

            assert_eq!(blocks, expected_blocks, "split {split}");
            assert_eq!(buffer.remainder(), expected_tail, "split {split}");
        }
    }

    #[test]
    fn clear_discards_the_partial_block() {
        let mut buffer = BlockBuffer::<8>::new();
        buffer.push(b"secret", |_| panic!("six bytes cannot complete a block"));
        assert!(!buffer.is_empty());

        buffer.clear();

        assert!(buffer.is_empty());
        assert_eq!(buffer.remainder(), b"");
    }
}
