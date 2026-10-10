//! The band-crossing fix on the simulated board: moving into or out of the mixer band
//! switches Si5332 output 3 without stopping the clock generator.

mod common {
    pub(crate) mod sim_bus;
}

use common::sim_bus::SimBus;
use rsl_usdr::Device;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};

/// The Si5332's 7-bit I2C address.
const CLOCK_ADDR: u16 = 0x6a;
/// Si5332 `OUT3210_OE`.
const OUTPUT_ENABLES: u8 = 0xb6;

/// The bytes of every Si5332 operation `call` makes: `[register, value]` for a write, the
/// register alone for a read.
fn clock_operations(sim: &SimBus, call: impl FnOnce()) -> Vec<Vec<u8>> {
    let start = sim.board().trace().len();
    call();
    sim.board().trace()[start..]
        .iter()
        .filter_map(|op| match op {
            Op::I2c { addr, write, .. } if addr.addr == CLOCK_ADDR && !write.is_empty() => {
                Some(write.clone())
            }
            _ => None,
        })
        .collect()
}

/// Entering and leaving the mixer band touch the Si5332 once each: `OUT3_OE` on or off,
/// with no READY request to stop its clocks.
#[test]
fn crossing_the_mixer_band_only_flips_output_3() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");

    let entering = clock_operations(&sim, || {
        device
            .set_rx_frequency(100_000_000)
            .expect("tunes into the mixer band");
    });
    let leaving = clock_operations(&sim, || {
        device
            .set_rx_frequency(1_000_000_000)
            .expect("tunes out of it");
    });

    assert_eq!(
        entering,
        [vec![OUTPUT_ENABLES, 0xff]],
        "output 3 on, and no READY request that would stop the clocks"
    );
    assert_eq!(
        leaving,
        [vec![OUTPUT_ENABLES, 0xbf]],
        "output 3 off, nothing else"
    );
}
