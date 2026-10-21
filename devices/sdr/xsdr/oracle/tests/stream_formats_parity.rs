//! xsdr's RX and TX streams in libusdr's other formats (`host@wire`) against libusdr's,
//! operation for operation and byte for byte, on fresh simulated XSDRs: everything
//! between the open and the close, the close included, every received buffer's bytes,
//! every send's statistics and every packet the TX core played.
//!
//! The boards report gateware with a TX MMCM (HWID PHY bit 0x20), stamped as new as
//! libusdr's TX minimum, and for the extended TX core also PHY bit 0x08. Packet sizes
//! fill whole front-end words, so no burst carries a stub.
//!
//! Whitelisted divergences, each refused by xsdr before any bus access: a host format
//! libusdr has no converter for, which it byte-copies
//! (`a_host_libusdr_byte_copies_is_refused_before_any_access`), and a `ci12` wire on the
//! original TX core, which libusdr refuses after streaming up
//! (`a_twelve_bit_wire_on_the_original_tx_core_is_refused_before_any_access`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CString;
use std::time::Duration;

use sim_bus::SimBus;
use xsdr::{Device, Error, HostSample, RxChannels, SampleFormat, TxChannels, WireFormat};
use xsdr_oracle::Oracle;
use xsdr_sim::{Op, SimBoard, TxPacket, Variant};

/// An XSDR with a TX MMCM.
const TX_MMCM: u32 = 0x0030_0020;
/// HWID PHY bit: the extended TX front end.
const EXTENDED_TX: u32 = 0x08;
/// libusdr's oldest TX gateware (`MINIM_FWID_COMPAT`).
const TX_GATEWARE: u32 = 0xd2b1_0c09;
/// A wait; the sim answers at once.
const TIMEOUT: Duration = Duration::from_millis(100);
/// The rate every scenario sets first.
const RATE: u32 = 10_000_000;

/// A host element as the tests write and read its bytes.
trait Element: HostSample {
    /// Bytes one element takes.
    const SIZE: usize;
    /// The `n`th test sample on `channel`, well inside full scale.
    fn nth(n: usize, channel: usize) -> Self;
    /// The element in `bytes`.
    fn read(bytes: &[u8]) -> Self;
    /// Appends the element's bytes.
    fn write(&self, out: &mut Vec<u8>);
}

/// A test value: `n` and `channel` folded into a 12-bit-safe 16-bit value.
fn value(n: usize, channel: usize) -> i16 {
    i16::try_from((n * 37 + channel * 1000) % 20_000).expect("small") - 10_000
}

impl Element for [i16; 2] {
    const SIZE: usize = 4;
    fn nth(n: usize, channel: usize) -> Self {
        [value(n, channel), -value(n, channel)]
    }
    fn read(b: &[u8]) -> Self {
        [
            i16::from_le_bytes([b[0], b[1]]),
            i16::from_le_bytes([b[2], b[3]]),
        ]
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend(self.iter().flat_map(|v| v.to_le_bytes()));
    }
}

impl Element for [f32; 2] {
    const SIZE: usize = 8;
    fn nth(n: usize, channel: usize) -> Self {
        let v = f32::from(value(n, channel)) / 32_767.0;
        [v, -v / 2.0]
    }
    fn read(b: &[u8]) -> Self {
        [
            f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            f32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        ]
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend(self.iter().flat_map(|v| v.to_le_bytes()));
    }
}

impl Element for [u8; 3] {
    const SIZE: usize = 3;
    fn nth(n: usize, channel: usize) -> Self {
        let [low, high] = value(n, channel).to_le_bytes();
        [low, high, low ^ 0x5a]
    }
    fn read(b: &[u8]) -> Self {
        [b[0], b[1], b[2]]
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend(self);
    }
}

impl Element for i16 {
    const SIZE: usize = 2;
    fn nth(n: usize, channel: usize) -> Self {
        value(n, channel)
    }
    fn read(b: &[u8]) -> Self {
        Self::from_le_bytes([b[0], b[1]])
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend(self.to_le_bytes());
    }
}

impl Element for f32 {
    const SIZE: usize = 4;
    fn nth(n: usize, channel: usize) -> Self {
        f32::from(value(n, channel)) / 32_767.0
    }
    fn read(b: &[u8]) -> Self {
        Self::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend(self.to_le_bytes());
    }
}

