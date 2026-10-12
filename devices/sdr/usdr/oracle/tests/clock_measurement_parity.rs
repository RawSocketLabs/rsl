//! rsl-usdr's raw clock-measurement access against libusdr's `/dm/sdr/0/clkmeas`.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use rsl_usdr::Device;
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

#[test]
fn clock_measurement_writes_and_reads_match_libusdr() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    let start = oracle.trace_len();
    oracle.set_clock_measurement(0x5a).expect("libusdr writes");
    let expected_value = oracle
        .get_uint(c"/dm/sdr/0/clkmeas")
        .expect("libusdr reads");
    let end = oracle.trace_len();
    let reference = oracle.close();

    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    let actual_start = sim.board().trace().len();
    device.set_clock_measurement(0x5a).expect("rsl-usdr writes");
    let value = device.clock_measurement().expect("rsl-usdr reads");
    let board = sim.board();

    assert_eq!(u64::from(value), expected_value, "value");
    let expected: &[Op] = &reference.trace()[start..end];
    let actual: &[Op] = &board.trace()[actual_start..];
    assert_eq!(expected, actual, "operations");
}
