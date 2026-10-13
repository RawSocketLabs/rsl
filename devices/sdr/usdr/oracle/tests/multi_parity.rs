//! rsl-usdr's `MultiDevice` against libusdr's multi-device aggregate (`device/mdev.c`,
//! opened with `bus=sim0:sim1:sim2`): each board's operations in each step, the samples
//! each device delivers and plays, and where both stop when one device fails. Also the
//! points where libusdr's aggregate fails and rsl-usdr's does not, as evidence for the
//! divergences.

#[path = "common/baseband.rs"]
mod baseband;
#[allow(
    dead_code,
    reason = "only the Si5332 address and mixer block are used, through the baseband cut"
)]
#[path = "common/crossing.rs"]
mod crossing;
#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;
#[path = "common/thermal_gate.rs"]
mod thermal_gate;

use std::ffi::CStr;
use std::iter;
use std::ops::Range;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use baseband::without_baseband_loop;
use num_complex::Complex;
use rsl_usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr, TxCommit, TxFill};
use rsl_usdr::{
    Device, Error, FrontEndKind, FrontEndSelection, HostFormat, MultiDevice, MultiError, MultiStep,
    Outcome, RxConfig, StreamTime, StreamsConfig, SyncMode, TxConfig, WireFormat,
};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, FrontEnd, Op, PllLock, SimBoard, TxPacket};
use sim_bus::SimBus;
use thermal_gate::{assert_same_ops, without_thermal_gate};

/// The aggregate's boards' revisions, the master first: libusdr's aggregate takes any
/// uSDRs, whatever their revision.
const REVISIONS: [BoardRevision; 3] = [
    BoardRevision::Rev3,
    BoardRevision::Rev1,
    BoardRevision::Rev2,
];

/// The sync unit's register, which the TX core's commands share.
const REG_SYNC: u32 = 14;

/// libusdr's channel mask for one channel on each device: the aggregate gives device `i`
/// a third of the channels, each its channel 0.
const ALL_CHANNELS: u64 = 0b111;

/// Samples per packet.
const SAMPLES: u32 = 1024;

/// A sync-unit write of mode `mode`.
fn sync_write(mode: u32) -> Op {
    Op::RegWrite {
        addr: REG_SYNC,
        value: 1 << 31 | mode << 16,
    }
}

/// Fresh boards, `adjust`ed by index.
fn boards(adjust: impl Fn(usize, SimBoard) -> SimBoard) -> Vec<SimBoard> {
    REVISIONS
        .iter()
        .enumerate()
        .map(|(index, &revision)| adjust(index, SimBoard::new(revision)))
        .collect()
}

/// The same boards, unchanged.
fn plain(_: usize, board: SimBoard) -> SimBoard {
    board
}

/// What one side did: each board's trace and each step's trace range on each board.
struct Run {
    /// Each board's trace at the end.
    traces: Vec<Vec<Op>>,
    /// Each step's name and range on each board.
    windows: Vec<(&'static str, Vec<Range<usize>>)>,
    /// The board of every operation, in the order they were performed across boards.
    order: Vec<usize>,
    /// Each step's range in `order`.
    order_windows: Vec<Range<usize>>,
    /// Each receive's bytes, per device.
    received: Vec<Vec<Vec<u8>>>,
    /// The packets each board's TX core played.
    played: Vec<Vec<TxPacket>>,
}

impl Run {
    /// Board `board`'s operations in the step named `name`.
    fn step(&self, board: usize, name: &str) -> &[Op] {
        let (_, ranges) = self
            .windows
            .iter()
            .find(|(step, _)| *step == name)
            .expect("the run has the step");
        &self.traces[board][ranges[board].clone()]
    }