/// Runs `$f::<element>($args)` for the element of host format `$host`.
macro_rules! by_host {
    ($host:expr, $f:ident($($arg:expr),*)) => {
        match $host {
            SampleFormat::Ci16 => $f::<[i16; 2]>($($arg),*),
            SampleFormat::Cf32 => $f::<[f32; 2]>($($arg),*),
            SampleFormat::Ci12 => $f::<[u8; 3]>($($arg),*),
            SampleFormat::I16 => $f::<i16>($($arg),*),
            SampleFormat::F32 => $f::<f32>($($arg),*),
            other => panic!("no host element for {other:?}"),
        }
    };
}

/// libusdr's name for a host format.
fn host_name(host: SampleFormat) -> &'static str {
    match host {
        SampleFormat::Ci16 => "ci16",
        SampleFormat::Cf32 => "cf32",
        SampleFormat::Ci12 => "ci12",
        SampleFormat::I16 => "i16",
        SampleFormat::F32 => "f32",
        other => panic!("no libusdr host name for {other:?}"),
    }
}

/// libusdr's data format `host@wire`.
fn format(host: SampleFormat, wire: WireFormat) -> CString {
    let wire = match wire {
        WireFormat::Ci16 => "ci16",
        WireFormat::Ci12 => "ci12",
        WireFormat::I16 => "i16",
        WireFormat::I12 => "i12",
        other => panic!("no libusdr wire name for {other:?}"),
    };
    CString::new(format!("{}@{wire}", host_name(host))).expect("no NUL")
}

/// libusdr's mask and the stream's channel count.
fn rx_mask(channels: RxChannels) -> (u64, usize) {
    match channels {
        RxChannels::A => (1, 1),
        RxChannels::B => (2, 1),
        RxChannels::Both => (3, 2),
    }
}

/// As [`rx_mask`], for TX.
fn tx_mask(channels: TxChannels) -> (u64, usize) {
    match channels {
        TxChannels::A => (1, 1),
        TxChannels::B => (2, 1),
        TxChannels::Both => (3, 2),
    }
}

/// A fresh XSDR libusdr streams both ways from, with the extended TX core if `extended`.
fn xsdr(extended: bool) -> SimBoard {
    let phy = if extended { EXTENDED_TX } else { 0 };
    SimBoard::new(Variant::Xsdr)
        .with_hwid(TX_MMCM | phy)
        .with_gateware_build(TX_GATEWARE)
}

/// Two received packets: each channel's host bytes, first sample and lost packets.
type Packets = Vec<(Vec<Vec<u8>>, u64, u64)>;

/// libusdr's RX session: the rate, a create in `host@wire`, the start, two receives and
/// the stop; its packets and its operations after the open, the close included.
fn libusdr_rx(
    wire: WireFormat,
    host: SampleFormat,
    channels: RxChannels,
    samples: u32,
) -> (Packets, Vec<Op>) {
    let mut oracle = Oracle::open(xsdr(false)).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    // SAFETY: the master rate takes its value as a number.
    unsafe { oracle.set_uint(c"/dm/rate/master", RATE.into()) }.expect("libusdr sets the rate");
    oracle
        .create_rx_stream_as(&format(host, wire), rx_mask(channels).0, samples)
        .expect("libusdr creates the stream");
    oracle.sync_rx_stream(c"none").expect("libusdr syncs");
    oracle.start_rx_stream().expect("libusdr starts");
    let packets = (0..2)
        .map(|_| {
            let (buffers, info) = oracle.receive_rx(100).expect("libusdr receives");
            (buffers, info.fsymtime, u64::from(info.totlost))
        })
        .collect();
    let stopped = oracle.stop_rx_stream();
    oracle
        .destroy_rx_stream()
        .and(stopped)
        .expect("libusdr stops");
    let board = oracle.close();
    (packets, board.trace()[opened..].to_vec())
}

