//! rsl-usdr's stream group (create, sync and start, receive, stop) against libusdr's
//! `usdr_dms_*` calls, for every host@wire format pair the uSDR's RX front end offers:
//! each call's operations, and the host buffers libusdr's own converters fill, byte for
//! byte.

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
    Ci12, Device, Error, HostFormat, HostSample, RxChannels, RxConfig, RxMetadata, StreamsConfig,
    SyncMode, WireFormat,
};
use rsl_usdr_oracle::{Oracle, RecvInfo};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

/// The RX front end's command register.
const REG_RX_FRONT_END: u32 = 0x1_0100;

/// A host buffer element as little-endian bytes, as libusdr's converters write it.
trait LeBytes: HostSample {
    /// The element's bytes.
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

impl LeBytes for i16 {
    fn le_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl LeBytes for f32 {
    fn le_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

/// One stream scenario, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
struct Scenario {
    /// libusdr's data format string.
    dformat: &'static CStr,

    /// libusdr's sync type.
    synctype: &'static CStr,

    /// libusdr's channel mask: 1 for channel 0, 2 for channel 1, 3 for both.
    channel_mask: u64,

    /// rsl-usdr's configuration for the same stream.
    config: StreamsConfig,
}

impl Scenario {
    /// A channel-0 stream of `samples` per packet in `dformat`, which is `host@wire`,
    /// started unsynchronised.
    fn new(dformat: &'static CStr, samples: u32, host: HostFormat, wire: WireFormat) -> Self {
        Self {
            dformat,
            synctype: c"none",
            channel_mask: 1,
            config: StreamsConfig::rx(RxConfig::new(samples, host, wire)),
        }
    }

    /// This scenario on channel 1 (libusdr's `aq` for a real wire, lanes 2 and 3 for a
    /// complex one).
    fn on_channel1(self) -> Self {
        self.on(2, RxChannels::Channel1)
    }

    /// This scenario on libusdr's channel `mask`, rsl-usdr's `channels`.
    fn on(mut self, mask: u64, channels: RxChannels) -> Self {
        self.channel_mask = mask;
        let rx = self.config.rx.expect("an RX stream");
        self.config = StreamsConfig::rx(rx.with_channels(channels));
        self
    }

    /// Channels the stream carries.
    fn channels(&self) -> usize {
        match self.config.rx.expect("an RX stream").channels {
            RxChannels::Both => 2,
            _ => 1,
        }
    }

    /// This scenario, started with libusdr's `synctype`, rsl-usdr's `sync`.
    fn synced(mut self, synctype: &'static CStr, sync: SyncMode) -> Self {
        self.synctype = synctype;
        self.config = self.config.with_sync(sync);
        self
    }

    /// Samples per packet.
    fn samples(&self) -> usize {
        let rx = self.config.rx.expect("an RX stream");
        usize::try_from(rx.samples_per_packet).expect("fits")
    }
}

/// A received packet's host bytes, one buffer per channel, and its time, samples, lost
/// packets and transport status (libusdr's `fsymtime`, `totsyms`, `totlost`, `extra`).
type Packet = (Vec<Vec<u8>>, (u64, usize, u64, u64));

/// What one side did: each step's operations and each packet received.
struct Run {
    /// The board's trace after the run.
    trace: Vec<Op>,
    /// Each step's name and trace range: create, start (sync then start), each receive,
    /// stop.
    windows: Vec<(&'static str, Range<usize>)>,
    /// Each received packet.
    packets: Vec<Packet>,
}

/// The trace range `step` adds, `trace_len` reading the trace's length.
fn window(trace_len: impl Fn() -> usize, step: impl FnOnce()) -> Range<usize> {
    let start = trace_len();
    step();
    start..trace_len()
}

/// libusdr: rate, frequency, create, sync and START, `receives` packets, STOP and destroy.
fn on_libusdr(scenario: Scenario, receives: usize) -> Run {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle.set_rx_frequency(1_000_000_000).expect("tunes");
    let samples = u32::try_from(scenario.samples()).expect("fits");
    let mut windows = Vec::new();
    let start = oracle.trace_len();
    oracle
        .create_rx_stream_on(scenario.dformat, scenario.channel_mask, samples)
        .expect("libusdr creates the stream");
    windows.push(("create", start..oracle.trace_len()));
    let start = oracle.trace_len();
    oracle
        .sync_rx_stream(scenario.synctype)
        .expect("libusdr syncs");
    oracle.start_rx_stream().expect("libusdr starts");
    windows.push(("start", start..oracle.trace_len()));
    let mut packets = Vec::new();
    for _ in 0..receives {
        let start = oracle.trace_len();
        let (bytes, info) = oracle.receive_rx_channels(100).expect("a packet");
        windows.push(("receive", start..oracle.trace_len()));
        let RecvInfo {
            fsymtime,
            totsyms,
            totlost,
            extra,
            ..
        } = info;
        let summary = (
            fsymtime,
            usize::try_from(totsyms).expect("fits"),
            u64::from(totlost),
            extra,
        );
        packets.push((bytes, summary));
    }
    let start = oracle.trace_len();
    oracle.stop_rx_stream().expect("libusdr stops");
    oracle.destroy_rx_stream().expect("libusdr destroys");
    windows.push(("stop", start..oracle.trace_len()));
    Run {
        trace: oracle.close().trace().to_vec(),
        windows,
        packets,
    }
}

/// Receives one packet into `channels` buffers of `S`, returning their bytes and the
/// packet's information.
fn receive<S: LeBytes + Default>(device: &mut Device, samples: usize, channels: usize) -> Packet {
    let mut buffers = vec![vec![S::default(); samples]; channels];
    let mut slices: Vec<&mut [S]> = buffers.iter_mut().map(Vec::as_mut_slice).collect();
    let RxMetadata {
        time,
        samples,
        lost_packets,
        transport_status,
        ..
    } = device
        .receive_into(&mut slices, Duration::from_millis(100))
        .expect("a packet");
    let bytes = buffers
        .iter()
        .map(|buffer| buffer.iter().flat_map(LeBytes::le_bytes).collect())
        .collect();
    (
        bytes,
        (time.samples(), samples, lost_packets, transport_status),
    )
}

/// rsl-usdr: the same calls through `create_streams`, `start_streams`, `receive_into` and
/// `stop_streams`.
fn on_rsl(scenario: Scenario, receives: usize) -> Run {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    device.set_rx_frequency(1_000_000_000).expect("tunes");
    let trace_len = || sim.board().trace().len();
    let mut windows = vec![
        (
            "create",
            window(trace_len, || {
                device
                    .create_streams(scenario.config)
                    .expect("rsl-usdr creates the stream");
            }),
        ),
        (
            "start",
            window(trace_len, || {
                device.start_streams().expect("rsl-usdr starts");
            }),
        ),
    ];
    let host = scenario.config.rx.expect("an RX stream").host;
    let mut packets = Vec::new();
    for _ in 0..receives {
        let mut packet = None;
        let range = window(trace_len, || {
            let (samples, channels) = (scenario.samples(), scenario.channels());
            packet = Some(match host {
                HostFormat::Ci16 => receive::<Complex<i16>>(&mut device, samples, channels),
                HostFormat::Cf32 => receive::<Complex<f32>>(&mut device, samples, channels),
                HostFormat::Ci12 => receive::<Ci12>(&mut device, samples, channels),
                HostFormat::I16 => receive::<i16>(&mut device, samples, channels),
                HostFormat::F32 => receive::<f32>(&mut device, samples, channels),
                other => panic!("{other:?} has no buffer type here"),
            });
        });
        windows.push(("receive", range));
        packets.push(packet.expect("received"));
    }
    let stopped = window(trace_len, || {
        device.stop_streams().expect("rsl-usdr stops");
    });
    windows.push(("stop", stopped));
    let trace = sim.board().trace().to_vec();
    Run {
        trace,
        windows,
        packets,
    }
}

/// Asserts both sides made the same operations in each step, libusdr's create without
/// its baseband loop (the whitelisted divergence), and returns both runs.
fn assert_windows_match(scenario: Scenario, receives: usize) -> (Run, Run) {
    let expected = on_libusdr(scenario, receives);
    let actual = on_rsl(scenario, receives);
    for ((name, expected_window), (_, actual_window)) in
        expected.windows.iter().zip(&actual.windows)
    {
        let mut expected_ops = expected.trace[expected_window.clone()].to_vec();
        if *name == "create" {
            (expected_ops, _) = without_baseband_loop(&expected_ops, false);
        }
        let actual_ops = &actual.trace[actual_window.clone()];
        assert_eq!(
            expected_ops, actual_ops,
            "{scenario:?}: the {name} step's operations differ"
        );
    }
    (expected, actual)
}

/// Every host@wire pair, at a packet size whose bursts need no padding in either wire
/// format (1024 samples: 4096 bytes of `ci16`, 3072 of `ci12`), sync "none": the same
/// operations, the same host bytes from libusdr's converters, the same packet times.
#[test]
fn every_format_pair_matches_libusdr_operations_and_bytes() {
    use HostFormat as H;
    use WireFormat as W;
    let pairs: [(&CStr, H, W); 6] = [
        (c"ci16", H::Ci16, W::Ci16),
        (c"cf32", H::Cf32, W::Ci16),
        (c"ci12@ci16", H::Ci12, W::Ci16),
        (c"ci12", H::Ci12, W::Ci12),
        (c"ci16@ci12", H::Ci16, W::Ci12),
        (c"cf32@ci12", H::Cf32, W::Ci12),
    ];
    for (dformat, host, wire) in pairs {
        let scenario = Scenario::new(dformat, 1024, host, wire);
        let (expected, actual) = assert_windows_match(scenario, 2);
        assert_eq!(
            expected.packets, actual.packets,
            "{scenario:?}: received bytes and packet information"
        );
        assert!(
            actual.packets.iter().all(|(_, info)| info.3 != 0),
            "{scenario:?}: a nonzero DMA status, so the comparison above covers the word"
        );
    }
}

/// Every real host@wire pair libusdr converts, on channel 0 (lane 0, I) and channel 1
/// (lane 1, Q): the burst format's lanes, the same operations, the same host bytes and
/// packet information. 1024 real samples fill whole words in either wire format.
#[test]
fn every_real_format_pair_on_either_lane_matches_libusdr() {
    use HostFormat as H;
    use WireFormat as W;
    let pairs: [(&CStr, H, W); 4] = [
        (c"i16", H::I16, W::I16),
        (c"f32@i16", H::F32, W::I16),
        (c"i16@i12", H::I16, W::I12),
        (c"f32@i12", H::F32, W::I12),
    ];
    for (dformat, host, wire) in pairs {
        for scenario in [
            Scenario::new(dformat, 1024, host, wire),
            Scenario::new(dformat, 1024, host, wire).on_channel1(),
        ] {
            let (expected, actual) = assert_windows_match(scenario, 2);
            assert_eq!(
                expected.packets, actual.packets,
                "{scenario:?}: received bytes and packet information"
            );
        }
    }
}

/// A real 12-bit packet whose burst ends in a stub: 1022 samples are 1533 bytes in a
/// 1536-byte burst. The operations and the packet's samples match; whitelisted
/// divergence, as for `ci12`: libusdr converts the 3-byte stub into two more samples,
/// where rsl-usdr's packet ends at its own.
#[test]
fn a_padded_real_twelve_bit_packet_matches_libusdr_up_to_its_stub() {
    let scenario = Scenario::new(c"i16@i12", 1022, HostFormat::I16, WireFormat::I12);
    let (expected, actual) = assert_windows_match(scenario, 1);
    let (expected_bytes, expected_info) = &expected.packets[0];
    let (actual_bytes, actual_info) = &actual.packets[0];
    let (expected_bytes, actual_bytes) = (&expected_bytes[0], &actual_bytes[0]);
    assert_eq!(expected_info, actual_info, "packet information");
    assert_eq!(
        expected_bytes.len(),
        actual_bytes.len() + 4,
        "libusdr's buffer holds the stub's two samples too"
    );
    assert_eq!(
        &expected_bytes[..actual_bytes.len()],
        &actual_bytes[..],
        "the packet's samples"
    );
}

/// libusdr's converter lookup falls back to a byte copy for a real wire into a complex
/// host buffer, so it creates such a stream; rsl-usdr refuses it before any operation
/// (whitelisted divergence).
#[test]
fn a_complex_host_on_a_real_wire_is_refused_where_libusdr_copies_bytes() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle
        .create_rx_stream_as(c"ci16@i16", 1024)
        .expect("libusdr creates a byte-copying stream");
    drop(oracle.close());

    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let start = sim.board().trace().len();
    let config = StreamsConfig::rx(RxConfig::new(1024, HostFormat::Ci16, WireFormat::I16));
    assert!(matches!(
        device.create_streams(config),
        Err(Error::NoConversion {
            host: HostFormat::Ci16,
            wire: WireFormat::I16,
            ..
        })
    ));
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// The 12-bit wire format splits a 16384-sample packet into two bursts by its own word
/// count, and the samples still arrive in order.
#[test]
fn a_two_burst_twelve_bit_stream_matches_libusdr() {
    let scenario = Scenario::new(c"cf32@ci12", 16384, HostFormat::Cf32, WireFormat::Ci12);
    let (expected, actual) = assert_windows_match(scenario, 2);
    assert_eq!(expected.packets, actual.packets, "received bytes");
}

/// The 12-bit format bits reach the front end: libusdr's burst-format word for `ci12`
/// differs from `ci16`'s in its sample format, and rsl-usdr writes the same.
#[test]
fn the_twelve_bit_burst_format_is_libusdrs() {
    let scenario = Scenario::new(c"ci16@ci12", 1024, HostFormat::Ci16, WireFormat::Ci12);
    let (expected, _) = assert_windows_match(scenario, 0);
    let burst_format = expected.trace[expected.windows[0].1.clone()]
        .iter()
        .find_map(|op| match op {
            Op::RegWrite {
                addr: REG_RX_FRONT_END,
                value,
            } if value >> 28 == 1 => Some(*value),
            _ => None,
        })
        .expect("create writes the burst format");
    // 21 bursts in the FIFO, 384 words less one, channel 0's lanes, `IFMT_12BIT`.
    assert_eq!(burst_format, 0x1000_0000 | 21 << 18 | 383 << 5 | 1 << 2 | 2);
}

/// libusdr's "all" sync with RX alone waits on the RX front end's activity; rsl-usdr's
/// `SyncMode::All` writes the same, and the stream flows.
#[test]
fn an_all_synchronised_stream_matches_libusdr() {
    let scenario = Scenario::new(c"cf32@ci12", 1024, HostFormat::Cf32, WireFormat::Ci12)
        .synced(c"all", SyncMode::All);
    let (expected, actual) = assert_windows_match(scenario, 2);
    assert_eq!(expected.packets, actual.packets, "received bytes");
}

/// libusdr's single-direction "rx" trigger releases an RX-only stream on its own
/// activity, as "all" does; "tx" and the generated SYSREF hold it in the sim (no TX stream
/// runs, no pulse is raised), and rsl-usdr's modes write the same triggers.
#[test]
fn single_direction_and_generated_triggers_match_libusdr_for_rx() {
    let scenario = Scenario::new(c"ci16", 1024, HostFormat::Ci16, WireFormat::Ci16)
        .synced(c"rx", SyncMode::RxActivity);
    let (expected, actual) = assert_windows_match(scenario, 2);
    assert_eq!(expected.packets, actual.packets, "received bytes");
    for (synctype, sync) in [
        (c"tx", SyncMode::TxActivity),
        (c"sysref+gen", SyncMode::GeneratedSysref),
    ] {
        let scenario =
            Scenario::new(c"ci16", 1024, HostFormat::Ci16, WireFormat::Ci16).synced(synctype, sync);
        let (expected, _) = assert_windows_match(scenario, 0);
        let mode = expected.trace[expected.windows[1].1.clone()]
            .iter()
            .find_map(|op| match op {
                Op::RegWrite { addr: 14, value } => Some(*value),
                _ => None,
            })
            .expect("start writes the sync unit");
        let expected_mode = if sync == SyncMode::TxActivity { 3 } else { 9 };
        assert_eq!(mode, 1 << 31 | expected_mode << 16, "{synctype:?}");
    }
}

/// A packet whose 12-bit burst ends in a stub: the operations match, and so do the
/// packet's samples. Whitelisted divergence (rsl-usdr's documented stub skipping):
/// libusdr converts the whole block, so its buffer holds one more sample, converted from
/// the zero stub; rsl-usdr's ends at the packet.
#[test]
fn a_padded_twelve_bit_packet_matches_libusdr_up_to_its_stub() {
    // 1023 samples are 3069 bytes in a 3072-byte burst: a 3-byte stub, one ci12 sample.
    let scenario = Scenario::new(c"ci16@ci12", 1023, HostFormat::Ci16, WireFormat::Ci12);
    let (expected, actual) = assert_windows_match(scenario, 1);
    let (expected_bytes, expected_info) = &expected.packets[0];
    let (actual_bytes, actual_info) = &actual.packets[0];
    let (expected_bytes, actual_bytes) = (&expected_bytes[0], &actual_bytes[0]);
    assert_eq!(expected_info, actual_info, "packet information");
    assert_eq!(
        expected_bytes.len(),
        actual_bytes.len() + 4,
        "libusdr's buffer holds the stub's sample too"
    );
    assert_eq!(
        &expected_bytes[..actual_bytes.len()],
        &actual_bytes[..],
        "the packet's samples"
    );
    assert_eq!(
        expected_bytes[actual_bytes.len()..],
        [0; 4],
        "libusdr's extra sample is the zero stub"
    );
}

/// libusdr's uSDR sync core has no "extall": it creates the stream, then refuses the
/// sync without an operation. rsl-usdr refuses `SyncMode::ExternalAll` at create, before
/// any operation.
#[test]
fn external_sync_is_refused_by_both_before_any_sync_operation() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle
        .create_rx_stream_as(c"cf32@ci12", 1024)
        .expect("libusdr creates the stream");
    let start = oracle.trace_len();
    assert_eq!(oracle.sync_rx_stream(c"extall"), Err(-22), "-EINVAL");
    assert_eq!(oracle.trace_len(), start, "libusdr's sync wrote nothing");
    drop(oracle.close());

    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let start = sim.board().trace().len();
    let config = StreamsConfig::rx(RxConfig::new(1024, HostFormat::Cf32, WireFormat::Ci12))
        .with_sync(SyncMode::ExternalAll);
    assert!(
        matches!(
            device.create_streams(config),
            Err(Error::UnsupportedSync(SyncMode::ExternalAll))
        ),
        "rsl-usdr refuses external sync"
    );
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// The second complex channel, alone (lanes 2 and 3, `IFMT_CH_32xx`) and with the first
/// (all four lanes, `IFMT_CH_3210`, split by libusdr's one-to-two converters), for every
/// host@wire pair libusdr converts for it: the same operations, burst format and host
/// bytes. 1024 samples of both channels fill whole words in either wire format.
#[test]
fn complex_channel_1_and_both_channels_match_libusdr() {
    use HostFormat as H;
    use WireFormat as W;
    let pairs: [(&CStr, H, W, bool); 6] = [
        (c"ci16", H::Ci16, W::Ci16, true),
        (c"cf32", H::Cf32, W::Ci16, true),
        (c"ci12", H::Ci12, W::Ci12, false),
        (c"ci16@ci12", H::Ci16, W::Ci12, true),
        (c"cf32@ci12", H::Cf32, W::Ci12, true),
        (c"ci12@ci16", H::Ci12, W::Ci16, false),
    ];
    for (dformat, host, wire, splits) in pairs {
        let alone = Scenario::new(dformat, 1024, host, wire).on_channel1();
        let both = Scenario::new(dformat, 1024, host, wire).on(3, RxChannels::Both);
        let scenarios = if splits {
            vec![alone, both]
        } else {
            vec![alone]
        };
        for scenario in scenarios {
            let (expected, actual) = assert_windows_match(scenario, 2);
            assert_eq!(
                expected.packets, actual.packets,
                "{scenario:?}: received bytes and packet information"
            );
        }
    }
}

/// The lanes reach the front end: channel 1 alone is `IFMT_CH_32xx` (5), both channels
/// `IFMT_CH_3210` (0) with a burst of twice the words.
#[test]
fn the_second_channels_burst_formats_are_libusdrs() {
    let burst_format = |scenario: Scenario| {
        let (expected, _) = assert_windows_match(scenario, 0);
        expected.trace[expected.windows[0].1.clone()]
            .iter()
            .find_map(|op| match op {
                Op::RegWrite {
                    addr: REG_RX_FRONT_END,
                    value,
                } if value >> 28 == 1 => Some(*value),
                _ => None,
            })
            .expect("create writes the burst format")
    };
    let alone = Scenario::new(c"ci16", 1024, HostFormat::Ci16, WireFormat::Ci16).on_channel1();
    // 16 bursts in the FIFO, 512 words less one, lanes 3 and 2, `IFMT_16BIT`.
    assert_eq!(
        burst_format(alone),
        0x1000_0000 | 16 << 18 | 511 << 5 | 5 << 2 | 3
    );
    let both =
        Scenario::new(c"ci16", 1024, HostFormat::Ci16, WireFormat::Ci16).on(3, RxChannels::Both);
    // 8 bursts in the FIFO, 1024 words less one, all four lanes.
    assert_eq!(burst_format(both), 0x1000_0000 | 8 << 18 | 1023 << 5 | 3);
}

/// Two real channels (`ai` and `aq`): libusdr has no one-to-two real converter and fails
/// the create after resetting the DMA; rsl-usdr refuses it before any operation
/// (whitelisted divergence).
#[test]
fn two_real_channels_are_refused_by_both() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    assert_eq!(oracle.create_rx_stream_on(c"i16", 3, 1024), Err(-22));
    drop(oracle.close());

    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let start = sim.board().trace().len();
    let config =
        RxConfig::new(1024, HostFormat::I16, WireFormat::I16).with_channels(RxChannels::Both);
    assert!(matches!(
        device.create_streams(StreamsConfig::rx(config)),
        Err(Error::NoConversion { .. })
    ));
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// Gateware without the RX chain reports one RX channel; libusdr still creates a complex
/// channel-1 stream there, and rsl-usdr refuses it before any operation (whitelisted
/// divergence: nothing shows what lanes 2 and 3 carry without the chain's second NCO).
#[test]
fn a_second_complex_channel_without_the_rx_chain_is_refused_where_libusdr_creates_it() {
    let board = || SimBoard::new(BoardRevision::Rev3).with_tx_only();
    let mut oracle = Oracle::open(board()).expect("libusdr opens");
    // libusdr divides by the sample rate when a stream tunes its default frequency.
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle
        .create_rx_stream_on(c"ci16", 2, 1024)
        .expect("libusdr creates the stream");
    drop(oracle.close());

    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    let start = sim.board().trace().len();
    for channels in [RxChannels::Channel1, RxChannels::Both] {
        let config =
            RxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16).with_channels(channels);
        assert!(matches!(
            device.create_streams(StreamsConfig::rx(config)),
            Err(Error::UnsupportedRxChannels { .. })
        ));
    }
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}
