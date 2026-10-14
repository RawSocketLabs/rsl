//! usdr's reference-clock choice against libusdr's: the runtime switch, and the
//! reference frequency feeding the next sample-clock plan.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ops::Range;
use std::time::Duration;

use num_complex::Complex;

use sim_bus::SimBus;
use usdr::{Device, ReferenceClock};
use usdr_oracle::Oracle;
use usdr_sim::{BoardRevision, Op, SimBoard};

/// One step applied to both drivers.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// An external reference at this frequency, or the on-board oscillator.
    Reference(Option<u32>),
    Rate(u32),
    Frequency(u32),
    /// Start and receive once from a 1024-sample stream, then stop.
    Stream,
}

/// Runs `steps` on fresh boards through libusdr and usdr and compares each step's
/// operations.
fn assert_steps_match(revision: BoardRevision, steps: &[Step]) {
    let mut oracle = Oracle::open(SimBoard::new(revision)).expect("libusdr opens the board");
    let mut expected_windows: Vec<Range<usize>> = Vec::new();
    for step in steps {
        let start = oracle.trace_len();
        match *step {
            Step::Reference(Some(hz)) => oracle.set_reference_clock(true, hz),
            Step::Reference(None) => oracle.set_reference_clock(false, ReferenceClock::INTERNAL_HZ),
            Step::Rate(rate) => oracle.set_rx_rate(rate),
            Step::Frequency(hz) => oracle.set_rx_frequency(hz),
            Step::Stream => oracle
                .create_rx_stream(1024)
                .and_then(|()| oracle.start_rx_stream())
                .and_then(|()| oracle.sync_free_run())
                .and_then(|()| oracle.receive_rx(100).map(drop))
                .and_then(|()| oracle.stop_rx_stream())
                .and_then(|()| oracle.destroy_rx_stream()),
        }
        .expect("libusdr accepts the step");
        expected_windows.push(start..oracle.trace_len());
    }
    let reference = oracle.close();

    let sim = SimBus::new(SimBoard::new(revision));
    let mut device = Device::with_bus(sim.clone()).expect("usdr powers up the board");
    let mut actual_windows: Vec<Range<usize>> = Vec::new();
    for step in steps {
        let start = sim.board().trace().len();
        match *step {
            Step::Reference(Some(hz)) => {
                device.set_reference_clock(ReferenceClock::External { hz })
            }
            Step::Reference(None) => device.set_reference_clock(ReferenceClock::Oscillator),
            Step::Rate(rate) => device.set_rx_sample_rate(rate),
            Step::Frequency(hz) => device.set_rx_frequency(hz),
            Step::Stream => {
                let mut samples = vec![Complex::new(0, 0); 1 << 16];
                device
                    .start_rx_stream(1024)
                    .and_then(|()| device.receive(&mut samples, Duration::from_millis(100)))
                    .and_then(|_| device.stop_rx_stream())
            }
        }
        .expect("usdr accepts the step");
        actual_windows.push(start..sim.board().trace().len());
    }
    let board = sim.board();

    for ((step, expected), actual) in steps.iter().zip(expected_windows).zip(actual_windows) {
        let mut expected: &[Op] = &reference.trace()[expected];
        let mut actual: &[Op] = &board.trace()[actual];
        if matches!(step, Step::Stream) {
            // Up to the end of DC calibration (which tunes the TX PLL from the reference);
            // stream_parity covers the rest and its whitelisted divergences.
            expected = through_calibration(expected);
            actual = through_calibration(actual);
        }
        assert_eq!(expected, actual, "{revision:?} {step:?}");
    }
}

/// The SPI word that ends libusdr's DC calibration: RXVGA2 gains back to 0.
const VGA2_GAIN_RESET: u32 = 0xe800;

/// `window` through the end of stream create's DC calibration.
fn through_calibration(window: &[Op]) -> &[Op] {
    let end = window
        .iter()
        .position(|op| matches!(op, Op::Spi { out, .. } if *out == VGA2_GAIN_RESET))
        .expect("stream create calibrates");
    &window[..=end]
}

#[test]
fn switching_to_an_external_reference_and_back_matches_libusdr() {
    assert_steps_match(
        BoardRevision::Rev3,
        &[
            Step::Reference(Some(40_000_000)),
            Step::Rate(20_000_000),
            Step::Reference(None),
            Step::Rate(20_000_000),
        ],
    );
}

#[test]
fn an_external_reference_feeds_the_next_sample_clock_plan() {
    assert_steps_match(
        BoardRevision::Rev1,
        &[
            Step::Reference(Some(30_720_000)),
            Step::Rate(1_000_000),
            Step::Rate(62_000_000),
        ],
    );
}

/// After a switch, tuning the RX synthesizer and the stream's DC calibration (which tunes
/// the TX PLL) use the new reference frequency too.
#[test]
fn tuning_and_stream_calibration_use_the_new_reference() {
    assert_steps_match(
        BoardRevision::Rev3,
        &[
            Step::Reference(Some(30_720_000)),
            Step::Rate(20_000_000),
            Step::Frequency(915_000_000),
            Step::Stream,
        ],
    );
}