/// xsdr's RX session, as [`libusdr_rx`]'s.
fn rsl_rx(
    wire: WireFormat,
    host: SampleFormat,
    channels: RxChannels,
    samples: u32,
) -> (Packets, Vec<Op>) {
    let sim = SimBus::new(xsdr(false));
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    device
        .set_sample_rate(RATE, RATE)
        .expect("xsdr sets the rate");
    device
        .create_rx_stream_as(channels, samples, wire, host)
        .expect("xsdr creates the stream");
    device.start_rx_stream().expect("xsdr starts");
    let count = rx_mask(channels).1;
    let size = usize::try_from(samples).expect("small");
    let packets = (0..2)
        .map(|_| by_host!(host, receive(&mut device, count, size)))
        .collect();
    device.stop_rx_stream().expect("xsdr stops");
    device.close().expect("xsdr closes the XSDR");
    let trace = sim.board().trace()[opened..].to_vec();
    (packets, trace)
}

/// One packet into `count` buffers of `samples` `S`s, as host bytes.
fn receive<S: Element + Default + Clone>(
    device: &mut Device,
    count: usize,
    samples: usize,
) -> (Vec<Vec<u8>>, u64, u64) {
    let mut buffers = vec![vec![S::default(); samples]; count];
    let mut slices: Vec<&mut [S]> = buffers.iter_mut().map(Vec::as_mut_slice).collect();
    let packet = device.receive(&mut slices, TIMEOUT).expect("xsdr receives");
    let bytes = buffers
        .iter()
        .map(|buffer| {
            let mut out = Vec::new();
            for sample in buffer {
                sample.write(&mut out);
            }
            out
        })
        .collect();
    (bytes, packet.first_sample, packet.lost_packets)
}

/// Compares an RX session in `host@wire` on `channels` through both.
fn compare_rx(wire: WireFormat, host: SampleFormat, channels: RxChannels, samples: u32) {
    let scenario = format!("{host:?}@{wire:?} on {channels:?}");
    let (expected, reference) = libusdr_rx(wire, host, channels, samples);
    let (actual, trace) = rsl_rx(wire, host, channels, samples);
    assert_eq!(
        actual, expected,
        "{scenario}: every packet's bytes, as libusdr's"
    );
    let filled = actual.iter().flat_map(|(buffers, ..)| buffers);
    assert!(
        filled.clone().count() == 2 * rx_mask(channels).1,
        "{scenario}: two packets"
    );
    assert!(
        filled.clone().all(|buffer| !buffer.is_empty()),
        "{scenario}: samples"
    );
    for (index, (ours, theirs)) in trace.iter().zip(&reference).enumerate() {
        assert_eq!(ours, theirs, "{scenario}: operation {index}");
    }
    assert_eq!(
        trace.len(),
        reference.len(),
        "{scenario}: as many operations"
    );
}

#[test]
fn every_complex_rx_format_on_each_channel_choice_matches_libusdr() {
    use SampleFormat::{Cf32, Ci12, Ci16};
    for wire in [WireFormat::Ci16, WireFormat::Ci12] {
        for channels in [RxChannels::A, RxChannels::B, RxChannels::Both] {
            for host in [Ci16, Cf32, Ci12] {
                // libusdr has no one-to-two `ci12` splitter.
                if host == Ci12 && channels == RxChannels::Both {
                    continue;
                }
                compare_rx(wire, host, channels, 1024);
            }
        }
    }
}

#[test]
fn twelve_bit_packets_of_several_bursts_match_libusdr() {
    // Both channels' `ci12` and lane 0's `i12`, split over several stub-free bursts.
    compare_rx(
        WireFormat::Ci12,
        SampleFormat::Cf32,
        RxChannels::Both,
        16_384,
    );
    compare_rx(WireFormat::I12, SampleFormat::F32, RxChannels::A, 65_536);
}

#[test]
fn every_real_rx_format_on_either_lane_matches_libusdr() {
    for wire in [WireFormat::I16, WireFormat::I12] {
        for channels in [RxChannels::A, RxChannels::B] {
            for host in [SampleFormat::I16, SampleFormat::F32] {
                compare_rx(wire, host, channels, 1024);
            }
        }
    }
}

/// A send's statistics: hardware time, kernel time, underruns and FIFO use.
type Stat = Option<(u64, u64, u64, u64)>;

/// The host bytes of send `send` on each of `count` channels, `samples` each.
fn host_buffers<S: Element>(send: usize, count: usize, samples: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|channel| {
            let mut out = Vec::new();
            for n in 0..samples {
                S::nth(send * samples + n, channel).write(&mut out);
            }
            out
        })
        .collect()
}

