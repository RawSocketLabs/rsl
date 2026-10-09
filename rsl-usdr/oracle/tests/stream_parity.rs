//! rsl-usdr's RX stream (start, receive, stop) against libusdr's, operation for operation.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ops::Range;
use std::time::Duration;

use num_complex::Complex;
use rsl_usdr::{Device, Error};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, I2cAddress, Op, SimBoard};
use sim_bus::SimBus;

/// The Si5332.
const CLOCK: I2cAddress = I2cAddress { bus: 0, addr: 0x6a };
/// FPGA register latching GPO banks.
const REG_GPO: u32 = 0;
/// The RX DSP chain's configuration port.
const REG_PHY_RX: u32 = 56;
/// The RX DMA status register, read first by libusdr's DMA reset.
const REG_RX_DMA_STATUS: u32 = 6;
/// The SPI word that ends libusdr's DC calibration: RXVGA2 gains back to 0.
const VGA2_GAIN_RESET: u32 = 0xe800;

/// One call, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `set_rx_sample_rate`.
    Rate(u32),

    /// `set_rx_frequency`.
    Frequency(u32),

    /// `start_rx_stream`: libusdr's create, START and sync "none", as one window.
    Start(u32),

    /// `receive`, with a 100 ms timeout.
    Receive,

    /// `stop_rx_stream`: libusdr's STOP and destroy, as one window.
    Stop,
}

impl Call {
    /// Makes the call through libusdr, returning whether it succeeded.
    fn on_libusdr(self, oracle: &mut Oracle) -> bool {
        match self {
            Self::Rate(rate) => oracle.set_rx_rate(rate).is_ok(),
            Self::Frequency(hz) => oracle.set_rx_frequency(hz).is_ok(),
            Self::Start(samples) => {
                oracle.create_rx_stream(samples).is_ok()
                    && oracle.start_rx_stream().is_ok()
                    && oracle.sync_free_run().is_ok()
            }
            Self::Receive => oracle.receive_rx(100).is_ok(),
            Self::Stop => oracle.stop_rx_stream().is_ok() && oracle.destroy_rx_stream().is_ok(),
        }
    }

    /// Makes the call through rsl-usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::Frequency(hz) => device.set_rx_frequency(hz),
            Self::Start(samples) => device.start_rx_stream(samples),
            Self::Receive => {
                let mut samples = vec![Complex::new(0, 0); 1 << 16];
                device
                    .receive(&mut samples, Duration::from_millis(100))
                    .map(|_| ())
            }
            Self::Stop => device.stop_rx_stream(),
        }
    }
}

/// libusdr's `si5332_set_port3_en` writes for an RX-only device: OUT3 on or off, READY,
/// the power-downs, ACTIVE.
fn mixer_lo_block(on: bool) -> [Op; 6] {
    let write = |bytes: [u8; 2]| Op::I2c {
        addr: CLOCK,
        write: bytes.to_vec(),
        read: Vec::new(),
    };
    if on {
        [
            [0xb6, 0x43],
            [0x06, 0x01],
            [0xba, 0x76],
            [0xbb, 0x34],
            [0xbc, 0x08],
            [0x06, 0x02],
        ]
    } else {
        [
            [0xb6, 0x03],
            [0x06, 0x01],
            [0xba, 0x7e],
            [0xbb, 0x3c],
            [0xbc, 0x48],
            [0x06, 0x02],
        ]
    }
    .map(write)
}

/// The window without libusdr's mixer-off block, which the baseband loop's mixer-on left
/// it owing: rsl-usdr never turned the mixer on, so has nothing to turn off. The block must
/// be exactly libusdr's six writes.
fn without_owed_mixer_exit(window: &[Op]) -> Vec<Op> {
    let block = mixer_lo_block(false);
    let start = window
        .windows(block.len())
        .position(|ops| ops == block)
        .expect("libusdr leaves the mixer band its baseband loop entered");
    [&window[..start], &window[start + block.len()..]].concat()
}

/// libusdr's create window without the per-channel baseband loop.
///
/// Intentional divergence, the second: after its DC calibration, libusdr's stream create
/// reapplies each channel's LO as a baseband offset (`usdr_rfic_fe_set_freq(FE_FREQ_BB_RX,
/// .., rx_raw.lo[i])` in `usdr_device_m2_lm6_1_create_stream`), a bug that reselects the
/// band, clamps the NCOs and bypasses the filter. rsl-usdr leaves it out. The cut runs from
/// the calibration's last write to the DMA engine's status read; it must be non-empty and
/// hold only what that loop does (LMS6002D SPI, GPO and PHY writes, Si5332 writes, sleeps),
/// so nothing else can hide in it. The window between also holds libusdr's TX PLL retune
/// when a TX LO is set; TX is not ported, so it never is, and the cut never hides one.
///
/// Returns the trimmed window and whether the cut turned the board mixer on: with no
/// frequency set, libusdr's loop selects the mixer band, and its next tune out of that band
/// writes a mixer-off block rsl-usdr has no reason to (see [`without_owed_mixer_exit`]).
fn without_baseband_loop(window: &[Op]) -> (Vec<Op>, bool) {
    let dma_reset = window
        .iter()
        .position(|op| matches!(op, Op::RegRead { addr, .. } if *addr == REG_RX_DMA_STATUS))
        .expect("create resets the DMA engine");
    let calibrated = window[..dma_reset]
        .iter()
        .rposition(|op| matches!(op, Op::Spi { out, .. } if *out == VGA2_GAIN_RESET))
        .expect("create calibrates first")
        + 1;
    let cut = &window[calibrated..dma_reset];
    assert!(!cut.is_empty(), "libusdr's baseband loop ran");
    assert!(
        cut.iter().all(|op| matches!(
            op,
            Op::Spi { .. }
                | Op::Sleep { .. }
                | Op::RegWrite {
                    addr: REG_GPO | REG_PHY_RX,
                    ..
                }
        ) || matches!(op, Op::I2c { addr, .. } if *addr == CLOCK)),
        "the cut holds only the baseband loop's operations: {cut:x?}"
    );
    let mixer_on = cut.windows(6).any(|ops| ops == mixer_lo_block(true));
    (
        [&window[..calibrated], &window[dma_reset..]].concat(),
        mixer_on,
    )
}

