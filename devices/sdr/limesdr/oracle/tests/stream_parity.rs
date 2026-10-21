//! limesdr's RX and TX streams against libusdr's (`u3_limesdr.c`'s stream create and sync,
//! `limesdr_ctrl.c`'s `limesdr_setup_stream` and `limesdr_disable_stream`, and
//! `ipblks/streams/stream_limesdr.c`'s framing), operation for operation, on fresh
//! simulated LimeSDR-minis. The transport is the shim's buffer boundary: the same RX blocks
//! from the board, the TX blocks committed to it compared byte for byte, and every
//! received sample and time compared.
//!
//! Whitelisted divergences, each in its own test: an RX packet of 0 samples is refused
//! before any exchange, where libusdr creates a stream whose receives deliver nothing.

#[path = "../../tests/common/sim_port.rs"]
mod sim_port;

use std::time::Duration;

use limesdr::{Device, Error};
use limesdr_oracle::Oracle;
use limesdr_sim::{Op, SimBoard};
use sim_port::SimPort;

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ETIMEDOUT`: the shim's wait with no block.
const ETIMEDOUT: i32 = 110;

/// One stream call, as libusdr's API and limesdr's method.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `usdr_dms_create` of `/ll/srx/0` with this many samples per packet.
    CreateRx(u32),
    /// `usdr_dms_create` of `/ll/stx/0`.
    CreateTx(u32),
    /// `usdr_dms_sync` with a sync type other than `"off"`.
    Start,
    /// `usdr_dms_sync(.., "off", ..)`.
    Stop,
    /// `usdr_dms_recv` of one packet.
    Receive,
    /// `usdr_dms_send` of this many samples, at this time or none.
    Send(usize, Option<u64>),
    /// `usdr_dms_destroy` of the RX stream.
    DestroyRx,
}

/// What one call returned: an errno, or a packet's samples and first time.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    /// It failed with this negative errno, or succeeded with 0.
    Errno(i32),
    /// A receive delivered these samples, the first at this time.
    Packet(Vec<[i16; 2]>, u64),
}

/// The samples a send of `count` carries: distinct, so a misplaced one shows.
fn tx_samples(count: usize) -> Vec<[i16; 2]> {
    (0..count)
        .map(|n| {
            let n = i16::try_from(n % 30_000).expect("in range");
            [n, -n]
        })
        .collect()
}

/// libusdr's outcomes and the operations between its open and close.
fn libusdr(board: SimBoard, calls: &[Call]) -> (Vec<Outcome>, Vec<Op>) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let mut packet = 0;
    let outcomes = calls
        .iter()
        .map(|&call| {
            let errno = |result: Result<(), i32>| Outcome::Errno(result.err().unwrap_or(0));
            match call {
                Call::CreateRx(samples) => {
                    packet = samples as usize;
                    errno(oracle.create_rx_stream(samples))
                }
                Call::CreateTx(samples) => errno(oracle.create_tx_stream(samples)),
                Call::Start => errno(oracle.sync(c"rx")),
                Call::Stop => errno(oracle.sync(c"off")),
                Call::Receive => {
                    let mut buffer = vec![[0x5555; 2]; packet];
                    match oracle.receive(&mut buffer) {
                        Ok(received) => {
                            assert_eq!(received.samples as usize, packet, "a whole packet");
                            assert_eq!(received.lost, 0, "libusdr reports none lost");
                            Outcome::Packet(buffer, received.time)
                        }
                        Err(errno) => Outcome::Errno(errno),
                    }
                }
                Call::Send(count, time) => {
                    errno(oracle.send(&tx_samples(count), time.unwrap_or(u64::MAX)))
                }
                Call::DestroyRx => errno(oracle.destroy_rx_stream()),
            }
        })
        .collect();
    let used = oracle.trace_len();
    let board = oracle.close();
    (outcomes, board.trace()[opened..used].to_vec())
}

/// limesdr's outcomes, as libusdr's errnos, and its operations after the open.
fn rsl(board: SimBoard, calls: &[Call]) -> (Vec<Outcome>, Vec<Op>) {
    let sim = SimPort::new(board);
    let mut device = Device::open(sim.clone()).expect("limesdr opens the board");
    let opened = sim.board().trace().len();
    let timeout = Duration::from_millis(100);
    let mut packet = 0;
    let outcomes = calls
        .iter()
        .map(|&call| {
            let outcome = |result: Result<(), Error>| Outcome::Errno(errno(result.err()));
            match call {
                Call::CreateRx(samples) => {
                    packet = samples as usize;
                    outcome(device.create_rx_stream(samples))
                }
                Call::CreateTx(samples) => outcome(device.create_tx_stream(samples)),
                Call::Start => outcome(device.start_streams()),
                Call::Stop => outcome(device.stop_streams()),
                Call::Receive => {
                    let mut buffer = vec![[0x5555; 2]; packet];
                    match device.receive(&mut buffer, timeout) {
                        Ok(received) => {
                            assert_eq!(received.samples, packet, "a whole packet");
                            Outcome::Packet(buffer, received.time)
                        }
                        Err(error) => Outcome::Errno(errno(Some(error))),
                    }
                }
                Call::Send(count, time) => outcome(device.send(&tx_samples(count), time, timeout)),
                Call::DestroyRx => outcome(device.destroy_rx_stream()),
            }
        })
        .collect();
    device.close().expect("limesdr's close does nothing");
    let trace = sim.board().trace()[opened..].to_vec();
    (outcomes, trace)
}

/// The errno libusdr returns where limesdr fails with `error`, 0 for none.
fn errno(error: Option<Error>) -> i32 {
    match error {
        None => 0,
        Some(Error::Timeout) => -ETIMEDOUT,
        Some(Error::SendSize { .. } | Error::UnsupportedPacketSize(_)) => -EINVAL,
        Some(other) => panic!("no libusdr counterpart: {other}"),
    }
}

/// Both drivers' outcomes and operations on boards `board` builds, which must agree;
/// libusdr's.
fn assert_same_on(
    board: &dyn Fn() -> SimBoard,
    calls: &[Call],
    scenario: &str,
) -> (Vec<Outcome>, Vec<Op>) {
    let (expected, actual) = (libusdr(board(), calls), rsl(board(), calls));
    if let Some(at) =
        (0..expected.1.len().max(actual.1.len())).find(|&i| expected.1.get(i) != actual.1.get(i))
    {
        let line = |op: Option<&Op>| op.map_or_else(|| "(none)".to_owned(), ToString::to_string);
        panic!(
            "{scenario}: op {at} differs\n  libusdr: {}\n  limesdr: {}",
            line(expected.1.get(at)),
            line(actual.1.get(at))
        );
    }
    for (n, (actual, expected)) in actual.0.iter().zip(&expected.0).enumerate() {
        assert!(
            actual == expected,
            "{scenario}: call {n} ({:?}) differs",
            calls[n]
        );
    }
    assert_eq!(actual.0.len(), expected.0.len());
    expected
}

/// The operations matching `pick`, as lines.
fn lines(ops: &[Op], pick: impl Fn(&Op) -> bool) -> Vec<String> {
    ops.iter()
        .filter(|op| pick(op))
        .map(ToString::to_string)
        .collect()
}

#[test]
fn an_rx_session_matches_libusdr() {
    let calls = [
        Call::CreateRx(1020),
        Call::Start,
        Call::Receive,
        Call::Receive,
        Call::Receive,
        Call::Stop,
        Call::DestroyRx,
    ];
    let (outcomes, ops) = assert_same_on(&SimBoard::new, &calls, "RX session");
    assert_eq!(
        outcomes[2..5]
            .iter()
            .map(|outcome| match outcome {
                Outcome::Packet(_, time) => *time,
                Outcome::Errno(errno) => panic!("received: {errno}"),
            })
            .collect::<Vec<_>>(),
        [0, 1020, 2040],
        "the timestamp starts from the setup's reset"
    );
    assert_eq!(
        lines(&ops, |op| matches!(
            op,
            Op::StreamOpen { .. } | Op::RxBlock { .. }
        )),
        [
            "stream-open rx 4096",
            "rx-block 4096 0",
            "rx-block 4096 1020",
            "rx-block 4096 2040"
        ]
    );
}

#[test]
fn rx_packets_across_and_within_blocks_match_libusdr() {
    for packet in [1500, 2040, 3060, 4096, 10_000] {
        let calls = [
            Call::CreateRx(packet),
            Call::Start,
            Call::Receive,
            Call::Receive,
            Call::Receive,
            Call::Receive,
        ];
        assert_same_on(&SimBoard::new, &calls, &format!("RX packets of {packet}"));
    }
}

#[test]
fn lost_rx_samples_are_zero_filled_like_libusdr() {
    for (packet, after, lost) in [
        (1020, 2, 100),
        (1020, 1, 2600),
        (1500, 3, 700),
        (3060, 1, 5),
        (4096, 4, 9000),
    ] {
        let board = || SimBoard::new().with_rx_gap(after, lost);
        let calls = [
            Call::CreateRx(packet),
            Call::Start,
            Call::Receive,
            Call::Receive,
            Call::Receive,
            Call::Receive,
            Call::Receive,
        ];
        let (outcomes, _) = assert_same_on(
            &board,
            &calls,
            &format!("{lost} samples lost after {after} bursts, packets of {packet}"),
        );
        assert!(
            outcomes.iter().any(
                |outcome| matches!(outcome, Outcome::Packet(samples, _) if samples.contains(&[0, 0]))
            ),
            "the gap is filled with zeroes"
        );
    }
}

#[test]
fn receiving_before_the_streams_start_times_out_like_libusdr() {
    let calls = [
        Call::CreateRx(1020),
        Call::Receive,
        Call::Start,
        Call::Receive,
        Call::Stop,
        Call::Receive,
    ];
    let (outcomes, _) = assert_same_on(&SimBoard::new, &calls, "receive while stopped");
    assert_eq!(outcomes[1], Outcome::Errno(-ETIMEDOUT));
    assert_eq!(outcomes[5], Outcome::Errno(-ETIMEDOUT));
}

#[test]
fn a_tx_session_matches_libusdr_byte_for_byte() {
    for packet in [0, 100, 4096, 8192] {
        let calls = [
            Call::CreateTx(packet),
            Call::Start,
            Call::Send(4096, Some(1000)),
            Call::Send(8, None),
            Call::Send(1020, Some(0)),
            Call::Send(0, Some(5)),
            Call::Send(4100, Some(7)),
            Call::Send(4098, None),
            Call::Stop,
        ];
        let (outcomes, ops) =
            assert_same_on(&SimBoard::new, &calls, &format!("TX packets of {packet}"));
        let blocks = lines(&ops, |op| matches!(op, Op::TxBlock { .. }));
        assert!(blocks.len() >= 4, "{packet}: {blocks:?}");
        assert_eq!(outcomes[7], Outcome::Errno(-EINVAL), "not a multiple of 4");
    }
}

#[test]
fn tx_packet_sizes_are_checked_like_libusdr() {
    let calls = |packet| [Call::CreateTx(packet), Call::Start, Call::Send(4, None)];
    let (outcomes, _) = assert_same_on(&SimBoard::new, &calls(4094), "raised to 4096");
    assert_eq!(outcomes[2], Outcome::Errno(0));
    let (outcomes, ops) = assert_same_on(&SimBoard::new, &calls(8194)[..2], "not a multiple of 4");
    assert_eq!(outcomes[0], Outcome::Errno(-EINVAL));
    assert!(
        lines(&ops, |op| matches!(op, Op::StreamOpen { .. })).is_empty(),
        "refused before preparing streaming"
    );
}

#[test]
fn both_streams_with_repeated_syncs_match_libusdr() {
    let calls = [
        Call::Stop,
        Call::CreateRx(2040),
        Call::CreateTx(4096),
        Call::Start,
        Call::Start,
        Call::Receive,
        Call::Send(4096, Some(2040)),
        Call::Receive,
        Call::Stop,
        Call::Start,
        Call::Receive,
    ];
    let (_, ops) = assert_same_on(
        &|| SimBoard::new().with_rx_gap(5, 300),
        &calls,
        "both streams",
    );
    let setups = lines(&ops, |op| matches!(op, Op::FpgaWrite { addr: 0x0008, .. }));
    assert_eq!(
        setups,
        ["fpga-wr 0x0008 0x0040", "fpga-wr 0x0008 0x0040"],
        "a second start while started does nothing; one after a stop sets up again"
    );
}

#[test]
fn an_rx_packet_of_zero_is_refused_where_libusdr_streams_empty_packets() {
    let (libusdr_outcomes, libusdr_ops) = libusdr(SimBoard::new(), &[Call::CreateRx(0)]);
    assert_eq!(libusdr_outcomes, [Outcome::Errno(0)]);
    assert!(
        lines(&libusdr_ops, |op| matches!(op, Op::StreamOpen { .. })) == ["stream-open rx 4096"],
        "libusdr prepares streaming and sets a one-burst stream up"
    );
    let sim = SimPort::new(SimBoard::new());
    let mut device = Device::open(sim.clone()).expect("opened");
    let opened = sim.board().trace().len();
    assert!(matches!(
        device.create_rx_stream(0),
        Err(Error::UnsupportedPacketSize(0))
    ));
    assert_eq!(sim.board().trace().len(), opened, "before any exchange");
}