/// libusdr's TX session: the rate, a create in `host@wire`, the start, three sends and the
/// stop; each send's statistics, the operations after the open (the close included) and
/// the packets played.
fn libusdr_tx(
    board: SimBoard,
    wire: WireFormat,
    host: SampleFormat,
    channels: TxChannels,
    samples: u32,
) -> (Vec<Stat>, Vec<Op>, Vec<TxPacket>) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    // SAFETY: the master rate takes its value as a number.
    unsafe { oracle.set_uint(c"/dm/rate/master", RATE.into()) }.expect("libusdr sets the rate");
    let (mask, count) = tx_mask(channels);
    oracle
        .create_tx_stream_as(&format(host, wire), mask, samples)
        .expect("libusdr creates the stream");
    oracle.start_tx_stream().expect("libusdr starts");
    oracle.sync_tx_stream(c"none").expect("libusdr syncs");
    let size = usize::try_from(samples).expect("small");
    let stats = (0..3)
        .map(|send| {
            let buffers = by_host!(host, host_buffers(send, count, size));
            let slices: Vec<&[u8]> = buffers.iter().map(Vec::as_slice).collect();
            let stat = oracle
                .send_tx(&slices, samples, u64::MAX, 100)
                .expect("libusdr sends");
            stat.map(|stat| {
                (
                    stat.lhwtime,
                    u64::from(stat.ktime),
                    u64::from(stat.underruns),
                    u64::from(stat.fifo_used),
                )
            })
        })
        .collect();
    let stopped = oracle.stop_tx_stream();
    oracle
        .destroy_tx_stream()
        .and(stopped)
        .expect("libusdr stops");
    let board = oracle.close();
    let trace = board.trace()[opened..].to_vec();
    (stats, trace, board.tx_packets().to_vec())
}

/// xsdr's TX session, as [`libusdr_tx`]'s.
fn rsl_tx(
    board: SimBoard,
    wire: WireFormat,
    host: SampleFormat,
    channels: TxChannels,
    samples: u32,
) -> (Vec<Stat>, Vec<Op>, Vec<TxPacket>) {
    let sim = SimBus::new(board);
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    device
        .set_sample_rate(RATE, RATE)
        .expect("xsdr sets the rate");
    device
        .create_tx_stream_as(channels, samples, wire, host)
        .expect("xsdr creates the stream");
    device.start_tx_stream().expect("xsdr starts");
    let count = tx_mask(channels).1;
    let size = usize::try_from(samples).expect("small");
    let stats = (0..3)
        .map(|send| by_host!(host, send_from(&mut device, send, count, size)))
        .collect();
    device.stop_tx_stream().expect("xsdr stops");
    device.close().expect("xsdr closes the XSDR");
    let board = sim.board();
    let trace = board.trace()[opened..].to_vec();
    (stats, trace, board.tx_packets().to_vec())
}

/// Sends send `send`'s samples as `S` buffers.
fn send_from<S: Element>(device: &mut Device, send: usize, count: usize, samples: usize) -> Stat {
    let buffers: Vec<Vec<S>> = host_buffers::<S>(send, count, samples)
        .iter()
        .map(|bytes| bytes.chunks_exact(S::SIZE).map(S::read).collect())
        .collect();
    let slices: Vec<&[S]> = buffers.iter().map(Vec::as_slice).collect();
    let telemetry = device.send(&slices, None, TIMEOUT).expect("xsdr sends");
    telemetry.map(|telemetry| {
        (
            u64::from(telemetry.hardware_time),
            u64::from(telemetry.kernel_time),
            telemetry.underruns,
            u64::from(telemetry.fifo_used),
        )
    })
}

/// Compares a TX session in `host@wire` on `channels` through both.
fn compare_tx(extended: bool, wire: WireFormat, host: SampleFormat, channels: TxChannels) {
    let scenario = format!("{host:?}@{wire:?} on {channels:?}, extended {extended}");
    let samples = 1024;
    let (expected, reference, played) = libusdr_tx(xsdr(extended), wire, host, channels, samples);
    let (actual, trace, ours) = rsl_tx(xsdr(extended), wire, host, channels, samples);
    assert_eq!(actual, expected, "{scenario}: every send's statistics");
    for (index, (ours, theirs)) in trace.iter().zip(&reference).enumerate() {
        assert_eq!(ours, theirs, "{scenario}: operation {index}");
    }
    assert_eq!(
        trace.len(),
        reference.len(),
        "{scenario}: as many operations"
    );
    assert_eq!(ours, played, "{scenario}: the packets the TX core played");
    assert!(!ours.is_empty(), "{scenario}: the core played the sends");
}

