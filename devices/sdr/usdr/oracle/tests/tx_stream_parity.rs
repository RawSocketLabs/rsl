//! rsl-usdr's TX stream (create, start and sync, send with statistics, mute, stop) against
//! libusdr's `usdr_dms_*` calls and stream option: each call's operations, the bytes the
//! sim's TX core receives (libusdr converting with its own converters), and the
//! statistics both report. Also RX and TX in one group, libusdr's sync modes for it, and
//! the refusals.

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

use std::ffi::CStr;
use std::ops::Range;
use std::time::Duration;

use baseband::without_baseband_loop;
use num_complex::Complex;
use rsl_usdr::{
    Ci12, Device, Error, HostFormat, HostSample, RxConfig, StreamTime, StreamsConfig, SyncMode,
    TxConfig, TxMute, TxTelemetry, WireFormat,
};
use rsl_usdr_oracle::{Oracle, SendStat};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard, TxPacket};
use sim_bus::SimBus;

/// The sync unit's register, which the TX core's commands share.
const REG_SYNC: u32 = 14;
/// The sync unit's "off" (`ST_STOP` with the select bit).
const SYNC_OFF: u32 = 1 << 31;

/// A host sample as libusdr's converters read it.
trait LeBytes: HostSample {
    /// The sample's bytes.
    fn le_bytes(&self) -> Vec<u8>;
}

impl LeBytes for Complex<i16> {
    fn le_bytes(&self) -> Vec<u8> {
        [self.re.to_le_bytes(), self.im.to_le_bytes()].concat()
    }
}

impl LeBytes for Complex<f32> {
    fn le_bytes(&self) -> Vec<u8> {
        [self.re.to_le_bytes(), self.im.to_le_bytes()].concat()
    }
}

impl LeBytes for Ci12 {
    fn le_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

/// Packet `k`'s `n` samples in format `S`: a pattern with negative values and, for
/// `cf32`, values off the 16-bit grid that libusdr's conversion rounds.
trait Pattern: LeBytes + Sized {
    /// The samples.
    fn packet(k: usize, n: usize) -> Vec<Self>;
}

/// Sample `i` of packet `k` as a small signed number.
fn value(k: usize, i: usize, scale: usize) -> i32 {
    i32::try_from((k * 977 + i * scale) % 4001).expect("small") - 2000
}

impl Pattern for Complex<i16> {
    fn packet(k: usize, n: usize) -> Vec<Self> {
        let at = |i, scale| i16::try_from(value(k, i, scale) * 16).expect("fits");
        (0..n).map(|i| Self::new(at(i, 7), at(i, 13))).collect()
    }
}

impl Pattern for Complex<f32> {
    fn packet(k: usize, n: usize) -> Vec<Self> {
        #[expect(clippy::cast_precision_loss, reason = "small integers")]
        let at = |i, scale| value(k, i, scale) as f32 / 2001.5;
        (0..n).map(|i| Self::new(at(i, 7), at(i, 13))).collect()
    }
}

impl Pattern for Ci12 {
    fn packet(k: usize, n: usize) -> Vec<Self> {
        let byte = |i: usize, s: usize| u8::try_from((k * 31 + i * s) % 251).expect("small");
        (0..n)
            .map(|i| Ci12([byte(i, 3), byte(i, 5), byte(i, 11)]))
            .collect()
    }
}

/// One send: its time, or `None` for "now", and its samples, at most a packet.
type Send = (Option<u64>, u32);

/// What one side did: each step's operations, each send's statistics, what the TX core
/// played and the board's final trace.
struct Run {
    /// The board's trace after the run.
    trace: Vec<Op>,
    /// Each step's name and trace range.
    windows: Vec<(&'static str, Range<usize>)>,
    /// Each send's statistics, as (lhwtime, ktime, underruns, fifo).
    stats: Vec<Option<(u64, u32, u64, u8)>>,
    /// The packets the sim's TX core played.
    played: Vec<TxPacket>,
}

/// A TX scenario made the same way on both sides.
#[derive(Clone, Copy, Debug)]
struct Scenario {
    /// libusdr's data format.
    dformat: &'static CStr,
    /// The host format, rsl-usdr's side of `dformat`.
    host: HostFormat,
    /// Samples per packet.
    samples: u32,
    /// libusdr's sync type and rsl-usdr's mode.
    sync: (&'static CStr, SyncMode),
    /// With an RX stream in the group too.
    with_rx: bool,
    /// Tune the transmitter before creating, which powers it.
    tune_tx: bool,
    /// Mute both lanes between create and start, which the start then applies.
    mute_before_start: bool,
}

impl Scenario {
    /// A TX-only group free-running, `samples` per packet, in `dformat` (`host@ci16`).
    const fn tx(dformat: &'static CStr, host: HostFormat, samples: u32) -> Self {
        Self {
            dformat,
            host,
            samples,
            sync: (c"none", SyncMode::None),
            with_rx: false,
            tune_tx: true,
            mute_before_start: false,
        }
    }

