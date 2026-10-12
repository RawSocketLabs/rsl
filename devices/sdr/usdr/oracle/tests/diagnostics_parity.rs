//! rsl-usdr's raw diagnostic registers against libusdr's: the ATCRBS core register and
//! `/dm/debug/all`'s TX DMA status words.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use rsl_usdr::Device;
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

#[test]
fn diagnostic_registers_match_libusdr() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    let start = oracle.trace_len();
    oracle
        .set_atcrbs_register(0x1234_5678)
        .expect("libusdr writes");
    let atcrbs = oracle
        .get_uint(c"/dm/sdr/0/core/atcrbs/reg")
        .expect("libusdr reads");
    let debug = oracle.debug_all().expect("libusdr reads the status");
    let end = oracle.trace_len();
    let reference = oracle.close();

    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    let actual_start = sim.board().trace().len();
    device
        .set_atcrbs_register(0x1234_5678)
        .expect("rsl-usdr writes");
    let value = device.atcrbs_register().expect("rsl-usdr reads");
    let status = device.tx_dma_status().expect("rsl-usdr reads the status");
    let board = sim.board();

    assert_eq!(u64::from(value), atcrbs, "ATCRBS value");
    let packed = [
        u64::from(status[1]) << 32 | u64::from(status[0]),
        u64::from(status[3]) << 32 | u64::from(status[2]),
    ];
    assert_eq!(packed, debug, "status words, as libusdr packs them");
    let expected: &[Op] = &reference.trace()[start..end];
    let actual: &[Op] = &board.trace()[actual_start..];
    assert_eq!(expected, actual, "operations");
}