#[test]
fn every_tx_host_on_the_original_core_matches_libusdr() {
    for channels in [TxChannels::A, TxChannels::B, TxChannels::Both] {
        for host in [SampleFormat::Ci16, SampleFormat::Cf32, SampleFormat::Ci12] {
            // libusdr has no two-to-one `ci12` interleaver to `ci16`.
            if host == SampleFormat::Ci12 && channels == TxChannels::Both {
                continue;
            }
            compare_tx(false, WireFormat::Ci16, host, channels);
        }
    }
}

#[test]
fn every_tx_format_on_the_extended_core_matches_libusdr() {
    for wire in [WireFormat::Ci16, WireFormat::Ci12] {
        for channels in [TxChannels::A, TxChannels::B, TxChannels::Both] {
            for host in [SampleFormat::Ci16, SampleFormat::Cf32, SampleFormat::Ci12] {
                // libusdr has no two-to-one `ci12` interleavers.
                if host == SampleFormat::Ci12 && channels == TxChannels::Both {
                    continue;
                }
                compare_tx(true, wire, host, channels);
            }
        }
    }
}

/// The operations a create adds after the rate, on a fresh board, and its result.
fn create_trace<E>(create: impl FnOnce(&mut Device) -> Result<(), E>) -> (Result<(), E>, Vec<Op>) {
    let sim = SimBus::new(xsdr(false));
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    device
        .set_sample_rate(RATE, RATE)
        .expect("xsdr sets the rate");
    let before = sim.board().trace().len();
    let result = create(&mut device);
    let trace = sim.board().trace()[before..].to_vec();
    (result, trace)
}

#[test]
fn a_host_libusdr_byte_copies_is_refused_before_any_access() {
    // libusdr has no `ci16` to `i16` converter and copies the wire's bytes instead.
    let mut oracle = Oracle::open(xsdr(false)).expect("libusdr opens the board");
    // SAFETY: the master rate takes its value as a number.
    unsafe { oracle.set_uint(c"/dm/rate/master", RATE.into()) }.expect("libusdr sets the rate");
    let format = format(SampleFormat::I16, WireFormat::Ci16);
    assert_eq!(
        oracle.create_rx_stream_as(&format, 1, 1024),
        Ok(()),
        "libusdr copies"
    );
    drop(oracle.close());

    let (result, trace) = create_trace(|device| {
        device.create_rx_stream_as(RxChannels::A, 1024, WireFormat::Ci16, SampleFormat::I16)
    });
    assert!(
        matches!(result, Err(Error::UnsupportedFormat { .. })),
        "{result:?}"
    );
    assert!(trace.is_empty(), "refused before any access: {trace:?}");
}

#[test]
fn a_twelve_bit_wire_on_the_original_tx_core_is_refused_before_any_access() {
    let mut oracle = Oracle::open(xsdr(false)).expect("libusdr opens the board");
    // SAFETY: the master rate takes its value as a number.
    unsafe { oracle.set_uint(c"/dm/rate/master", RATE.into()) }.expect("libusdr sets the rate");
    let rated = oracle.trace_len();
    let format = format(SampleFormat::Ci16, WireFormat::Ci12);
    assert_eq!(
        oracle.create_tx_stream_as(&format, 1, 1024),
        Err(-22),
        "libusdr's -EINVAL"
    );
    assert!(oracle.trace_len() > rated, "libusdr streamed up first");
    drop(oracle.close());

    let (result, trace) = create_trace(|device| {
        device.create_tx_stream_as(TxChannels::A, 1024, WireFormat::Ci12, SampleFormat::Ci16)
    });
    assert!(
        matches!(result, Err(Error::UnsupportedFormat { .. })),
        "{result:?}"
    );
    assert!(trace.is_empty(), "refused before any access: {trace:?}");
}