    /// rsl-usdr's configuration for the scenario.
    fn config(&self) -> StreamsConfig {
        let tx = TxConfig::new(self.samples, self.host, WireFormat::Ci16);
        let config = if self.with_rx {
            StreamsConfig::rx(RxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16)).with_tx(tx)
        } else {
            StreamsConfig::tx(tx)
        };
        config.with_sync(self.sync.1)
    }
}

/// A board the scenarios use.
fn board() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3)
}

/// The status words for send `k`: drop counters that wrap, in the `PCIe` layout, except
/// every fifth send, which reports the USB layout.
fn status(k: usize) -> [u32; 4] {
    let k32 = u32::try_from(k).expect("small");
    let (fe, dma, fifo) = ((k32 * 37) % 256, (k32 * 101) % 256, (k32 * 7) % 256);
    if k % 5 == 0 {
        [8 | fe << 8, ((k32 * 300) & 0xffff) << 16, 0x1000 + k32, k32]
    } else {
        [0, fifo << 16 | fe << 8 | dma, 0x1000 + k32, 0xabc0 + k32]
    }
}

/// The trace range `step` adds.
fn window(trace_len: impl Fn() -> usize, step: impl FnOnce()) -> Range<usize> {
    let start = trace_len();
    step();
    start..trace_len()
}

/// libusdr: rate, RX and TX tunes, create, start and sync, the sends (setting the core's
/// status before each), a mute, then stop and destroy.
fn on_libusdr<S: Pattern>(scenario: Scenario, sends: &[Send]) -> Run {
    let mut oracle = Oracle::open(board()).expect("libusdr opens");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle.set_rx_frequency(1_000_000_000).expect("tunes RX");
    if scenario.tune_tx {
        oracle.set_tx_frequency(1_500_000_000).expect("tunes TX");
    }
    let mut windows = Vec::new();
    let start = oracle.trace_len();
    if scenario.with_rx {
        oracle
            .create_rx_stream_as(c"ci16", 1024)
            .expect("libusdr creates RX");
    }
    oracle
        .create_tx_stream_as(scenario.dformat, scenario.samples)
        .expect("libusdr creates TX");
    windows.push(("create", start..oracle.trace_len()));
    let start = oracle.trace_len();
    if scenario.mute_before_start {
        oracle
            .set_tx_mute(3)
            .expect("libusdr mutes before starting");
    }
    if scenario.with_rx {
        oracle.start_rx_stream().expect("libusdr starts RX");
    }
    oracle.start_tx_stream().expect("libusdr starts TX");
    oracle.sync_streams(scenario.sync.0).expect("libusdr syncs");
    windows.push(("start", start..oracle.trace_len()));
    let mut stats = Vec::new();
    for (k, &(time, count)) in sends.iter().enumerate() {
        oracle.board(|board| board.set_tx_status(status(k)));
        let samples = usize::try_from(count).expect("fits");
        let bytes: Vec<u8> = S::packet(k, samples).iter().flat_map(S::le_bytes).collect();
        let start = oracle.trace_len();
        let report = oracle
            .send_tx(&bytes, count, time.unwrap_or(u64::MAX), 100)
            .expect("libusdr sends");
        windows.push(("send", start..oracle.trace_len()));
        stats.push(report.map(
            |SendStat {
                 lhwtime,
                 ktime,
                 underruns,
                 fifo_used,
                 ..
             }| (lhwtime, ktime, u64::from(underruns), fifo_used),
        ));
    }
    let start = oracle.trace_len();
    oracle.set_tx_mute(3).expect("libusdr mutes");
    windows.push(("mute", start..oracle.trace_len()));
    let start = oracle.trace_len();
    if scenario.with_rx {
        oracle.stop_rx_stream().expect("libusdr stops RX");
    }
    oracle.stop_tx_stream().expect("libusdr stops TX");
    if scenario.with_rx {
        oracle.destroy_rx_stream().expect("libusdr destroys RX");
    }
    oracle.destroy_tx_stream().expect("libusdr destroys TX");
    windows.push(("stop", start..oracle.trace_len()));
    let board = oracle.close();
    Run {
        trace: board.trace().to_vec(),
        windows,
        stats,
        played: board.tx_packets().to_vec(),
    }
}