    /// Every board's operations in the step named `name`, as (board, operation), in the
    /// order they were performed across the boards.
    fn interleaved(&self, name: &str) -> Vec<(usize, Op)> {
        let position = self
            .windows
            .iter()
            .position(|(step, _)| *step == name)
            .expect("the run has the step");
        let mut cursors: Vec<usize> = self.windows[position]
            .1
            .iter()
            .map(|range| range.start)
            .collect();
        self.order[self.order_windows[position].clone()]
            .iter()
            .map(|&board| {
                let op = self.traces[board][cursors[board]].clone();
                cursors[board] += 1;
                (board, op)
            })
            .collect()
    }
}

/// A board's bus that records, in an order the boards share, the board of each operation
/// its board traces.
struct Ordered {
    /// The board.
    sim: SimBus,
    /// Its index in the aggregate.
    index: usize,
    /// The shared order.
    order: Arc<Mutex<Vec<usize>>>,
}

impl Ordered {
    /// Runs `f` on the board's bus, recording each operation it traces.
    fn ordered<R>(&mut self, f: impl FnOnce(&mut SimBus) -> R) -> R {
        let before = self.sim.board().trace().len();
        let result = f(&mut self.sim);
        let added = self.sim.board().trace().len() - before;
        self.order
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(iter::repeat_n(self.index, added));
        result
    }
}

impl Bus for Ordered {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.ordered(|sim| sim.read_regs(addr, out))
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        self.ordered(|sim| sim.write_regs(addr, values))
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.ordered(|sim| sim.spi32(target, word))
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.ordered(|sim| sim.i2c(dev, write, read))
    }

    fn sleep(&mut self, duration: Duration) {
        self.ordered(|sim| sim.sleep(duration));
    }

    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.ordered(|sim| sim.rx_stream_open(block_bytes))
    }

    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        self.ordered(|sim| sim.rx_stream_recv(timeout, consume))
    }

    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        self.ordered(SimBus::rx_stream_close)
    }

    fn tx_stream_open(&mut self, block_bytes: u32, sample_bytes: u32) -> Result<(), BusError> {
        self.ordered(|sim| sim.tx_stream_open(block_bytes, sample_bytes))
    }

    fn tx_stream_send(
        &mut self,
        timeout: Duration,
        time: Option<u64>,
        fill: &mut TxFill<'_>,
    ) -> Result<TxCommit, BusError> {
        self.ordered(|sim| sim.tx_stream_send(timeout, time, fill))
    }

    fn tx_stream_close(&mut self) -> Result<(), BusError> {
        self.ordered(SimBus::tx_stream_close)
    }
}

/// Records the trace ranges a step adds on each board.
fn ranges(before: &[usize], after: &[usize]) -> Vec<Range<usize>> {
    before.iter().zip(after).map(|(&b, &a)| b..a).collect()
}

/// rsl-usdr's sims, their trace lengths and the order of their operations.
struct Sims(Vec<SimBus>, Arc<Mutex<Vec<usize>>>);

impl Sims {
    /// Wraps `boards`.
    fn new(boards: Vec<SimBoard>) -> Self {
        Self(
            boards.into_iter().map(SimBus::new).collect(),
            Arc::default(),
        )
    }

    /// Each board's trace length now.
    fn lens(&self) -> Vec<usize> {
        self.0.iter().map(|sim| sim.board().trace().len()).collect()
    }

    /// The board of every operation so far, in order across the boards.
    fn order(&self) -> Vec<usize> {
        self.1
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Opens the aggregate over the sims with `front_end` on each.
    fn open(&self, front_end: FrontEndSelection) -> Result<MultiDevice, MultiError<()>> {
        let builder = |index: usize| {
            let bus = Ordered {
                sim: self.0[index].clone(),
                index,
                order: Arc::clone(&self.1),
            };
            Device::builder(bus).front_end(front_end)
        };
        MultiDevice::open(builder(0), (1..self.0.len()).map(builder))
    }

    /// Each board's trace and played packets.
    fn finish(&self) -> (Vec<Vec<Op>>, Vec<Vec<TxPacket>>) {
        self.0
            .iter()
            .map(|sim| {
                let board = sim.board();
                (board.trace().to_vec(), board.tx_packets().to_vec())
            })
            .unzip()
    }
}

/// The trace lengths of each of libusdr's boards.
fn oracle_lens(oracle: &Oracle) -> Vec<usize> {
    (0..REVISIONS.len())
        .map(|board| oracle.trace_len_of(board))
        .collect()
}

/// Packet `k` of device `device`: a pattern that differs between devices, so a channel
/// mapped to the wrong device shows.
fn packet(device: usize, k: usize) -> Vec<Complex<i16>> {
    let at = |i: usize, scale: usize| {
        i16::try_from((device * 1999 + k * 977 + i * scale) % 4001).expect("small") * 8 - 16_000
    };
    (0..SAMPLES as usize)
        .map(|i| Complex::new(at(i, 7), at(i, 13)))
        .collect()
}

/// A packet's `ci16` bytes, as libusdr's host buffers hold them.
fn le_bytes(samples: &[Complex<i16>]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| [sample.re.to_le_bytes(), sample.im.to_le_bytes()].concat())
        .collect()
}

