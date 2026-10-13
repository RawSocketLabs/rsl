//! The USB TX ring's ownership, bounds and failure handling over a fake endpoint. Header
//! bytes and status throttling against libusdr are in `oracle/tests/usb_tx_parity.rs`.

use std::collections::VecDeque;
use std::time::Duration;

use super::{HEADER, TRANSFERS, Tx, TxQueue, TxStatus, header};
use crate::lowlevel::BusError;

/// Records what the endpoint was given; completes transfers only when told to.
#[derive(Default)]
struct Endpoint {
    /// Buffers allocated.
    allocations: usize,
    /// Sent transfers, oldest first, with their lengths; each owns its buffer.
    in_flight: VecDeque<(Vec<u8>, usize)>,
    /// Transfers the device has finished, ready for `wait`; `Err` fails the oldest.
    completions: VecDeque<Result<(), BusError>>,
    /// Every frame sent, as the endpoint saw it.
    sent: Vec<Vec<u8>>,
    /// Cancellations, successful or not.
    cancellations: usize,
    /// Refuse the next cancellation.
    fail_cancel: bool,
}

impl TxQueue for Endpoint {
    type Buffer = Vec<u8>;

    fn bytes(buffer: &mut Vec<u8>) -> &mut [u8] {
        buffer
    }

    fn alloc(&mut self, capacity: usize) -> Result<Vec<u8>, BusError> {
        self.allocations += 1;
        Ok(vec![0; capacity])
    }

    fn submit(&mut self, buffer: Vec<u8>, len: usize) -> Result<(), BusError> {
        self.sent.push(buffer[..len].to_vec());
        self.in_flight.push_back((buffer, len));
        Ok(())
    }

    fn wait(&mut self, _: Duration) -> Result<Option<Vec<u8>>, BusError> {
        let Some(outcome) = self.completions.pop_front() else {
            return Ok(None);
        };
        let (buffer, _) = self.in_flight.pop_front().expect("a transfer to complete");
        outcome.map(|()| Some(buffer))
    }

    fn cancel(&mut self) -> Result<(), BusError> {
        self.cancellations += 1;
        if std::mem::take(&mut self.fail_cancel) {
            return Err(BusError::Timeout);
        }
        self.in_flight.clear();
        Ok(())
    }
}

/// An open endpoint of 1024-byte blocks of 4-byte samples.
fn open() -> Option<Tx<Endpoint>> {
    let mut slot = None;
    Tx::open_endpoint(&mut slot, 1024, 4, || Ok(Endpoint::default())).expect("opens");
    slot
}

/// Sends `len` bytes of `byte`, played as soon as they arrive.
fn send(tx: &mut Tx<Endpoint>, len: usize, byte: u8) -> Result<(), BusError> {
    tx.acquire(Duration::ZERO)?;
    tx.commit(None, Some([0; 4]), &mut |buffer, _| {
        buffer[..len].fill(byte);
        len
    })
}

#[test]
fn block_sizes_outside_libusdrs_usb_limits_are_refused_before_the_endpoint() {
    for (block, accepted) in [
        (0, false),
        (63, false),
        (64, true),
        (126_976, true),
        (126_977, false),
    ] {
        let mut slot = None;
        let mut acquired = false;
        let result = Tx::open_endpoint(&mut slot, block, 4, || {
            acquired = true;
            Ok(Endpoint::default())
        });
        assert_eq!(result.is_ok(), accepted, "block of {block} bytes");
        assert_eq!(acquired, accepted, "block of {block} bytes");
    }
}

#[test]
fn thirty_two_blocks_fly_before_a_send_waits_and_a_timeout_keeps_them_all() {
    let mut slot = open();
    let tx = slot.as_mut().expect("open");
    for n in 0..TRANSFERS {
        send(tx, 1024, u8::try_from(n).expect("small")).expect("a buffer is free");
    }
    assert_eq!(tx.queue.allocations, TRANSFERS);
    assert!(matches!(send(tx, 4, 0xee), Err(BusError::Timeout)));
    assert_eq!(tx.queue.in_flight.len(), TRANSFERS, "nothing cancelled");
    assert_eq!(tx.queue.sent.len(), TRANSFERS, "nothing sent on a timeout");

    tx.queue.completions.push_back(Ok(()));
    send(tx, 4, 0xee).expect("the oldest buffer is free again");
    assert_eq!(
        tx.queue.allocations, TRANSFERS,
        "the freed buffer is reused"
    );
    assert_eq!(tx.queue.sent.last().expect("sent")[HEADER..], [0xee; 4]);
}

#[test]
fn a_block_of_no_bytes_too_many_or_part_samples_sends_nothing_and_keeps_its_buffer() {
    let mut slot = open();
    let tx = slot.as_mut().expect("open");
    for len in [0, 3, 1025] {
        tx.acquire(Duration::ZERO).expect("a buffer is free");
        let refused = tx.commit(None, None, &mut |_, _| len);
        assert!(
            matches!(refused, Err(BusError::Unsupported(_))),
            "{len} bytes"
        );
    }
    assert_eq!(tx.queue.sent.len(), 0, "nothing sent");
    assert_eq!(tx.queue.allocations, 1, "one buffer, kept throughout");
    send(tx, 8, 2).expect("whole samples go");
    assert_eq!(
        tx.queue.sent[0].len(),
        HEADER + 8,
        "header and samples only"
    );
}

#[test]
fn a_failed_transfer_fails_the_stream_until_it_is_closed() {
    let mut slot = open();
    let tx = slot.as_mut().expect("open");
    for _ in 0..TRANSFERS {
        send(tx, 4, 0).expect("a buffer is free");
    }
    tx.queue.completions.push_back(Err(BusError::Disconnected));
    assert!(matches!(send(tx, 4, 0), Err(BusError::Disconnected)));
    assert!(matches!(send(tx, 4, 0), Err(BusError::Unsupported(_))));

    tx.queue.fail_cancel = true;
    assert!(Tx::close_endpoint(&mut slot).is_err());
    assert!(slot.is_some(), "an undrained endpoint stays owned");
    assert!(
        Tx::open_endpoint(&mut slot, 1024, 4, || Ok(Endpoint::default())).is_err(),
        "no second endpoint while one awaits cleanup"
    );
    Tx::close_endpoint(&mut slot).expect("drains on retry");
    assert!(slot.is_none());
}

#[test]
fn now_sets_every_time_bit_and_the_now_flag_in_libusdrs_header() {
    // 1024 bytes of 4-byte samples, time -1: hand-worked from `usb_uram_send_dma_commit`.
    let words: Vec<u32> = header(None, 1024, 4)
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().expect("four bytes")))
        .collect();
    assert_eq!(
        words,
        [
            0xffff_ffff,
            1 << 31 | 255 << 16 | 0xffff,
            1023 << 15 | 0x7fff,
            0
        ]
    );
}

#[test]
fn status_is_read_on_the_first_send_then_every_64th() {
    let mut status = TxStatus::default();
    let reads: Vec<usize> = (0..200).filter(|_| status.count_send()).collect();
    assert_eq!(reads, [0, 64, 128, 192]);
}