/// rsl-usdr: the same through `create_streams`, `start_streams`, `send_with_stat`,
/// `set_tx_mute` and `stop_streams`.
fn on_rsl<S: Pattern>(scenario: Scenario, sends: &[Send]) -> Run {
    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    device.set_rx_frequency(1_000_000_000).expect("tunes RX");
    if scenario.tune_tx {
        device.set_tx_frequency(1_500_000_000).expect("tunes TX");
    }
    let trace_len = || sim.board().trace().len();
    let mut windows = vec![
        (
            "create",
            window(trace_len, || {
                device
                    .create_streams(scenario.config())
                    .expect("rsl-usdr creates");
            }),
        ),
        (
            "start",
            window(trace_len, || {
                if scenario.mute_before_start {
                    device
                        .set_tx_mute(TxMute::Both)
                        .expect("rsl-usdr mutes before starting");
                }
                device.start_streams().expect("rsl-usdr starts");
            }),
        ),
    ];
    let mut stats = Vec::new();
    for (k, &(time, count)) in sends.iter().enumerate() {
        sim.board().set_tx_status(status(k));
        let packet = S::packet(k, usize::try_from(count).expect("fits"));
        let mut report = None;
        let range = window(trace_len, || {
            report = device
                .send_with_stat(
                    &[&packet],
                    time.map(StreamTime::from_samples),
                    Duration::from_millis(100),
                )
                .expect("rsl-usdr sends");
        });
        windows.push(("send", range));
        stats.push(report.map(
            |TxTelemetry {
                 hardware_time,
                 kernel_time,
                 underruns,
                 fifo_used,
                 ..
             }| (u64::from(hardware_time), kernel_time, underruns, fifo_used),
        ));
    }
    windows.push((
        "mute",
        window(trace_len, || {
            device.set_tx_mute(TxMute::Both).expect("rsl-usdr mutes");
        }),
    ));
    windows.push((
        "stop",
        window(trace_len, || device.stop_streams().expect("rsl-usdr stops")),
    ));
    let board = sim.board();
    Run {
        trace: board.trace().to_vec(),
        windows,
        stats,
        played: board.tx_packets().to_vec(),
    }
}

/// Runs `scenario` on both sides and asserts every step's operations match (libusdr's
/// RX create without its baseband loop, the whitelisted divergence), returning both.
fn assert_windows_match<S: Pattern>(scenario: Scenario, sends: &[Send]) -> (Run, Run) {
    let expected = on_libusdr::<S>(scenario, sends);
    let actual = on_rsl::<S>(scenario, sends);
    assert_eq!(
        expected.windows.len(),
        actual.windows.len(),
        "{scenario:?}: the same steps"
    );
    for ((name, expected_window), (_, actual_window)) in
        expected.windows.iter().zip(&actual.windows)
    {
        let mut expected_ops = expected.trace[expected_window.clone()].to_vec();
        if *name == "create" && scenario.with_rx {
            (expected_ops, _) = without_baseband_loop(&expected_ops, false);
        }
        assert_eq!(
            expected_ops,
            actual.trace[actual_window.clone()],
            "{scenario:?}: the {name} step's operations differ"
        );
    }
    (expected, actual)
}

/// The sends of the format tests: timed packets and "now" ones, whole 1024-sample packets
/// and partial ones of 1 sample and of one more than rsl-usdr's 512-sample conversion
/// chunk.
const SENDS: [Send; 4] = [
    (Some(0x10_0000), 1024),
    (None, 1),
    (Some(0x7fff_0000_1234), 513),
    (None, 1024),
];

/// Every host format libusdr converts to the TX core's `ci16`: the same operations in
/// every step, and the TX core receives the same bytes at the same times.
#[test]
fn every_host_format_matches_libusdr_operations_and_bytes() {
    let scenarios = [
        Scenario::tx(c"ci16@ci16", HostFormat::Ci16, 1024),
        Scenario::tx(c"cf32@ci16", HostFormat::Cf32, 1024),
        Scenario::tx(c"ci12@ci16", HostFormat::Ci12, 1024),
    ];
    for scenario in scenarios {
        let (expected, actual) = match scenario.host {
            HostFormat::Ci16 => assert_windows_match::<Complex<i16>>(scenario, &SENDS),
            HostFormat::Cf32 => assert_windows_match::<Complex<f32>>(scenario, &SENDS),
            _ => assert_windows_match::<Ci12>(scenario, &SENDS),
        };
        assert_eq!(
            expected.played.len(),
            SENDS.len(),
            "{scenario:?}: all played"
        );
        assert_eq!(
            expected.played, actual.played,
            "{scenario:?}: the bytes and times the TX core received"
        );
    }
}

