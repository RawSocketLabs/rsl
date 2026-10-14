//! usdr's RX stream (start, receive, stop) against libusdr's, operation for operation.

// Only the Si5332 block and its address are used here; the fix itself is not.
#[path = "common/baseband.rs"]
mod baseband;
#[allow(
    dead_code,
    reason = "this test shares the module but not the band-crossing fix"
)]
#[path = "common/crossing.rs"]
mod crossing;
#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;
use std::ops::Range;
use std::time::Duration;

use baseband::without_baseband_loop;
use crossing::mixer_lo_block;
use num_complex::Complex;
use sim_bus::SimBus;
use usdr::{Device, Error, RxGain, RxIqCorrection, RxPath, RxThrottle, RxTrim};
use usdr_oracle::Oracle;
use usdr_sim::{BoardRevision, Op, PllLock, SimBoard};

/// One call, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `set_rx_sample_rate`.
    Rate(u32),

    /// `set_rx_frequency`.
    Frequency(u32),

    /// `set_tx_frequency`.
    TxFrequency(u32),

    /// libusdr's numeric RX gain control `path` set to `value`; usdr's `set_rx_gain`.
    Gain(&'static CStr, u64, RxGain),

    /// libusdr's `rx/phgaincorr` set to `value`; usdr's `set_rx_iq_correction`.
    IqCorrection(u64, RxIqCorrection),

    /// libusdr's `rx/path` set to a string; usdr's `set_rx_path`.
    Path(&'static CStr, RxPath),

    /// `start_rx_stream`: libusdr's create, START and sync "none", as one window.
    Start(u32),

    /// As [`Call::Start`], where libusdr's baseband loop does nothing (see
    /// [`without_baseband_loop`]).
    StartWithIdleLoop(u32),

    /// libusdr's `rx/dccorr` set to `value`; usdr's `set_rx_dc_trim`.
    DcTrim(u64, RxTrim, RxTrim),

    /// `receive`, with a 100 ms timeout.
    Receive,

    /// `stop_rx_stream`: libusdr's STOP and destroy, as one window.
    Stop,

    /// `set_rx_throttle`: send and skip, or off.
    Throttle(Option<(u8, u8)>),

    /// `set_rx_ready`: libusdr's `usdr_dms_set_ready`.
    Ready,
}

impl Call {
    /// Makes the call through libusdr, returning whether it succeeded.
    fn on_libusdr(self, oracle: &mut Oracle) -> bool {
        match self {
            Self::Rate(rate) => oracle.set_rx_rate(rate).is_ok(),
            Self::Frequency(hz) => oracle.set_rx_frequency(hz).is_ok(),
            Self::TxFrequency(hz) => oracle.set_tx_frequency(hz).is_ok(),
            // SAFETY: every RX gain handler reads its value as a number.
            Self::Gain(path, value, _) => unsafe { oracle.set_uint(path, value) }.is_ok(),
            Self::IqCorrection(value, _) => {
                // SAFETY: `dev_m2_lm6_1_sdr_rx_phgaincorr_set` reads its value as a number.
                unsafe { oracle.set_uint(c"/dm/sdr/0/rx/phgaincorr", value) }.is_ok()
            }
            Self::Path(name, _) => {
                // SAFETY: `dev_m2_lm6_1_sdr_rx_path_set` reads a value above 4096 as a
                // string and keeps no reference to it.
                unsafe { oracle.set_str(c"/dm/sdr/0/rx/path", name) }.is_ok()
            }
            Self::DcTrim(value, ..) => {
                // SAFETY: `dev_m2_lm6_1_sdr_rx_dccorr_set` reads its value as a number.
                unsafe { oracle.set_uint(c"/dm/sdr/0/rx/dccorr", value) }.is_ok()
            }
            Self::Start(samples) | Self::StartWithIdleLoop(samples) => {
                oracle.create_rx_stream(samples).is_ok()
                    && oracle.start_rx_stream().is_ok()
                    && oracle.sync_free_run().is_ok()
            }
            Self::Receive => oracle.receive_rx(100).is_ok(),
            Self::Stop => oracle.stop_rx_stream().is_ok() && oracle.destroy_rx_stream().is_ok(),
            Self::Throttle(throttle) => {
                let value = throttle.map_or(0, |(send, skip)| {
                    1 << 16 | u32::from(send) << 8 | u32::from(skip)
                });
                oracle.set_rx_throttle(value).is_ok()
            }
            Self::Ready => oracle.set_rx_ready().is_ok(),
        }
    }

    /// Makes the call through usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::Frequency(hz) => device.set_rx_frequency(hz),
            Self::TxFrequency(hz) => device.set_tx_frequency(hz),
            Self::Gain(_, _, gain) => device.set_rx_gain(gain),
            Self::IqCorrection(_, correction) => device.set_rx_iq_correction(correction),
            Self::Path(_, path) => device.set_rx_path(path),
            Self::DcTrim(_, i, q) => device.set_rx_dc_trim(i, q),
            Self::Start(samples) | Self::StartWithIdleLoop(samples) => {
                device.start_rx_stream(samples)
            }
            Self::Receive => {
                let mut samples = vec![Complex::new(0, 0); 1 << 16];
                device
                    .receive(&mut samples, Duration::from_millis(100))
                    .map(|_| ())
            }
            Self::Stop => device.stop_rx_stream(),
            Self::Throttle(throttle) => {
                device.set_rx_throttle(throttle.map(|(send, skip)| RxThrottle { send, skip }))
            }
            Self::Ready => device.set_rx_ready(),
        }
    }
}