/// Makes `calls` in turn on fresh boards through libusdr and rsl-usdr and compares each
/// call's outcome and operations; returns whether each succeeded.
fn assert_calls_match(board: impl Fn() -> SimBoard, calls: &[Call]) -> Vec<bool> {
    let mut oracle = Oracle::open(board()).expect("libusdr opens the board");
    let mut expected: Vec<(bool, Range<usize>)> = Vec::new();
    for &call in calls {
        let start = oracle.trace_len();
        let ok = call.on_libusdr(&mut oracle);
        expected.push((ok, start..oracle.trace_len()));
    }
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up the board");
    let mut actual: Vec<(Result<(), Error>, Range<usize>)> = Vec::new();
    for &call in calls {
        let start = sim.board().trace().len();
        let result = call.on_rsl(&mut device);
        actual.push((result, start..sim.board().trace().len()));
    }
    let board = sim.board();

    let mut outcomes = Vec::new();
    let mut owed_mixer_exit = false;
    for ((call, (expected_ok, expected)), (result, actual)) in
        calls.iter().zip(expected).zip(actual)
    {
        assert_eq!(
            result.is_ok(),
            expected_ok,
            "{call:?}: libusdr ok={expected_ok}, rsl-usdr {result:?}"
        );
        let expected = &reference.trace()[expected];
        let mut expected = expected.to_vec();
        if owed_mixer_exit && expected.windows(6).any(|ops| ops == mixer_lo_block(false)) {
            expected = without_owed_mixer_exit(&expected);
            owed_mixer_exit = false;
        }
        if let Call::Start(_) = call {
            let (trimmed, mixer_on) = without_baseband_loop(&expected);
            expected = trimmed;
            owed_mixer_exit |= mixer_on;
        }
        let expected: &[Op] = &expected;
        let actual: &[Op] = &board.trace()[actual];
        if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
            panic!(
                "{call:?}: first difference at op {i}: libusdr {:x?}, rsl-usdr {:x?}",
                expected[i], actual[i]
            );
        }
        assert_eq!(
            expected.len(),
            actual.len(),
            "{call:?}: same prefix, different lengths"
        );
        outcomes.push(expected_ok);
    }
    outcomes
}

/// A revision-3 board.
fn rev3() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3)
}

/// The FFI crate's flow: rate, frequency, a stream of 1024-sample packets, two receives
/// (only the first tells the DMA engine the host is ready), a retune while streaming, then
/// stop.
#[test]
fn a_tuned_stream_matches_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Start(1024),
            Call::Receive,
            Call::Receive,
            Call::Frequency(433_000_000),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// With no frequency set, creating the stream tunes to 320 MHz first; a second stream in
/// the same session powers the receiver up again and recalibrates, and retuning while it
/// runs matches too.
#[test]
fn an_untuned_stream_and_a_second_one_match_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(1_000_000),
            Call::Start(1024),
            Call::Stop,
            Call::Start(1024),
            Call::Frequency(1_000_000_000),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// 16384 samples need two bursts; revision 1 routes its clocks differently.
#[test]
fn a_two_burst_stream_on_rev1_matches_libusdr() {
    let outcomes = assert_calls_match(
        || SimBoard::new(BoardRevision::Rev1),
        &[
            Call::Rate(10_000_000),
            Call::Frequency(433_000_000),
            Call::Start(16384),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// The packets each side receives for `samples_per_packet`, two of them, after the FFI
/// crate's setup.
fn received(samples_per_packet: u32) -> (Vec<Complex<i16>>, Vec<Complex<i16>>) {
    let packet = usize::try_from(samples_per_packet).expect("fits");
    let mut oracle = Oracle::open(rev3()).expect("libusdr opens the board");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle
        .create_rx_stream(samples_per_packet)
        .expect("creates");
    oracle.start_rx_stream().expect("starts");
    oracle.sync_free_run().expect("free-runs");
    let mut expected = Vec::new();
    for _ in 0..2 {
        let bytes = oracle.receive_rx(100).expect("a packet");
        expected.extend(bytes[..packet * 4].chunks_exact(4).map(|sample| {
            Complex::new(
                i16::from_le_bytes([sample[0], sample[1]]),
                i16::from_le_bytes([sample[2], sample[3]]),
            )
        }));
    }
    drop(oracle.close());

    let mut device = Device::with_bus(SimBus::new(rev3())).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    device.start_rx_stream(samples_per_packet).expect("starts");
    let mut actual = Vec::new();
    let mut samples = vec![Complex::new(0, 0); packet];
    for _ in 0..2 {
        device
            .receive(&mut samples, Duration::from_millis(100))
            .expect("a packet");
        actual.extend_from_slice(&samples);
    }
    (expected, actual)
}

/// The samples themselves match libusdr's, for one-burst and two-burst packets: the sim's
/// counting pattern arrives in order, I then Q.
#[test]
fn received_samples_match_libusdr() {
    for samples_per_packet in [1024, 16384] {
        let (expected, actual) = received(samples_per_packet);
        assert_eq!(actual, expected, "{samples_per_packet}-sample packets");
        assert_eq!(actual[1], Complex::new(1, -1), "the sim's counting pattern");
    }
}