/// Creating a TX stream with the transmitter off powers it, as libusdr's streaming-up
/// does, and stopping powers it down again.
#[test]
fn an_untuned_transmitter_is_powered_by_the_stream_like_libusdr() {
    let mut scenario = Scenario::tx(c"ci16@ci16", HostFormat::Ci16, 512);
    scenario.tune_tx = false;
    let (expected, _) = assert_windows_match::<Complex<i16>>(scenario, &[(None, 512)]);
    assert_eq!(expected.played.len(), 1);
}

/// Muting before the start: the stream keeps the mute and its start applies it, on both
/// sides.
#[test]
fn a_mute_before_start_is_applied_by_the_start_like_libusdr() {
    let mut scenario = Scenario::tx(c"ci16@ci16", HostFormat::Ci16, 256);
    scenario.mute_before_start = true;
    let (expected, _) = assert_windows_match::<Complex<i16>>(scenario, &[(None, 256)]);
    let start = &expected.trace[expected.windows[1].1.clone()];
    assert_eq!(
        start.last(),
        Some(&Op::RegWrite {
            addr: REG_SYNC,
            value: 1 << 31 | 7 << 16,
        }),
        "the free-run sync comes last"
    );
    assert!(
        start.contains(&Op::RegWrite {
            addr: REG_SYNC,
            value: 0x303,
        }),
        "the start carries both lanes muted"
    );
}

/// The statistics `usdr_dms_send_stat` reports: none for the 32 buffers the driver starts
/// with, then each freed buffer's status, the drop counters extended across their wraps
/// (and libusdr's USB-layout reading, every fifth), the same on both sides.
#[test]
fn send_statistics_match_libusdr_across_counter_wraps() {
    let scenario = Scenario::tx(c"ci16@ci16", HostFormat::Ci16, 256);
    let sends: Vec<Send> = (0..48).map(|k| (Some(1000 + 256 * k), 256)).collect();
    let (expected, actual) = assert_windows_match::<Complex<i16>>(scenario, &sends);
    assert!(
        expected.stats[..32].iter().all(Option::is_none),
        "no statistics before a buffer comes back"
    );
    assert!(
        expected.stats[32..].iter().all(Option::is_some),
        "statistics with every freed buffer"
    );
    assert_eq!(expected.stats, actual.stats, "the statistics");
}

/// A stream not started takes 32 packets, then times out; libusdr reads the TX core's
/// status for its log on the way, and so does rsl-usdr.
#[test]
fn a_full_unstarted_stream_times_out_like_libusdr() {
    let mut oracle = Oracle::open(board()).expect("libusdr opens");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle
        .create_tx_stream_as(c"ci16@ci16", 256)
        .expect("libusdr creates TX");
    let bytes = vec![0; 1024];
    for _ in 0..32 {
        oracle
            .send_tx(&bytes, 256, u64::MAX, 10)
            .expect("a free buffer");
    }
    let start = oracle.trace_len();
    assert_eq!(oracle.send_tx(&bytes, 256, u64::MAX, 10), Err(-110));
    let end = oracle.trace_len();
    let expected = oracle.close().trace()[start..end].to_vec();

    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let config = TxConfig::new(256, HostFormat::Ci16, WireFormat::Ci16);
    device
        .create_streams(StreamsConfig::tx(config))
        .expect("creates");
    let packet = vec![Complex::new(0_i16, 0); 256];
    let timeout = Duration::from_millis(10);
    for _ in 0..32 {
        device
            .send(&[&packet], None, timeout)
            .expect("a free buffer");
    }
    let start = sim.board().trace().len();
    assert!(matches!(
        device.send(&[&packet], None, timeout),
        Err(Error::Timeout)
    ));
    assert_eq!(sim.board().trace()[start..], expected, "the status read");
    assert_eq!(expected.len(), 4, "four status words");
}

