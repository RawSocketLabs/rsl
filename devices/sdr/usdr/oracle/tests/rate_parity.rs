//! rsl-usdr's sample-rate changes, for RX, TX or both chains, against libusdr's, operation
//! for operation.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ops::Range;

use rsl_usdr::{Device, Error};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

/// The RX chain's configuration port (`REG_CFG_PHY_0`).
const REG_RX_PHY: u32 = 56;

/// The TX chain's configuration port (`REG_CFG_PHY_1`).
const REG_TX_PHY: u32 = 57;

/// The FIR-load sub-register of a configuration port (`CFG_REG_DSP_LD`), in bits 30:24.
const FIR_LOAD: u32 = 64;

/// One call, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// libusdr's `/dm/rate/rxtxadcdac` with `{rate, 0}`; rsl-usdr's `set_rx_sample_rate`.
    Rx(u32),

    /// libusdr's `/dm/rate/rxtxadcdac` with `{0, rate}`; rsl-usdr's `set_tx_sample_rate`.
    Tx(u32),

    /// libusdr's `/dm/rate/master`; rsl-usdr's `set_sample_rate`.
    Both(u32),

    /// `set_tx_frequency`, every channel.
    TxFrequency(u32),
}

impl Call {
    /// Makes the call through libusdr.
    fn on_libusdr(self, oracle: &mut Oracle) -> Result<(), i32> {
        match self {
            Self::Rx(rate) => oracle.set_rx_rate(rate),
            Self::Tx(rate) => oracle.set_rates(0, rate),
            Self::Both(rate) => oracle.set_master_rate(rate),
            Self::TxFrequency(hz) => oracle.set_tx_frequency(hz),
        }
    }

    /// Makes the call through rsl-usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rx(rate) => device.set_rx_sample_rate(rate),
            Self::Tx(rate) => device.set_tx_sample_rate(rate),
            Self::Both(rate) => device.set_sample_rate(rate),
            Self::TxFrequency(hz) => device.set_tx_frequency(hz),
        }
    }
}

/// Sets each of `rates`, RX rates, in turn on fresh boards through libusdr and rsl-usdr,
/// and compares the operations of every call.
fn assert_rate_changes_match(revision: BoardRevision, rates: &[u32]) {
    let calls: Vec<Call> = rates.iter().copied().map(Call::Rx).collect();
    assert_calls_match_on(revision, || SimBoard::new(revision), &calls);
}

/// Makes `calls` in turn on fresh boards made by `board`, through libusdr and rsl-usdr,
/// and compares the operations of every call. Returns libusdr's trace and each call's
/// window in it.
fn assert_calls_match_on(
    revision: BoardRevision,
    board: impl Fn() -> SimBoard,
    calls: &[Call],
) -> (Vec<Op>, Vec<Range<usize>>) {
    let mut oracle = Oracle::open(board()).expect("libusdr opens the board");
    let mut expected_windows: Vec<Range<usize>> = Vec::new();
    for &call in calls {
        let start = oracle.trace_len();
        if let Err(errno) = call.on_libusdr(&mut oracle) {
            panic!("libusdr refuses {call:?}: {errno}");
        }
        expected_windows.push(start..oracle.trace_len());
    }
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up the board");
    let mut actual_windows: Vec<Range<usize>> = Vec::new();
    for &call in calls {
        let start = sim.board().trace().len();
        if let Err(error) = call.on_rsl(&mut device) {
            panic!("rsl-usdr refuses {call:?}: {error}");
        }
        actual_windows.push(start..sim.board().trace().len());
    }
    let board = sim.board();

    for ((call, expected), actual) in calls.iter().zip(&expected_windows).zip(actual_windows) {
        let expected: &[Op] = &reference.trace()[expected.clone()];
        let actual: &[Op] = &board.trace()[actual];
        if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
            panic!(
                "{revision:?} {call:?}: first difference at op {i}: libusdr {:x?}, rsl-usdr {:x?}",
                expected[i], actual[i]
            );
        }
        assert_eq!(
            expected.len(),
            actual.len(),
            "{revision:?} {call:?}: same prefix, different lengths"
        );
    }
    (reference.trace().to_vec(), expected_windows)
}

/// As [`assert_calls_match_on`], on fresh revision-3 boards.
fn assert_calls_match(calls: &[Call]) -> (Vec<Op>, Vec<Range<usize>>) {
    assert_calls_match_on(
        BoardRevision::Rev3,
        || SimBoard::new(BoardRevision::Rev3),
        calls,
    )
}

/// How many FIR microcode bytes `ops` loads at configuration port `port`: writes to its
/// FIR-load sub-register.
fn fir_bytes(ops: &[Op], port: u32) -> usize {
    ops.iter()
        .filter(|op| matches!(op, Op::RegWrite { addr, value } if *addr == port && value >> 24 == FIR_LOAD))
        .count()
}

/// How many TX FIR microcode bytes `ops` loads.
fn tx_fir_bytes(ops: &[Op]) -> usize {
    fir_bytes(ops, REG_TX_PHY)
}

#[test]
fn rev1_rate_changes_match_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev1, &[1_000_000]);
}

#[test]
fn rev2_rate_changes_match_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev2, &[20_000_000]);
}

/// ×32, ×2 and ×1 decimation; the I/O rail boosted at 62 MS/s and lowered again; the RX
/// filter from its narrowest setting to bypassed.
#[test]
fn rev3_rate_changes_match_libusdr() {
    assert_rate_changes_match(
        BoardRevision::Rev3,
        &[1_000_000, 20_000_000, 62_000_000, 20_000_000],
    );
}

/// 10 and 15 MS/s both decimate by 4, so the second change reloads no FIR.
#[test]
fn rev3_same_decimation_skips_the_fir_like_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev3, &[10_000_000, 15_000_000]);
}

