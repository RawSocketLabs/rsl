//! The harness for stream parity: libusdr runs a whole RX stream lifecycle on the sim, and
//! each call's operations can be isolated.
//!
//! rsl-usdr has no stream API yet; these checks pin where libusdr's calls end, from
//! `stream_sfetrx4_dma32.c` and `m2_lm6_1.c`, so the parity test that follows the port
//! compares exactly those windows.

use std::ops::Range;

use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};

/// The RX DSP chain's configuration port (`REG_CFG_PHY_0`).
const PHY_RX: u32 = 56;
/// The RX DMA control register.
const RX_DMA_CONTROL: u32 = 5;
/// The RX front end's command register (`CSR_RFE4_BASE`).
const RX_FRONT_END: u32 = 0x1_0100;
/// The stream sync register.
const SYNC: u32 = 14;

/// A register write.
fn write(addr: u32, value: u32) -> Op {
    Op::RegWrite { addr, value }
}

#[test]
fn a_stream_lifecycle_runs_on_the_sim() {
    let mut oracle =
        Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens the board");
    oracle.set_rx_rate(20_000_000).expect("sets the rate");
    oracle
        .set_rx_frequency(1_000_000_000)
        .expect("tunes the receiver");

    let mut windows: Vec<(&str, Range<usize>)> = Vec::new();
    let mut step = |name, oracle: &mut Oracle, call: fn(&mut Oracle) -> Result<(), i32>| {
        let start = oracle.trace_len();
        call(oracle).unwrap_or_else(|errno| panic!("{name}: errno {errno}"));
        windows.push((name, start..oracle.trace_len()));
    };
    step("create", &mut oracle, |o| o.create_rx_stream(1024));
    step("start", &mut oracle, Oracle::start_rx_stream);
    step("sync", &mut oracle, Oracle::sync_free_run);
    step("stop", &mut oracle, Oracle::stop_rx_stream);
    step("destroy", &mut oracle, Oracle::destroy_rx_stream);
    let board = oracle.close();

    let tail = |name: &str, len: usize| -> Vec<Op> {
        let (_, range) = windows
            .iter()
            .find(|(window, _)| *window == name)
            .expect("the step ran");
        board.trace()[range.end - len..range.end].to_vec()
    };
    // `usdr_rxupdate_cal`: IQ correction defaults, the last thing create does.
    assert_eq!(
        tail("create", 4),
        [
            write(PHY_RX, 0x023f_ffff),
            write(PHY_RX, 0x033f_ffff),
            write(PHY_RX, 0x0400_0000),
            write(PHY_RX, 0x0500_0000),
        ]
    );
    // `_sfetrx4_op`: DMA on, then the front end starts at once.
    assert_eq!(
        tail("start", 2),
        [write(RX_DMA_CONTROL, 1), write(RX_FRONT_END, 0x3000_0002)]
    );
    // `usdr_device_m2_lm6_1_sync` for "none": free-run.
    assert_eq!(tail("sync", 1), [write(SYNC, 0x8007_0000)]);
    // Stop and destroy both end with DMA off, then the front end stops at once.
    let halt = [write(RX_DMA_CONTROL, 0), write(RX_FRONT_END, 0x3000_0004)];
    assert_eq!(tail("stop", 2), halt);
    assert_eq!(tail("destroy", 2), halt);
}
