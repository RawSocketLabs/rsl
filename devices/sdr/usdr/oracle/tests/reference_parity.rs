//! rsl-usdr's reference-clock choice against libusdr's: the runtime switch, and the
//! reference frequency feeding the next sample-clock plan.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ops::Range;

use rsl_usdr::{Device, ReferenceClock};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

/// One step applied to both drivers.
#[derive(Clone, Copy, Debug)]
enum Step {
    Reference(ReferenceClock),
    Rate(u32),
}

/// Runs `steps` on fresh boards through libusdr and rsl-usdr and compares each step's
/// operations.
fn assert_steps_match(revision: BoardRevision, steps: &[Step]) {
    let mut oracle = Oracle::open(SimBoard::new(revision)).expect("libusdr opens the board");
    let mut expected_windows: Vec<Range<usize>> = Vec::new();
    for step in steps {
        let start = oracle.trace_len();
        match *step {
            Step::Reference(ReferenceClock::External { hz }) => {
                oracle.set_reference_clock(true, hz)
            }
            Step::Reference(_) => oracle.set_reference_clock(false, ReferenceClock::INTERNAL_HZ),
            Step::Rate(rate) => oracle.set_rx_rate(rate),
        }
        .expect("libusdr accepts the step");
        expected_windows.push(start..oracle.trace_len());
    }
    let reference = oracle.close();

    let sim = SimBus::new(SimBoard::new(revision));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up the board");
    let mut actual_windows: Vec<Range<usize>> = Vec::new();
    for step in steps {
        let start = sim.board().trace().len();
        match *step {
            Step::Reference(clock) => device.set_reference_clock(clock),
            Step::Rate(rate) => device.set_rx_sample_rate(rate),
        }
        .expect("rsl-usdr accepts the step");
        actual_windows.push(start..sim.board().trace().len());
    }
    let board = sim.board();

    for ((step, expected), actual) in steps.iter().zip(expected_windows).zip(actual_windows) {
        let expected: &[Op] = &reference.trace()[expected];
        let actual: &[Op] = &board.trace()[actual];
        assert_eq!(expected, actual, "{revision:?} {step:?}");
    }
}

#[test]
fn switching_to_an_external_reference_and_back_matches_libusdr() {
    assert_steps_match(
        BoardRevision::Rev3,
        &[
            Step::Reference(ReferenceClock::External { hz: 40_000_000 }),
            Step::Rate(20_000_000),
            Step::Reference(ReferenceClock::Oscillator),
            Step::Rate(20_000_000),
        ],
    );
}

#[test]
fn an_external_reference_feeds_the_next_sample_clock_plan() {
    assert_steps_match(
        BoardRevision::Rev1,
        &[
            Step::Reference(ReferenceClock::External { hz: 30_720_000 }),
            Step::Rate(1_000_000),
            Step::Rate(62_000_000),
        ],
    );
}