/// A stream scenario made the same way on both sides.
#[derive(Clone, Copy, Debug)]
struct Scenario {
    /// libusdr's aggregate sync type and rsl-usdr's mode.
    sync: (&'static CStr, SyncMode),
    /// With a TX stream beside the RX one.
    with_tx: bool,
    /// Receives after the start (the sync must release the streams).
    receives: usize,
    /// Sends after the start, each with its time.
    sends: usize,
}

impl Scenario {
    /// rsl-usdr's configuration.
    fn config(self) -> StreamsConfig {
        let rx = StreamsConfig::rx(RxConfig::new(SAMPLES, HostFormat::Ci16, WireFormat::Ci16));
        let config = if self.with_tx {
            rx.with_tx(TxConfig::new(SAMPLES, HostFormat::Ci16, WireFormat::Ci16))
        } else {
            rx
        };
        config.with_sync(self.sync.1)
    }
}

/// The time of send `k`.
fn send_time(k: usize) -> u64 {
    0x10_0000 + u64::try_from(k).expect("small") * u64::from(SAMPLES)
}

/// libusdr's aggregate: open, broadcast the rate and tunes, create RX then TX, start (RX
/// alone: sync then start; with TX: start RX, start TX, sync), receive, send, stop and
/// destroy, close.
fn on_libusdr(scenario: Scenario) -> Run {
    let mut oracle = Oracle::open_multi(boards(plain), c"").expect("libusdr opens the aggregate");
    let mut windows = vec![("open", ranges(&[0; 3], &oracle_lens(&oracle)))];
    let mut order_windows = Vec::new();
    order_windows.push(0..oracle.order().len());
    let mut step = |oracle: &mut Oracle, name, run: &mut dyn FnMut(&mut Oracle)| {
        let (before, order_before) = (oracle_lens(oracle), oracle.order().len());
        run(oracle);
        windows.push((name, ranges(&before, &oracle_lens(oracle))));
        order_windows.push(order_before..oracle.order().len());
    };
    step(&mut oracle, "rate", &mut |o| {
        o.set_rx_rate(20_000_000).expect("rate");
    });
    step(&mut oracle, "rx tune", &mut |o| {
        o.set_rx_frequency(1_000_000_000).expect("tunes RX");
    });
    if scenario.with_tx {
        step(&mut oracle, "tx tune", &mut |o| {
            o.set_tx_frequency(1_500_000_000).expect("tunes TX");
        });
    }
    step(&mut oracle, "create", &mut |o| {
        o.create_rx_stream_on(c"ci16", ALL_CHANNELS, SAMPLES)
            .expect("libusdr creates RX");
        if scenario.with_tx {
            o.create_tx_stream_on(c"ci16", ALL_CHANNELS, SAMPLES)
                .expect("libusdr creates TX");
        }
    });
    step(&mut oracle, "start", &mut |o| {
        if scenario.with_tx {
            o.start_rx_stream().expect("starts RX");
            o.start_tx_stream().expect("starts TX");
            o.sync_streams(scenario.sync.0).expect("syncs");
        } else {
            o.sync_rx_stream(scenario.sync.0).expect("syncs");
            o.start_rx_stream().expect("starts RX");
        }
    });
    let mut received = Vec::new();
    for _ in 0..scenario.receives {
        step(&mut oracle, "receive", &mut |o| {
            let (buffers, _) = o.receive_rx_channels(100).expect("libusdr receives");
            received.push(buffers);
        });
    }
    for k in 0..scenario.sends {
        step(&mut oracle, "send", &mut |o| {
            let packets: Vec<Vec<u8>> = (0..3).map(|d| le_bytes(&packet(d, k))).collect();
            let channels: Vec<&[u8]> = packets.iter().map(Vec::as_slice).collect();
            o.send_tx_channels(&channels, SAMPLES, send_time(k), 100)
                .expect("libusdr sends");
        });
    }
    step(&mut oracle, "stop", &mut |o| {
        o.stop_rx_stream().expect("stops RX");
        if scenario.with_tx {
            o.stop_tx_stream().expect("stops TX");
        }
        o.destroy_rx_stream().expect("destroys RX");
        if scenario.with_tx {
            o.destroy_tx_stream().expect("destroys TX");
        }
    });
    let before = oracle_lens(&oracle);
    let order = oracle.order();
    let boards = oracle.close_all();
    let after: Vec<usize> = boards.iter().map(|board| board.trace().len()).collect();
    windows.push(("close", ranges(&before, &after)));
    // The order is taken before the close, so the close has none.
    order_windows.push(order.len()..order.len());
    Run {
        traces: boards.iter().map(|board| board.trace().to_vec()).collect(),
        windows,
        order,
        order_windows,
        received,
        played: boards
            .iter()
            .map(|board| board.tx_packets().to_vec())
            .collect(),
    }
}

/// rsl-usdr's `MultiDevice`, the same calls.
fn on_rsl(scenario: Scenario) -> Run {
    let sims = Sims::new(boards(plain));
    let mut multi = sims
        .open(FrontEndSelection::Auto)
        .expect("rsl-usdr opens the aggregate");
    let mut windows = vec![("open", ranges(&[0; 3], &sims.lens()))];
    let mut order_windows = Vec::new();
    order_windows.push(0..sims.order().len());
    let mut step = |multi: &mut MultiDevice, name, run: &mut dyn FnMut(&mut MultiDevice)| {
        let (before, order_before) = (sims.lens(), sims.order().len());
        run(multi);
        windows.push((name, ranges(&before, &sims.lens())));
        order_windows.push(order_before..sims.order().len());
    };
    step(&mut multi, "rate", &mut |m| {
        m.broadcast(|device| device.set_rx_sample_rate(20_000_000))
            .expect("rate");
    });
    step(&mut multi, "rx tune", &mut |m| {
        m.broadcast(|device| device.set_rx_frequency(1_000_000_000))
            .expect("tunes RX");
    });
    if scenario.with_tx {
        step(&mut multi, "tx tune", &mut |m| {
            m.broadcast(|device| device.set_tx_frequency(1_500_000_000))
                .expect("tunes TX");
        });
    }
    step(&mut multi, "create", &mut |m| {
        m.create_streams(scenario.config())
            .expect("rsl-usdr creates");
    });
    step(&mut multi, "start", &mut |m| {
        m.start_streams().expect("rsl-usdr starts");
    });
    let mut received = Vec::new();
    for _ in 0..scenario.receives {
        step(&mut multi, "receive", &mut |m| {
            let mut buffers = vec![vec![Complex::new(0, 0); SAMPLES as usize]; 3];
            let mut channels: Vec<&mut [Complex<i16>]> =
                buffers.iter_mut().map(Vec::as_mut_slice).collect();
            m.receive_into(&mut channels, Duration::from_millis(100))
                .expect("rsl-usdr receives");
            received.push(buffers.iter().map(|buffer| le_bytes(buffer)).collect());
        });
    }
    for k in 0..scenario.sends {
        step(&mut multi, "send", &mut |m| {
            let packets: Vec<Vec<Complex<i16>>> = (0..3).map(|d| packet(d, k)).collect();
            let channels: Vec<&[Complex<i16>]> = packets.iter().map(Vec::as_slice).collect();
            m.send(
                &channels,
                Some(StreamTime::from_samples(send_time(k))),
                Duration::from_millis(100),
            )
            .expect("rsl-usdr sends");
        });
    }
    step(&mut multi, "stop", &mut |m| {
        m.stop_streams().expect("rsl-usdr stops");
    });
    let before = sims.lens();
    let order = sims.order();
    multi.close().expect("rsl-usdr closes");
    windows.push(("close", ranges(&before, &sims.lens())));
    order_windows.push(order.len()..order.len());
    let (traces, played) = sims.finish();
    Run {
        traces,
        windows,
        order,
        order_windows,
        received,
        played,
    }
}

/// Every step's operations on every board match: libusdr's RX create without its
/// baseband loop and rsl-usdr's open without its thermal gate, the whitelisted
/// differences of the single-device tests.
fn assert_runs_match(expected: &Run, actual: &Run, context: &str) {
    let names = |run: &Run| {
        run.windows
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(expected), names(actual), "{context}: the same steps");
    for ((name, expected_ranges), (_, actual_ranges)) in
        expected.windows.iter().zip(&actual.windows)
    {
        for (board, revision) in REVISIONS.iter().enumerate() {
            let mut expected_ops = expected.traces[board][expected_ranges[board].clone()].to_vec();
            let mut actual_ops = actual.traces[board][actual_ranges[board].clone()].to_vec();
            if *name == "create" {
                (expected_ops, _) = without_baseband_loop(&expected_ops, false);
            }
            if *name == "open" {
                actual_ops = without_thermal_gate(&actual_ops, *revision);
            }
            assert_same_ops(
                &expected_ops,
                &actual_ops,
                &format!("{context}: board {board}, step {name}"),
            );
        }
    }
}

/// The start and stop happen in the same order across the boards on both sides: each
/// part of the start on every device before the next part, the master's SYSREF after
/// every device's sync, each part of the stop on every device before the next.
fn assert_interleaving_matches(expected: &Run, actual: &Run, context: &str) {
    for name in ["start", "stop"] {
        assert_eq!(
            expected.interleaved(name),
            actual.interleaved(name),
            "{context}: the {name} step's operations across the boards, in order"
        );
    }
}

/// An RX group free-running on every device: each board's operations, and each device's
/// samples in its own channel.
#[test]
fn a_free_running_rx_group_spans_the_devices_like_libusdr() {
    let scenario = Scenario {
        sync: (c"none", SyncMode::None),
        with_tx: false,
        receives: 2,
        sends: 0,
    };
    let (expected, actual) = (on_libusdr(scenario), on_rsl(scenario));
    assert_runs_match(&expected, &actual, "free-running RX");
    assert_interleaving_matches(&expected, &actual, "free-running RX");
    assert_eq!(expected.received.len(), 2);
    assert_eq!(
        expected.received, actual.received,
        "each device's packet lands in its own channel's buffer"
    );
}

/// Each device's RX activity starts it (`"all"` with RX alone): no alignment across the
/// devices, as in libusdr.
#[test]
fn an_activity_synchronised_rx_group_matches_libusdr() {
    let scenario = Scenario {
        sync: (c"all", SyncMode::All),
        with_tx: false,
        receives: 1,
        sends: 0,
    };
    let (expected, actual) = (on_libusdr(scenario), on_rsl(scenario));
    assert_runs_match(&expected, &actual, "RX on activity");
    assert_interleaving_matches(&expected, &actual, "RX on activity");
    assert_eq!(expected.received, actual.received);
}

/// RX and TX on every device, free-running: the packets each TX core plays are its own
/// channel's.
#[test]
fn a_duplex_group_plays_each_devices_channel_like_libusdr() {
    let scenario = Scenario {
        sync: (c"none", SyncMode::None),
        with_tx: true,
        receives: 1,
        sends: 2,
    };
    let (expected, actual) = (on_libusdr(scenario), on_rsl(scenario));
    assert_runs_match(&expected, &actual, "duplex");
    assert_interleaving_matches(&expected, &actual, "duplex");
    assert!(expected.played.iter().all(|played| played.len() == 2));
    assert_eq!(
        expected.played, actual.played,
        "each device plays its own channel"
    );
    assert_ne!(
        expected.played[0], expected.played[1],
        "the channels differ"
    );
}

/// Every device waits for 1PPS from outside (`"1pps"`): no device generates it.
#[test]
fn a_one_pps_group_waits_on_every_device_like_libusdr() {
    let scenario = Scenario {
        sync: (c"1pps", SyncMode::OnePps),
        with_tx: true,
        receives: 0,
        sends: 1,
    };
    let (expected, actual) = (on_libusdr(scenario), on_rsl(scenario));
    assert_runs_match(&expected, &actual, "1PPS");
    assert_interleaving_matches(&expected, &actual, "1PPS");
    for board in 0..3 {
        let start = actual.step(board, "start");
        assert_eq!(
            start.last(),
            Some(&sync_write(1)),
            "board {board} waits for 1PPS"
        );
        assert!(
            !start.contains(&sync_write(9)),
            "board {board} generates nothing"
        );
    }
}

/// libusdr's aggregate `"sysref"`, rsl-usdr's `GeneratedSysref`: every device waits for
/// 1PPS, then the master alone generates SYSREF.
#[test]
fn a_generated_sysref_group_has_the_master_generate_it_like_libusdr() {
    let scenario = Scenario {
        sync: (c"sysref", SyncMode::GeneratedSysref),
        with_tx: true,
        receives: 0,
        sends: 1,
    };
    let (expected, actual) = (on_libusdr(scenario), on_rsl(scenario));
    assert_runs_match(&expected, &actual, "generated SYSREF");
    assert_interleaving_matches(&expected, &actual, "generated SYSREF");
    let start = |board: usize| actual.step(board, "start");
    assert!(
        start(0).ends_with(&[sync_write(1), sync_write(9)]),
        "the master arms on 1PPS, then generates SYSREF"
    );
    for board in 1..3 {
        assert_eq!(
            start(board).last(),
            Some(&sync_write(1)),
            "board {board} waits"
        );
        assert!(
            !start(board).contains(&sync_write(9)),
            "board {board} generates nothing"
        );
    }
}

/// libusdr refuses its aggregate's direct `"sysref+gen"`, every device generating, before
/// any operation; rsl-usdr's `GeneratedSysref` means the master's generator instead, so
/// the refused form cannot be asked for.
#[test]
fn libusdr_refuses_every_device_generating_sysref() {
    let mut oracle = Oracle::open_multi(boards(plain), c"").expect("opens");
    oracle.set_rx_rate(20_000_000).expect("rate");
    oracle
        .create_rx_stream_on(c"ci16", ALL_CHANNELS, SAMPLES)
        .expect("creates");
    let before = oracle_lens(&oracle);
    assert_eq!(oracle.sync_rx_stream(c"sysref+gen"), Err(-22));
    assert_eq!(oracle_lens(&oracle), before, "refused before any operation");
}

/// A setting that fails on device 1 stops there on both sides: device 0 has it, device 2
/// never sees it; rsl-usdr reports each device's outcome.
#[test]
fn a_failed_broadcast_stops_at_its_device_like_libusdr() {
    let unlocked = |index, board: SimBoard| {
        if index == 1 {
            board.with_rx_pll_lock(PllLock {
                unlocked_below_hz: 2_000_000_000,
                ..PllLock::default()
            })
        } else {
            board
        }
    };
    let mut oracle = Oracle::open_multi(boards(unlocked), c"").expect("opens");
    oracle.set_rx_rate(20_000_000).expect("rate");
    let before = oracle_lens(&oracle);
    assert!(
        oracle.set_rx_frequency(1_000_000_000).is_err(),
        "libusdr fails"
    );
    let after = oracle_lens(&oracle);
    let expected = oracle.close_all();

    let sims = Sims::new(boards(unlocked));
    let mut multi = sims.open(FrontEndSelection::Auto).expect("opens");
    multi
        .broadcast(|device| device.set_rx_sample_rate(20_000_000))
        .expect("rate");
    let rsl_before = sims.lens();
    let result = multi.broadcast(|device| device.set_rx_frequency(1_000_000_000));
    let Err(MultiError::Devices { step, outcomes }) = result else {
        panic!("the broadcast fails on device 1: {result:?}");
    };
    assert_eq!(step, MultiStep::Call);
    assert!(
        matches!(
            outcomes[..],
            [
                Outcome::Done(()),
                Outcome::Failed(Error::PllUnlocked(_)),
                Outcome::NotReached
            ]
        ),
        "device 0 tuned, device 1 failed, device 2 untouched: {outcomes:?}"
    );
    let rsl_after = sims.lens();
    for board in 0..3 {
        assert_same_ops(
            &expected[board].trace()[before[board]..after[board]],
            &sims.0[board].board().trace()[rsl_before[board]..rsl_after[board]],
            &format!("board {board}"),
        );
    }
    assert_eq!(after[2], before[2], "libusdr never reached device 2");
}

/// A TX create that fails on device 1 (gateware older than TX streaming needs) after RX
/// was created everywhere: both stop there with the same operations. libusdr then holds
/// device 0's TX stream with no handle to destroy it, and a retry fails `-EBUSY`;
/// rsl-usdr reports each device's outcome and stops every group it left.
#[test]
fn a_create_failing_on_one_device_stops_there_like_libusdr() {
    let old_build = 0xd2b1_0c08;
    let outdated = |index, board: SimBoard| {
        if index == 1 {
            board.with_gateware_build(old_build)
        } else {
            board
        }
    };
    let mut oracle = Oracle::open_multi(boards(outdated), c"").expect("opens");
    oracle.set_rx_rate(20_000_000).expect("rate");
    oracle.set_rx_frequency(1_000_000_000).expect("tunes");
    let before = oracle_lens(&oracle);
    oracle
        .create_rx_stream_on(c"ci16", ALL_CHANNELS, SAMPLES)
        .expect("creates RX everywhere");
    assert_eq!(
        oracle.create_tx_stream_on(c"ci16", ALL_CHANNELS, SAMPLES),
        Err(-104),
        "device 1's gateware is too old"
    );
    let after = oracle_lens(&oracle);
    assert_eq!(
        oracle.create_tx_stream_on(c"ci16", ALL_CHANNELS, SAMPLES),
        Err(-16),
        "libusdr's device 0 still holds the TX stream it created"
    );
    assert_eq!(oracle_lens(&oracle), after, "the retry operates nothing");
    oracle.destroy_rx_stream().expect("destroys RX");
    let expected = oracle.close_all();

    let sims = Sims::new(boards(outdated));
    let mut multi = sims.open(FrontEndSelection::Auto).expect("opens");
    multi
        .broadcast(|device| device.set_rx_sample_rate(20_000_000))
        .expect("rate");
    multi
        .broadcast(|device| device.set_rx_frequency(1_000_000_000))
        .expect("tunes");
    let rsl_before = sims.lens();
    let config = Scenario {
        sync: (c"none", SyncMode::None),
        with_tx: true,
        receives: 0,
        sends: 0,
    }
    .config();
    let result = multi.create_streams(config);
    let Err(MultiError::Devices { step, outcomes }) = result else {
        panic!("the TX create fails on device 1: {result:?}");
    };
    assert_eq!(step, MultiStep::CreateTx);
    assert!(
        matches!(
            outcomes[..],
            [
                Outcome::Done(()),
                Outcome::Failed(Error::OutdatedGateware(build)),
                Outcome::NotReached
            ] if build == old_build
        ),
        "{outcomes:?}"
    );
    let rsl_after = sims.lens();
    for board in 0..3 {
        let (window, _) =
            without_baseband_loop(&expected[board].trace()[before[board]..after[board]], false);
        assert_same_ops(
            &window,
            &sims.0[board].board().trace()[rsl_before[board]..rsl_after[board]],
            &format!("board {board}"),
        );
    }
    multi
        .stop_streams()
        .expect("rsl-usdr stops every group it left");
    multi
        .create_streams(StreamsConfig::rx(RxConfig::new(
            SAMPLES,
            HostFormat::Ci16,
            WireFormat::Ci16,
        )))
        .expect("and can create again");
}

/// Device 1's sync unit stops after the start (written on both sides alike): the receive
/// delivers device 0's packet, then times out on device 1 and never reaches device 2.
#[test]
fn a_receive_failing_on_one_device_stops_there_like_libusdr() {
    let stop_device_1 = |board: &mut SimBoard| board.write_reg(REG_SYNC, 1 << 31);

    let mut oracle = Oracle::open_multi(boards(plain), c"").expect("opens");
    oracle.set_rx_rate(20_000_000).expect("rate");
    oracle.set_rx_frequency(1_000_000_000).expect("tunes");
    oracle
        .create_rx_stream_on(c"ci16", ALL_CHANNELS, SAMPLES)
        .expect("creates");
    oracle.sync_rx_stream(c"none").expect("syncs");
    oracle.start_rx_stream().expect("starts");
    oracle.board_at(1, stop_device_1);
    let before = oracle_lens(&oracle);
    assert_eq!(oracle.receive_rx_channels(100).map(|_| ()), Err(-110));
    let after = oracle_lens(&oracle);
    oracle.stop_rx_stream().expect("stops");
    let expected = oracle.close_all();

    let sims = Sims::new(boards(plain));
    let mut multi = sims.open(FrontEndSelection::Auto).expect("opens");
    multi
        .broadcast(|device| device.set_rx_sample_rate(20_000_000))
        .expect("rate");
    multi
        .broadcast(|device| device.set_rx_frequency(1_000_000_000))
        .expect("tunes");
    multi
        .create_streams(StreamsConfig::rx(RxConfig::new(
            SAMPLES,
            HostFormat::Ci16,
            WireFormat::Ci16,
        )))
        .expect("creates");
    multi.start_streams().expect("starts");
    stop_device_1(&mut sims.0[1].board());
    let rsl_before = sims.lens();
    let mut buffers = vec![vec![Complex::new(0, 0); SAMPLES as usize]; 3];
    let mut channels: Vec<&mut [Complex<i16>]> =
        buffers.iter_mut().map(Vec::as_mut_slice).collect();
    let result = multi.receive_into(&mut channels, Duration::from_millis(100));
    let Err(MultiError::Devices { step, outcomes }) = result else {
        panic!("the receive fails on device 1: {result:?}");
    };
    assert_eq!(step, MultiStep::Call);
    assert!(
        matches!(
            outcomes[..],
            [
                Outcome::Done(_),
                Outcome::Failed(Error::Timeout),
                Outcome::NotReached
            ]
        ),
        "{outcomes:?}"
    );
    let rsl_after = sims.lens();
    for board in 0..3 {
        assert_same_ops(
            &expected[board].trace()[before[board]..after[board]],
            &sims.0[board].board().trace()[rsl_before[board]..rsl_after[board]],
            &format!("board {board}"),
        );
    }
}

/// A `SimpleSync` master distributes its 50 MHz reference: libusdr's aggregate moves
/// every device to it at open (`/ll/mdev`), and so does rsl-usdr's.
#[test]
fn a_simplesync_master_moves_every_device_to_its_reference_like_libusdr() {
    let simplesync = |index, board: SimBoard| {
        if index == 0 {
            board.with_front_end(FrontEnd::SimpleSync)
        } else {
            board
        }
    };
    let oracle = Oracle::open_multi(boards(simplesync), c"").expect("libusdr opens");
    let opened = oracle_lens(&oracle);
    let expected = oracle.close_all();

    let sims = Sims::new(boards(simplesync));
    let multi = sims.open(FrontEndSelection::Auto).expect("rsl-usdr opens");
    assert_eq!(multi.master().front_end(), Some(FrontEndKind::SimpleSync));
    let rsl_opened = sims.lens();
    multi.close().expect("closes");
    for (board, revision) in REVISIONS.iter().enumerate() {
        assert_same_ops(
            &expected[board].trace()[..opened[board]],
            &without_thermal_gate(
                &sims.0[board].board().trace()[..rsl_opened[board]],
                *revision,
            ),
            &format!("board {board}"),
        );
    }
}

/// libusdr's aggregate needs its master's front-end node (`/ll/mdev`), which exists only
/// with a front end: with `fe=none` it opens every device, then fails `-ENOENT` and
/// powers them down. rsl-usdr opens; up to libusdr's failure, both did the same.
#[test]
fn libusdr_fails_an_aggregate_whose_master_has_no_front_end() {
    let Err(refused) = Oracle::open_multi(boards(plain), c"fe=none") else {
        panic!("libusdr fails the open");
    };
    assert_eq!(refused.errno, -2);

    let sims = Sims::new(boards(plain));
    let multi = sims.open(FrontEndSelection::None).expect("rsl-usdr opens");
    for (board, revision) in REVISIONS.iter().enumerate() {
        let opened = without_thermal_gate(sims.0[board].board().trace(), *revision);
        assert!(
            refused.boards[board].trace().starts_with(&opened),
            "board {board}: libusdr opened it as rsl-usdr does before failing"
        );
    }
    assert_eq!(multi.device_count(), 3);
}

/// libusdr's aggregate tells a stream's direction by an inverted test
/// (`strstr(sid, "rx/0") == 0`), so its RX stream reports itself as TX; on one device it
/// reports RX. rsl-usdr has no such report: each device's stream is the one it created.
#[test]
fn libusdr_reports_its_aggregate_rx_stream_as_tx() {
    let mut oracle = Oracle::open_multi(boards(plain), c"").expect("opens");
    oracle.set_rx_rate(20_000_000).expect("rate");
    oracle
        .create_rx_stream_on(c"ci16", ALL_CHANNELS, SAMPLES)
        .expect("creates");
    assert_eq!(
        oracle.rx_stream_kind_and_channels(),
        Ok((0, 3)),
        "USDR_DMS_TX"
    );
    drop(oracle);

    let mut single = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("opens");
    single.set_rx_rate(20_000_000).expect("rate");
    single.create_rx_stream(SAMPLES).expect("creates");
    assert_eq!(
        single.rx_stream_kind_and_channels(),
        Ok((1, 1)),
        "USDR_DMS_RX"
    );
}
