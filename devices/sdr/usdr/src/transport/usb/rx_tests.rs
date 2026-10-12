//! Observable queue lifecycle, including faults that must not leak into a restarted stream.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use super::*;

/// Owned buffer with a stable identity, independent of its received length.
struct TestBuffer {
    id: usize,
    bytes: Vec<u8>,
}

/// The endpoint's pending buffers, scripted completions and fault injection.
#[derive(Default)]
struct State {
    pending: VecDeque<TestBuffer>,
    incoming: VecDeque<Result<Option<Vec<u8>>, BusError>>,
    capacities: Vec<usize>,
    submissions: Vec<usize>,
    cancels: usize,
    fail_alloc: Option<usize>,
    fail_submit: Option<usize>,
    fail_cancel: bool,
}

/// Fake only at the owned transfer boundary; production RX owns its real lifecycle.
struct Queue {
    state: Arc<Mutex<State>>,
    packet: usize,
}

impl RxQueue for Queue {
    type Buffer = TestBuffer;

    fn data(buffer: &TestBuffer) -> &[u8] {
        &buffer.bytes
    }

    fn packet_size(&self) -> usize {
        self.packet
    }

    fn alloc(&mut self, capacity: usize) -> Result<TestBuffer, BusError> {
        let mut state = self.state.lock().unwrap();
        let id = state.capacities.len();
        if state.fail_alloc == Some(id) {
            return Err(BusError::Disconnected);
        }
        state.capacities.push(capacity);
        Ok(TestBuffer {
            id,
            bytes: Vec::new(),
        })
    }

    fn submit(&mut self, buffer: TestBuffer) -> Result<(), BusError> {
        let mut state = self.state.lock().unwrap();
        if state.fail_submit == Some(state.submissions.len()) {
            return Err(BusError::Disconnected);
        }
        state.submissions.push(buffer.id);
        state.pending.push_back(buffer);
        Ok(())
    }

    fn wait(&mut self, _: Duration) -> Result<Option<TestBuffer>, BusError> {
        let mut state = self.state.lock().unwrap();
        let Some(bytes) = state.incoming.pop_front().unwrap_or(Ok(None))? else {
            return Ok(None);
        };
        let mut buffer = state.pending.pop_front().expect("a transfer is pending");
        buffer.bytes = bytes;
        Ok(Some(buffer))
    }

    fn cancel(&mut self) -> Result<(), BusError> {
        let mut state = self.state.lock().unwrap();
        state.cancels += 1;
        if state.fail_cancel {
            return Err(BusError::Timeout);
        }
        state.pending.clear();
        state.incoming.clear();
        Ok(())
    }
}

/// An endpoint with observed transfer ownership.
fn endpoint(packet: usize) -> (Rx<Queue>, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State::default()));
    (
        Rx::new(Queue {
            state: Arc::clone(&state),
            packet,
        }),
        state,
    )
}

/// C's exact trailer order: low-word loss, high-word burst mask, then TX statistics.
fn frame() -> Vec<u8> {
    let mut bytes = vec![0xa5; 4096];
    bytes.extend(0x8765_4321_0000_0003_u64.to_le_bytes());
    bytes.extend(0xfedc_ba98_7654_3210_u64.to_le_bytes());
    bytes
}

#[test]
fn timeout_preserves_the_same_transfer_then_receives_and_reuses_it() {
    let (mut rx, state) = endpoint(512);
    rx.open(4096).unwrap();
    {
        let mut state = state.lock().unwrap();
        assert_eq!(state.capacities, vec![4608; 8]);
        state.incoming.extend([Ok(None), Ok(Some(frame()))]);
    }
    assert!(matches!(
        rx.receive(Duration::ZERO, &mut |_, _| panic!("no data on timeout")),
        Err(BusError::Timeout)
    ));
    assert_eq!(state.lock().unwrap().pending.len(), 8);
    rx.receive(Duration::ZERO, &mut |payload, oob| {
        assert_eq!(payload, vec![0xa5; 4096]);
        assert_eq!(oob, [0x8765_4321_0000_0003, 0xfedc_ba98_7654_3210]);
    })
    .unwrap();
    let state = state.lock().unwrap();
    assert_eq!(state.submissions, [0, 1, 2, 3, 4, 5, 6, 7, 0]);
    assert_eq!(state.pending.len(), 8);
    assert_eq!(state.cancels, 0);
    assert_eq!(state.capacities.len(), 8, "no steady-state allocation");
}

