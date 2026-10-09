//! rsl-usdr's RX controls (sample rate, bandwidth, frequency) against libusdr's, operation
//! for operation, over sequences of calls.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ops::Range;

use rsl_usdr::{Device, Error};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

/// One RX call, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `set_rx_sample_rate`.
    Rate(u32),

    /// `set_rx_bandwidth`.
    Bandwidth(u32),
}

impl Call {
    /// Makes the call through libusdr, returning whether it succeeded.
    fn on_libusdr(self, oracle: &mut Oracle) -> bool {
        match self {
            Self::Rate(rate) => oracle.set_rx_rate(rate),
            Self::Bandwidth(hz) => oracle.set_rx_bandwidth(hz),
        }
        .is_ok()
    }

    /// Makes the call through rsl-usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::Bandwidth(hz) => device.set_rx_bandwidth(hz),
        }
    }
}

/// Makes `calls` in turn on fresh boards from `board` through libusdr and rsl-usdr, and
/// compares each call's outcome and operations.
fn assert_calls_match(board: impl Fn() -> SimBoard, calls: &[Call]) {
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

    for ((call, (expected_ok, expected)), (result, actual)) in
        calls.iter().zip(expected).zip(actual)
    {
        assert_eq!(
            result.is_ok(),
            expected_ok,
            "{call:?}: libusdr ok={expected_ok}, rsl-usdr {result:?}"
        );
        let expected: &[Op] = &reference.trace()[expected];
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
    }
}

/// A revision-3 board.
fn rev3() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3)
}

/// A fixed bandwidth survives rate changes; 0 writes the narrowest filter and hands the
/// filter back to the rate.
#[test]
fn fixed_bandwidth_holds_across_rate_changes_like_libusdr() {
    assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Bandwidth(5_000_000),
            Call::Rate(10_000_000),
            Call::Bandwidth(0),
            Call::Rate(20_000_000),
        ],
    );
}

/// Above 47 MHz the filter is bypassed.
#[test]
fn wide_bandwidth_bypasses_the_filter_like_libusdr() {
    assert_calls_match(rev3, &[Call::Rate(20_000_000), Call::Bandwidth(50_000_000)]);
}