/// 1.002 MS/s exercises the Si5332 denominator search's 32-bit wrap.
#[test]
fn rev3_wrapped_pll_denominator_matches_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev3, &[1_002_000]);
}

/// x8 at 4 MS/s; x16 at 1.9 MS/s, where x32 would overshoot 60 MS/s and libusdr steps back.
/// With the tests above, every RX FIR table passes through the comparison.
#[test]
fn rev3_remaining_decimations_match_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev3, &[4_000_000, 1_900_000]);
}

/// Without a TX chain libusdr never decimates, so the sample clock runs at twice the
/// requested rate: 2 MHz (slowest slew), 26 MHz (the reference itself, so the outputs
/// bypass the PLL, slew 2) and 160 MHz (fastest slew, I/O rail boosted).
#[test]
fn rev3_rx_only_rate_changes_match_libusdr() {
    let calls = [1_000_000, 13_000_000, 80_000_000].map(Call::Rx);
    assert_calls_match_on(
        BoardRevision::Rev3,
        || SimBoard::new(BoardRevision::Rev3).with_rx_only(),
        &calls,
    );
}

/// TX rates on a fresh board load every TX interpolation table (×32, ×16, ×8, ×4, ×2,
/// then ×1 with the I/O rail boosted), each once, 1280 bytes apiece.
#[test]
fn tx_rate_changes_load_every_interpolation_table_like_libusdr() {
    let rates = [
        1_000_000, 1_900_000, 4_000_000, 10_000_000, 20_000_000, 62_000_000,
    ];
    let (trace, windows) = assert_calls_match(&rates.map(Call::Tx));
    for (rate, window) in rates.iter().zip(windows) {
        assert_eq!(
            tx_fir_bytes(&trace[window]),
            1280,
            "{rate} S/s loads one TX table"
        );
    }
}

/// libusdr records every rate's factor for both chains, so a TX rate after an RX rate of
/// the same factor loads no TX table, and one of another factor does.
#[test]
fn a_tx_rate_after_an_rx_rate_of_the_same_factor_loads_no_table_like_libusdr() {
    let (trace, windows) = assert_calls_match(&[
        Call::Rx(20_000_000),
        Call::Tx(20_000_000),
        Call::Tx(10_000_000),
    ]);
    let loaded: Vec<usize> = windows
        .into_iter()
        .map(|w| tx_fir_bytes(&trace[w]))
        .collect();
    assert_eq!(loaded, [0, 0, 1280]);
}

/// The same the other way round: a TX rate first records its factor for RX too, so an RX
/// rate of that factor on a fresh board loads no RX decimator table at all (libusdr's
/// hazard, kept; `Device::set_rx_sample_rate` documents it).
#[test]
fn an_rx_rate_after_a_tx_rate_of_the_same_factor_loads_no_table_like_libusdr() {
    let (trace, windows) = assert_calls_match(&[Call::Tx(20_000_000), Call::Rx(20_000_000)]);
    let loaded: Vec<(usize, usize)> = windows
        .into_iter()
        .map(|w| {
            (
                fir_bytes(&trace[w.clone()], REG_RX_PHY),
                tx_fir_bytes(&trace[w]),
            )
        })
        .collect();
    assert_eq!(loaded, [(0, 1280), (0, 0)], "(RX, TX) table bytes per call");
}

/// Both chains at once, as libusdr's `/dm/rate/master`; a TX tune; then a TX rate, which
/// makes the TX filter follow it, and an RX rate, which leaves it.
#[test]
fn both_chains_and_tx_bandwidth_following_match_libusdr() {
    assert_calls_match(&[
        Call::Both(20_000_000),
        Call::TxFrequency(1_000_000_000),
        Call::Tx(4_000_000),
        Call::Rx(10_000_000),
        Call::Both(1_000_000),
    ]);
}

/// libusdr's `{rx, tx}` with two different rates runs both chains at the RX rate: the
/// same operations as both chains at that rate (characterization of why rsl-usdr offers
/// no separate rates).
#[test]
fn different_rx_and_tx_rates_are_libusdrs_rx_rate_for_both() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    let start = oracle.trace_len();
    oracle
        .set_rates(20_000_000, 4_000_000)
        .expect("libusdr accepts two rates");
    let end = oracle.trace_len();
    let mixed = oracle.close().trace()[start..end].to_vec();

    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    let start = oracle.trace_len();
    oracle
        .set_rates(20_000_000, 20_000_000)
        .expect("libusdr accepts the rate");
    let end = oracle.trace_len();
    let both = oracle.close().trace()[start..end].to_vec();
    assert_eq!(mixed, both, "the TX rate changes nothing");
}

/// libusdr refuses two zero rates and out-of-range ones before any operation; rsl-usdr
/// cannot express the first, and refuses the others as libusdr does, untouched.
#[test]
fn out_of_range_rates_are_refused_by_both_before_any_operation() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    let start = oracle.trace_len();
    assert_eq!(oracle.set_rates(0, 0), Err(-22), "-EINVAL");
    assert_eq!(oracle.set_rates(0, 80_000_001), Err(-34), "-ERANGE");
    assert_eq!(oracle.set_master_rate(80_000_001), Err(-34), "-ERANGE");
    assert_eq!(oracle.trace_len(), start, "libusdr refused untouched");
    drop(oracle.close());

    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up");
    let start = sim.board().trace().len();
    for result in [
        device.set_tx_sample_rate(80_000_001),
        device.set_sample_rate(80_000_001),
    ] {
        assert!(matches!(
            result,
            Err(Error::UnsupportedSampleRate(80_000_001))
        ));
    }
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}