/// The window without libusdr's mixer-off block, which the baseband loop's mixer-on left
/// it owing: usdr never turned the mixer on, so has nothing to turn off. The block must
/// be exactly libusdr's six writes.
fn without_owed_mixer_exit(window: &[Op]) -> Vec<Op> {
    let block = mixer_lo_block(false);
    let start = window
        .windows(block.len())
        .position(|ops| ops == block)
        .expect("libusdr leaves the mixer band its baseband loop entered");
    [&window[..start], &window[start + block.len()..]].concat()
}

/// Makes `calls` in turn on fresh boards through libusdr and usdr and compares each
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
    let mut device = Device::with_bus(sim.clone()).expect("usdr powers up the board");
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
            "{call:?}: libusdr ok={expected_ok}, usdr {result:?}"
        );
        let expected = &reference.trace()[expected];
        let mut expected = expected.to_vec();
        if owed_mixer_exit && expected.windows(6).any(|ops| ops == mixer_lo_block(false)) {
            expected = without_owed_mixer_exit(&expected);
            owed_mixer_exit = false;
        }
        if let Call::Start(_) | Call::StartWithIdleLoop(_) = call {
            let idle_loop = matches!(call, Call::StartWithIdleLoop(_));
            let (trimmed, mixer_on) = without_baseband_loop(&expected, idle_loop);
            expected = trimmed;
            owed_mixer_exit |= mixer_on;
        }
        let expected: &[Op] = &expected;
        let actual: &[Op] = &board.trace()[actual];
        if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
            panic!(
                "{call:?}: first difference at op {i}: libusdr {:x?}, usdr {:x?}",
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

/// The DC calibration that starts a stream ends by restoring RXVGA2's stage steps, the
/// ones set before it, rather than zeroing them.
#[test]
fn stream_calibration_restores_the_rx_vga2_gains_like_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Gain(c"/dm/sdr/0/rx/gain/vga2a", 9, RxGain::Vga2a(9)),
            Call::Gain(c"/dm/sdr/0/rx/gain/vga2b", 12, RxGain::Vga2b(12)),
            Call::Start(1024),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// Stream creation writes the IQ correction set before it, for this stream and the next,
/// rather than a neutral one.
#[test]
fn stream_creation_reapplies_the_rx_iq_correction_like_libusdr() {
    let correction = RxIqCorrection {
        i_gain: 1500,
        q_gain: 2047,
        phase: 700,
    };
    // Phase 700 in bits 63:48, Q gain 2047 in 31:16, I gain 1500 in 15:0.
    let value = 0x02bc << 48 | 0x07ff << 16 | 0x05dc;
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::IqCorrection(value, correction),
            Call::Start(1024),
            Call::Stop,
            Call::Start(1024),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// The DC calibration borrows the TX PLL; with a TX frequency set, stream creation tunes it
/// back there, and a second stream does again.
#[test]
fn stream_creation_retunes_the_tx_pll_like_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::TxFrequency(2_400_000_000),
            Call::Frequency(1_000_000_000),
            Call::Start(1024),
            Call::Stop,
            Call::Start(1024),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// After a TX low-LO fallback, stream creation retunes the TX PLL to the TX frequency
/// itself, not the lowest lock the fallback found; that tune fails, and libusdr ignores
/// it, so the stream still starts.
#[test]
fn stream_creation_retunes_a_fallen_back_tx_lo_like_libusdr() {
    let board = || {
        SimBoard::new(BoardRevision::Rev3).with_tx_pll_lock(PllLock {
            unlocked_below_hz: 245_000_000,
            ..PllLock::default()
        })
    };
    let outcomes = assert_calls_match(
        board,
        &[
            Call::Rate(1_000_000),
            Call::Frequency(1_000_000_000),
            Call::TxFrequency(240_000_000),
            Call::Start(1024),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// A path fixed before the first tune holds through the default 320 MHz tune at stream
/// creation. libusdr's baseband loop then selects no band and fails to retune at 0 Hz
/// before any operation, so here the divergence has nothing to cut.
#[test]
fn a_path_fixed_before_tuning_holds_through_stream_creation_like_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Path(c"rxh", RxPath::High),
            Call::StartWithIdleLoop(1024),
            Call::Frequency(1_000_000_000),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// The DC calibration that starts a stream rewrites the mixer input with no I trim, as
/// libusdr's does from a cache nothing sets; the Q trim register is not touched.
#[test]
fn stream_calibration_clears_the_i_dc_trim_like_libusdr() {
    let trim = |code| RxTrim::new(code).expect("a seven-bit code");
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::DcTrim(0x0015_006a, trim(0x15), trim(0x6a)),
            Call::Start(1024),
            Call::Stop,
        ],
    );
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {outcomes:?}"
    );
}

/// The DC calibration that starts a stream runs the same with the RF loopback on.
#[test]
fn a_stream_starts_with_the_rf_loopback_on_like_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Path(c"rxw_lb", RxPath::WideLoopback),
            Call::Start(1024),
            Call::Stop,
            Call::Path(c"auto", RxPath::Auto),
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

/// Throttling needs a stream (libusdr refuses with -EINVAL before one exists); while one
/// runs, enabling and disabling it is one front-end command each.
#[test]
fn throttling_a_stream_matches_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Throttle(Some((0, 0))),
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Start(1024),
            Call::Throttle(Some((3, 1))),
            Call::Receive,
            Call::Throttle(None),
            Call::Stop,
        ],
    );
    assert_eq!(
        outcomes,
        [false, true, true, true, true, true, true, true],
        "only the throttle before the stream fails"
    );
}

/// While a stream runs, the explicit host-ready strobe is the DMA control write each
/// receive makes until the first packet, which the receive then makes again. (Without a
/// stream there is no libusdr handle to call it on; usdr's `Error::NotStreaming` is
/// in `tests/stream.rs::receiving_or_stopping_without_a_stream_is_refused`.)
#[test]
fn an_explicit_host_ready_matches_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Start(1024),
            Call::Ready,
            Call::Receive,
            Call::Ready,
            Call::Stop,
        ],
    );
    assert!(outcomes.iter().all(|&ok| ok), "every call succeeds");
}