#[test]
fn malformed_transfer_never_delivers_samples_and_requires_cleanup() {
    for length in [0, 1, 15, 16, 4096, 4111, 4113, 4608] {
        let (mut rx, state) = endpoint(512);
        rx.open(4096).unwrap();
        state
            .lock()
            .unwrap()
            .incoming
            .push_back(Ok(Some(vec![0; length])));
        assert!(matches!(
            rx.receive(Duration::ZERO, &mut |_, _| panic!(
                "malformed length {length}"
            )),
            Err(BusError::Io(_))
        ));
        assert!(rx.receive(Duration::ZERO, &mut |_, _| panic!()).is_err());
        assert!(rx.open(4096).is_err());
        rx.close().unwrap();
        rx.open(4096).unwrap();
        assert_eq!(state.lock().unwrap().pending.len(), 8);
    }
}

#[test]
fn allocation_and_submission_failures_keep_partial_rings_drainable() {
    for allocation in [true, false] {
        let (mut rx, state) = endpoint(512);
        {
            let mut state = state.lock().unwrap();
            if allocation {
                state.fail_alloc = Some(3);
            } else {
                state.fail_submit = Some(3);
            }
        }
        assert!(matches!(rx.open(4096), Err(BusError::Disconnected)));
        assert_eq!(state.lock().unwrap().pending.len(), 3);
        assert!(rx.open(4096).is_err());
        rx.close().unwrap();
        let mut state = state.lock().unwrap();
        assert!(state.pending.is_empty());
        state.fail_alloc = None;
        state.fail_submit = None;
        drop(state);
        rx.open(4096).unwrap();
    }
}

#[test]
fn failed_drain_cannot_reopen_until_every_old_transfer_is_returned() {
    let (mut rx, state) = endpoint(512);
    rx.open(4096).unwrap();
    state.lock().unwrap().fail_cancel = true;
    assert!(matches!(rx.close(), Err(BusError::Timeout)));
    assert!(rx.open(4096).is_err());
    assert!(rx.receive(Duration::ZERO, &mut |_, _| panic!()).is_err());
    state.lock().unwrap().fail_cancel = false;
    rx.close().unwrap();
    rx.open(4096).unwrap();
    assert_eq!(state.lock().unwrap().pending.len(), 8);
}

#[test]
fn transfer_and_requeue_failures_require_close_but_close_remains_available() {
    for requeue in [false, true] {
        let (mut rx, state) = endpoint(1024);
        rx.open(4096).unwrap();
        {
            let mut state = state.lock().unwrap();
            if requeue {
                state.incoming.push_back(Ok(Some(frame())));
                state.fail_submit = Some(8);
            } else {
                state.incoming.push_back(Err(BusError::Disconnected));
            }
        }
        let mut delivered = false;
        assert!(matches!(
            rx.receive(Duration::ZERO, &mut |_, _| delivered = true),
            Err(BusError::Disconnected)
        ));
        assert_eq!(delivered, requeue);
        assert!(rx.receive(Duration::ZERO, &mut |_, _| panic!()).is_err());
        rx.close().unwrap();
        assert!(state.lock().unwrap().pending.is_empty());
    }
}

#[test]
fn invalid_block_or_endpoint_cannot_allocate_and_bounds_are_inclusive() {
    for block in [0, 1, 7, 9, (1 << 20) + 8, u32::MAX] {
        let (mut rx, state) = endpoint(512);
        assert!(rx.open(block).is_err(), "block {block}");
        assert_eq!(state.lock().unwrap().capacities, Vec::<usize>::new());
    }
    for packet in [0, 64, 256, 513, 2048, usize::MAX] {
        let (mut rx, state) = endpoint(packet);
        assert!(rx.open(4096).is_err(), "packet {packet}");
        assert_eq!(state.lock().unwrap().capacities, Vec::<usize>::new());
    }
    for block in [8, 1 << 20] {
        let (mut rx, state) = endpoint(1024);
        rx.open(block).unwrap();
        assert_eq!(state.lock().unwrap().pending.len(), 8);
        assert!(rx.open(block).is_err());
    }
}

#[test]
fn whole_packet_frames_leave_room_for_the_terminating_zero_length_packet() {
    for (packet, block) in [(512, 496), (1024, 1008), (512, 4080)] {
        let (mut rx, state) = endpoint(packet);
        rx.open(block).unwrap();
        let capacity = (usize::try_from(block).unwrap() + 16) + packet;
        assert_eq!(state.lock().unwrap().capacities, vec![capacity; 8]);
        let mut frame = vec![0x5a; usize::try_from(block).unwrap()];
        frame.extend([0; 16]);
        state.lock().unwrap().incoming.push_back(Ok(Some(frame)));
        rx.receive(Duration::ZERO, &mut |payload, oob| {
            assert_eq!(payload.len(), usize::try_from(block).unwrap());
            assert_eq!(oob, [0; 2]);
        })
        .unwrap();
    }
}
