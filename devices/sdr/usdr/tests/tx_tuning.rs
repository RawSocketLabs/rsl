//! Tuning before any sample rate, on the simulated board: where libusdr divides by zero
//! (`_usdr_update_bandwidth`, and the low-LO fallback's room check), so the oracle cannot
//! run it, usdr leaves the filter alone and fails the fallback. Both chains share the
//! code; the TX chain is checked here.

mod common {
    pub(crate) mod sim_bus;
}

use common::sim_bus::SimBus;
use usdr::{Device, Error};
use usdr_sim::{BoardRevision, Op, PllLock, SimBoard};

/// The TX filter's bandwidth, bypass and control registers (`TxLPF` 0x34 to 0x36).
const TX_FILTER: [u32; 3] = [0x34, 0x35, 0x36];

/// The LMS6002D registers `call` writes, in order.
fn lms_writes(sim: &SimBus, call: impl FnOnce()) -> Vec<u32> {
    let start = sim.board().trace().len();
    call();
    sim.board().trace()[start..]
        .iter()
        .filter_map(|op| match op {
            Op::Spi { bus: 0, out, .. } if out & 0x8000 != 0 => Some(out >> 8 & 0x7f),
            _ => None,
        })
        .collect()
}

/// With no sample rate there is no rate for the filter to follow, so the tune leaves it
/// alone; the PLL still tunes.
#[test]
fn a_tx_tune_before_any_rate_leaves_the_filter_alone() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    let writes = lms_writes(&sim, || {
        device
            .set_tx_frequency(1_000_000_000)
            .expect("tunes without a rate");
    });
    assert!(
        writes.iter().any(|reg| (0x10..=0x1f).contains(reg)),
        "the TX PLL tunes: {writes:x?}"
    );
    assert!(
        !writes.iter().any(|reg| TX_FILTER.contains(reg)),
        "no TX filter write without a rate: {writes:x?}"
    );
}

/// With no sample rate the NCOs have no room to measure, so an LO the PLL cannot reach
/// fails, naming the requested LO.
#[test]
fn a_tx_low_lo_fallback_before_any_rate_fails() {
    let board = SimBoard::new(BoardRevision::Rev3).with_tx_pll_lock(PllLock {
        unlocked_below_hz: 245_000_000,
        ..PllLock::default()
    });
    let mut device = Device::with_bus(SimBus::new(board)).expect("powers up");
    let result = device.set_tx_frequency(240_000_000);
    assert!(
        matches!(result, Err(Error::PllUnlocked(240_000_000))),
        "the fallback has no rate to fit the offset in: {result:?}"
    );
}
