//! Which DMA buffer comes next, and its out-of-band record (`recv_dma_wait` in libusdr's
//! `pcie_uram_main.c`).
//!
//! One wait can report several filled buffers: the driver returns their count and one
//! 16-byte record per buffer, and the gateware fills the 32 buffers in order. The ring
//! hands them out one at a time, without another wait, until they run out.

use std::collections::VecDeque;

/// DMA buffers per stream: the gateware's count, which the driver requires.
pub(crate) const BUFFER_COUNT: u32 = 32;
/// [`BUFFER_COUNT`] as an index bound.
pub(crate) const BUFFERS: usize = BUFFER_COUNT as usize;

/// Most records one wait returns (the driver's 512-byte cap).
pub(crate) const MAX_RECORDS: usize = 32;

/// Filled buffers not yet handed out.
#[derive(Debug, Default)]
pub(crate) struct Ring {
    /// The next buffer to hand out.
    next: usize,

    /// Out-of-band records of the buffers still to hand out, oldest first.
    pending: VecDeque<[u64; 2]>,

    /// Buffers ready but not handed out; may exceed the records the driver returned.
    ready: usize,
}

impl Ring {
    /// Whether a wait is needed before the next buffer.
    pub(crate) const fn is_empty(&self) -> bool {
        self.ready == 0
    }

    /// Records a wait's result: `count` buffers ready, and the records the driver wrote. More
    /// than 32 cannot be ready; a count above that is taken as 32, so no buffer is lent twice.
    pub(crate) fn filled(&mut self, count: usize, records: &[[u64; 2]]) {
        self.ready = count.min(BUFFERS);
        self.pending.clear();
        self.pending.extend(records.iter().take(count));
    }

    /// The next buffer's index and record. A buffer the driver gave no record for gets
    /// zeros, as libusdr gives it.
    pub(crate) fn take(&mut self) -> Option<(usize, [u64; 2])> {
        if self.ready == 0 {
            return None;
        }
        let index = self.next;
        self.next = (self.next + 1) % BUFFERS;
        self.ready -= 1;
        Some((index, self.pending.pop_front().unwrap_or([0, 0])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_wait_hands_out_every_ready_buffer_in_order() {
        let mut ring = Ring::default();
        ring.filled(3, &[[1, 0], [2, 0], [3, 0]]);
        let taken: Vec<_> = std::iter::from_fn(|| ring.take()).collect();
        assert_eq!(taken, [(0, [1, 0]), (1, [2, 0]), (2, [3, 0])]);
        assert!(ring.is_empty(), "a wait is needed for the fourth");
    }

    #[test]
    fn the_index_wraps_after_32_buffers() {
        let mut ring = Ring::default();
        for _ in 0..BUFFERS {
            ring.filled(1, &[[0, 0]]);
            ring.take();
        }
        ring.filled(1, &[[7, 0]]);
        assert_eq!(ring.take(), Some((0, [7, 0])));
    }

    #[test]
    fn a_buffer_without_a_record_gets_zeros() {
        let mut ring = Ring::default();
        ring.filled(2, &[[5, 0]]);
        assert_eq!(ring.take(), Some((0, [5, 0])));
        assert_eq!(ring.take(), Some((1, [0, 0])));
    }
}