/// RX and TX in one group, started together with libusdr's "all" (`ST_DELAYADCDAC`):
/// creating TX holds the sync unit off again, both start, and TX plays (the sim releases it only with RX running).
#[test]
fn a_duplex_group_matches_libusdr() {
    let scenario = Scenario {
        sync: (c"all", SyncMode::All),
        with_rx: true,
        ..Scenario::tx(c"cf32@ci16", HostFormat::Cf32, 1024)
    };
    let (expected, actual) = assert_windows_match::<Complex<f32>>(scenario, &SENDS);
    assert_eq!(expected.played, actual.played, "the TX bytes");
    let create = &expected.trace[expected.windows[0].1.clone()];
    let sync_offs = create
        .iter()
        .filter(|op| {
            **op == Op::RegWrite {
                addr: REG_SYNC,
                value: SYNC_OFF,
            }
        })
        .count();
    assert_eq!(sync_offs, 2, "RX's create and TX's each write sync off");
    let start = &expected.trace[expected.windows[1].1.clone()];
    assert_eq!(
        start.last(),
        Some(&Op::RegWrite {
            addr: REG_SYNC,
            value: 1 << 31 | 6 << 16,
        }),
        "the duplex start comes last"
    );
}

/// Each sync mode libusdr offers for the uSDR's groups, with and without RX: the same
/// start, so the same trigger.
#[test]
fn sync_modes_match_libusdr_for_every_group() {
    let modes = [
        (c"none", SyncMode::None),
        (c"all", SyncMode::All),
        (c"any", SyncMode::Any),
        (c"1pps", SyncMode::OnePps),
    ];
    for with_rx in [false, true] {
        for sync in modes {
            let scenario = Scenario {
                sync,
                with_rx,
                ..Scenario::tx(c"ci16@ci16", HostFormat::Ci16, 256)
            };
            // A held group takes sends but plays none; one send keeps every mode alike.
            assert_windows_match::<Complex<i16>>(scenario, &[(None, 256)]);
        }
    }
}

/// libusdr's TX core takes `ci16` only, but libusdr refuses `ci12` on the wire only
/// after powering the transmitter and reading the gateware's build; rsl-usdr refuses it
/// before any operation (whitelisted divergence).
#[test]
fn a_twelve_bit_tx_wire_is_refused_before_libusdr_would_power_up() {
    let mut oracle = Oracle::open(board()).expect("libusdr opens");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    let start = oracle.trace_len();
    assert_eq!(oracle.create_tx_stream_as(c"ci16@ci12", 1024), Err(-22));
    assert!(oracle.trace_len() > start, "libusdr operated first");
    drop(oracle.close());

    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let start = sim.board().trace().len();
    let config = TxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci12);
    assert!(matches!(
        device.create_streams(StreamsConfig::tx(config)),
        Err(Error::UnsupportedWireFormat(WireFormat::Ci12))
    ));
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// Refusals both drivers make at the same point, with the same operations before it: a
/// gateware build older than libusdr's minimum, and a packet over the core's FIFO. A size
/// of 0, which libusdr takes as the transport's largest buffer, rsl-usdr refuses before
/// the bus (`tests/tx_stream.rs`, a whitelisted divergence).
#[test]
fn refused_creates_match_libusdr_up_to_the_refusal() {
    let old_build = 0xd2b1_0c08;
    for (build, samples, errno) in [(old_build, 1024, -104), (0xd2b1_0c09, 31_745, -22)] {
        let mut oracle = Oracle::open(board().with_gateware_build(build)).expect("libusdr opens");
        oracle.set_rx_rate(20_000_000).expect("sets the rate");
        let start = oracle.trace_len();
        assert_eq!(oracle.create_tx_stream_as(c"ci16", samples), Err(errno));
        let end = oracle.trace_len();
        let expected = oracle.close().trace()[start..end].to_vec();

        let sim = SimBus::new(board().with_gateware_build(build));
        let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
        device
            .set_rx_sample_rate(20_000_000)
            .expect("sets the rate");
        let start = sim.board().trace().len();
        let config = TxConfig::new(samples, HostFormat::Ci16, WireFormat::Ci16);
        let result = device.create_streams(StreamsConfig::tx(config));
        match errno {
            -104 => assert!(matches!(result, Err(Error::OutdatedGateware(b)) if b == build)),
            _ => assert!(matches!(result, Err(Error::UnsupportedPacketSize(s)) if s == samples)),
        }
        assert_eq!(
            sim.board().trace()[start..],
            expected,
            "build {build:#x}, {samples} samples: the operations before the refusal"
        );
    }
}
