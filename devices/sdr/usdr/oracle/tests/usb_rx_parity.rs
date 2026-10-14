//! Pinned C USB RX framing and lifecycle, over the same hardware-free frame boundary.
#![cfg(target_os = "linux")]

use std::collections::VecDeque;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use usdr::lowlevel::BusError;
// Pull the oracle archive and its native link directives into this integration binary.
use usdr_oracle as _;

/// Satisfies the production RX module's normal error import without copying that module.
mod lowlevel {
    pub(crate) use usdr::lowlevel::{BusError, usb_error};
}

#[path = "../../src/transport/usb/rx.rs"]
mod rx;

unsafe extern "C" {
    /// Runs the original pinned open, receive, release and close over a borrowed frame.
    fn rsl_oracle_usb_rx(
        block: u32,
        frame: *const u8,
        frame_size: u32,
        output: *mut u8,
        oob: *mut u64,
        events: *mut u32,
    ) -> i32;
}

/// Observable endpoint lifecycle, scripted only at the owned buffer boundary.
#[derive(Default)]
struct State {
    pending: VecDeque<Vec<u8>>,
    incoming: VecDeque<Option<Vec<u8>>>,
    capacities: Vec<usize>,
    submissions: usize,
    cancellations: usize,
}

/// Hardware-free endpoint with the same pending-buffer ownership as `InQueue`.
struct Queue(Arc<Mutex<State>>);

impl rx::RxQueue for Queue {
    type Buffer = Vec<u8>;

    fn data(buffer: &Vec<u8>) -> &[u8] {
        buffer
    }

    fn packet_size(&self) -> usize {
        512
    }

    fn alloc(&mut self, capacity: usize) -> Result<Vec<u8>, BusError> {
        self.0.lock().unwrap().capacities.push(capacity);
        Ok(Vec::new())
    }

    fn submit(&mut self, buffer: Vec<u8>) -> Result<(), BusError> {
        let mut state = self.0.lock().unwrap();
        state.pending.push_back(buffer);
        state.submissions += 1;
        Ok(())
    }

    fn wait(&mut self, _: Duration) -> Result<Option<Vec<u8>>, BusError> {
        let mut state = self.0.lock().unwrap();
        let Some(frame) = state.incoming.pop_front().flatten() else {
            return Ok(None);
        };
        state
            .pending
            .pop_front()
            .expect("an owned RX buffer is pending");
        Ok(Some(frame))
    }

    fn cancel(&mut self) -> Result<(), BusError> {
        let mut state = self.0.lock().unwrap();
        state.cancellations += 1;
        state.pending.clear();
        Ok(())
    }
}

/// Calls the C functions with valid, independently allocated output storage.
fn reference(block: u32, frame: Option<&[u8]>) -> (Vec<u8>, [u64; 2], [u32; 7]) {
    // The original C receiver has a process-wide, non-atomic static receive counter.
    static REFERENCE: Mutex<()> = Mutex::new(());
    let _guard = REFERENCE.lock().unwrap();
    let mut output = vec![0; block as usize];
    let mut oob = [0; 2];
    let mut events = [0; 7];
    let (pointer, size) = frame.map_or((ptr::null(), 0), |bytes| {
        (bytes.as_ptr(), u32::try_from(bytes.len()).unwrap())
    });
    // SAFETY: C borrows the immutable frame synchronously, copies at most `block`
    // bytes into output, writes two u64 OOB words and seven u32 event slots.
    let status = unsafe {
        rsl_oracle_usb_rx(
            block,
            pointer,
            size,
            output.as_mut_ptr(),
            oob.as_mut_ptr(),
            events.as_mut_ptr(),
        )
    };
    assert_eq!(status, 0, "the C reference lifecycle must complete");
    (output, oob, events)
}

#[test]
fn raw_samples_and_all_trailer_words_match_the_pinned_c_receiver() {
    for (block, oob) in [
        (112_u32, [0x8000_0000_0000_0000_u64, 0]),
        (496, [0x8000_0000_0000_0001, 0]),
        (4096, [0xc000_0000_0000_0003, 0xfedc_ba98_7654_3210]),
        (8192, [0xffff_ffff_00ff_ffff, 0x0123_4567_89ab_cdef]),
    ] {
        let payload: Vec<_> = (0..block).map(|index| index.to_le_bytes()[0]).collect();
        let mut frame = payload.clone();
        for word in oob {
            frame.extend_from_slice(&word.to_le_bytes());
        }
        let (expected_payload, expected_oob, events) = reference(block, Some(&frame));
        assert_eq!(events[4], 0, "C receive must succeed for block {block}");
        assert_eq!(events[0], 8, "C primes the Linux transfer ring");
        assert_eq!(
            &events[2..4],
            &[block, 0],
            "C exposes the raw block MTU on RX channel zero"
        );
        assert_eq!(
            &events[5..7],
            &[1, 1],
            "C releases the borrowed buffer and closes RX"
        );

        let state = Arc::new(Mutex::new(State::default()));
        state.lock().unwrap().incoming.push_back(Some(frame));
        let mut endpoint = None;
        rx::Rx::open_endpoint(&mut endpoint, block, || Ok(Queue(Arc::clone(&state)))).unwrap();
        let receiver = endpoint.as_mut().unwrap();
        receiver
            .receive(Duration::ZERO, &mut |bytes, metadata| {
                assert_eq!(
                    bytes, expected_payload,
                    "sample bytes must match C for block {block}"
                );
                assert_eq!(
                    metadata, expected_oob,
                    "OOB order must match C for block {block}"
                );
            })
            .unwrap();
        rx::Rx::close_endpoint(&mut endpoint).unwrap();
        let state = state.lock().unwrap();
        assert_eq!(state.submissions, events[0] as usize + events[5] as usize);
        assert_eq!(state.cancellations, events[6] as usize);
        assert!(
            state.pending.is_empty(),
            "close returns every queued transfer"
        );
        // Intentional divergence: C rounds allocation to 4 KiB; usbfs needs only
        // endpoint packet alignment plus room for a terminating ZLP. Neither capacity
        // changes frame bytes or OOB.
        assert_eq!(
            events[1] as usize,
            (block as usize + 16).div_ceil(4096) * 4096
        );
        assert_eq!(
            state.capacities,
            vec![((block as usize + 16) / 512 + 1) * 512; 8]
        );
    }
}

#[test]
fn timeout_lends_no_buffer_and_both_receivers_can_close_without_release() {
    let (_, _, events) = reference(4096, None);
    assert_eq!(
        i32::from_ne_bytes(events[4].to_ne_bytes()),
        -110,
        "Linux C reports ETIMEDOUT"
    );
    assert_eq!(&events[5..7], &[0, 1]);
    let state = Arc::new(Mutex::new(State::default()));
    let mut endpoint = None;
    rx::Rx::open_endpoint(&mut endpoint, 4096, || Ok(Queue(Arc::clone(&state)))).unwrap();
    let receiver = endpoint.as_mut().unwrap();
    assert!(matches!(
        receiver.receive(Duration::ZERO, &mut |_, _| panic!(
            "timeout must lend no samples"
        )),
        Err(BusError::Timeout)
    ));
    assert_eq!(state.lock().unwrap().submissions, events[0] as usize);
    rx::Rx::close_endpoint(&mut endpoint).unwrap();
    assert_eq!(state.lock().unwrap().cancellations, events[6] as usize);
}
