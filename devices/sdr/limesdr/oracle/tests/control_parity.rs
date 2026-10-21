//! limesdr's sample rate, LO tuning, gains, bandwidths and stream preparation against
//! libusdr's, operation for operation (frames and the register operations they decode to),
//! on fresh simulated LimeSDR-minis: everything between the open and the close.
//!
//! The sim's LMS7002M answers the VCO comparators from the board's 40 MHz reference
//! (`limesdr-sim`, `lms7002m-sim`). libusdr prepares streaming when it creates a
//! stream, then sets the transport's stream up: an RX stream's creation is compared, and
//! a second one in libusdr (which keeps both) is limesdr's destroy, then create.
//!
//! No divergence is whitelisted here: neither of limesdr's on these paths can be shown
//! against libusdr. Refusing to stream up when the default rate did not lock, where
//! libusdr divides by zero, cannot run in libusdr (`tests/control.rs` covers it on the sim
//! alone); stopping at a bus failure, where libusdr carries on, needs a failing bus, which
//! the sim does not model.

#[path = "../../tests/common/sim_port.rs"]
mod sim_port;

use std::ffi::CStr;

use limesdr::{Device, Direction, Error, Gain};
use limesdr_oracle::Oracle;
use limesdr_sim::{Op, SimBoard};
use sim_port::SimPort;

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ERANGE`.
const ERANGE: i32 = 34;
/// `EIO`.
const EIO: i32 = 5;
/// Samples per packet of the RX streams created: one burst.
const RX_PACKET: u32 = 1020;
/// The RF switches, gateware register 0x0017.
const RF_SWITCHES: u16 = 0x0017;

/// One device-manager call, as libusdr's path and limesdr's method.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `/dm/rate/master`: both directions at one rate.
    Rate(u32),
    /// `/dm/rate/rxtxadcdac`: RX and TX rates.
    Rates(u32, u32),
    /// `/dm/sdr/0/rx/frequency`.
    Rx(u32),
    /// `/dm/sdr/0/tx/frequency`.
    Tx(u32),
    /// A gain path and value.
    Gain(Gain, i32),
    /// A bandwidth path and value.
    Bandwidth(Direction, u32),
    /// An RX stream's creation, which prepares streaming.
    Stream,
}

/// libusdr's device-manager path for a gain.
fn gain_path(gain: Gain) -> &'static CStr {
    match gain {
        Gain::RxLna => c"/dm/sdr/0/rx/gain/lna",
        Gain::RxTia => c"/dm/sdr/0/rx/gain/vga",
        Gain::RxPga => c"/dm/sdr/0/rx/gain/pga",
        Gain::RxLoopback => c"/dm/sdr/0/rx/gain/lb",
        Gain::TxPad => c"/dm/sdr/0/tx/gain",
        Gain::TxLoopback => c"/dm/sdr/0/tx/gain/lb",
    }
}

/// Sets a numeric device-manager path.
fn set_uint(oracle: &mut Oracle, path: &CStr, value: u64) -> Result<(), i32> {
    // SAFETY: every path these tests set (rate master, frequencies, gains, bandwidths)
    // takes its value as a number, not a pointer.
    unsafe { oracle.set_uint(path, value) }
}

/// libusdr's results and the operations between its open and close.
fn libusdr(board: SimBoard, calls: &[Call]) -> (Vec<i32>, Vec<Op>) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let results = calls
        .iter()
        .map(|&call| {
            let result = match call {
                Call::Rate(rate) => set_uint(&mut oracle, c"/dm/rate/master", rate.into()),
                Call::Rates(rx, tx) => oracle.set_rates(rx, tx),
                Call::Rx(hz) => set_uint(&mut oracle, c"/dm/sdr/0/rx/frequency", hz.into()),
                Call::Tx(hz) => set_uint(&mut oracle, c"/dm/sdr/0/tx/frequency", hz.into()),
                // The handlers take the u64 as an `int`: two's complement.
                Call::Gain(gain, value) => set_uint(
                    &mut oracle,
                    gain_path(gain),
                    u64::from_ne_bytes(i64::from(value).to_ne_bytes()),
                ),
                Call::Bandwidth(Direction::Rx, hz) => {
                    set_uint(&mut oracle, c"/dm/sdr/0/rx/bandwidth", hz.into())
                }
                Call::Bandwidth(Direction::Tx, hz) => {
                    set_uint(&mut oracle, c"/dm/sdr/0/tx/bandwidth", hz.into())
                }
                Call::Stream => oracle.create_rx_stream(RX_PACKET),
            };
            result.err().unwrap_or(0)
        })
        .collect();
    let used = oracle.trace_len();
    let board = oracle.close();
    (results, board.trace()[opened..used].to_vec())
}

/// limesdr's results, as libusdr's errnos, and its operations after the open.
fn rsl(board: SimBoard, calls: &[Call]) -> (Vec<i32>, Vec<Op>) {
    let sim = SimPort::new(board);
    let mut device = Device::open(sim.clone()).expect("limesdr opens the board");
    let opened = sim.board().trace().len();
    let results = calls
        .iter()
        .map(|&call| {
            let result = match call {
                Call::Rate(rate) => device.set_sample_rate(rate, rate),
                Call::Rates(rx, tx) => device.set_sample_rate(rx, tx),
                Call::Rx(hz) => device.set_rx_frequency(hz),
                Call::Tx(hz) => device.set_tx_frequency(hz),
                Call::Gain(gain, value) => device.set_gain(gain, value).map(|_| ()),
                Call::Bandwidth(direction, hz) => device.set_bandwidth(direction, hz),
                Call::Stream => {
                    // libusdr's unregister forgets a stream without a bus operation.
                    let _ = device.destroy_rx_stream();
                    device.create_rx_stream(RX_PACKET)
                }
            };
            errno(result)
        })
        .collect();
    device.close().expect("limesdr's close does nothing");
    let trace = sim.board().trace()[opened..].to_vec();
    (results, trace)
}

/// The errno libusdr returns where limesdr returns `result`.
fn errno(result: Result<(), Error>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(Error::InvalidRate(0) | Error::UndeliverableRate) => -EINVAL,
        Err(Error::InvalidRate(_) | Error::ClockUnlocked | Error::TuneOutOfRange(_)) => -ERANGE,
        Err(Error::VcoComparatorFailed) => -EIO,
        Err(other) => panic!("no libusdr counterpart: {other}"),
    }
}

/// Both drivers' results and operations on boards `board` builds, which must agree;
/// libusdr's.
fn assert_same_on(
    board: &dyn Fn() -> SimBoard,
    calls: &[Call],
    scenario: &str,
) -> (Vec<i32>, Vec<Op>) {
    let (expected, actual) = (libusdr(board(), calls), rsl(board(), calls));
    assert_eq!(actual.0, expected.0, "{scenario}: results");
    if let Some(at) =
        (0..expected.1.len().max(actual.1.len())).find(|&i| expected.1.get(i) != actual.1.get(i))
    {
        panic!(
            "{scenario}: limesdr diverges at op {at} of {}: libusdr {}, limesdr {}",
            expected.1.len(),
            expected.1.get(at).map_or("(end)".into(), Op::to_string),
            actual.1.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
    expected
}

/// [`assert_same_on`] on fresh default boards: libusdr's results.
fn assert_same(calls: &[Call], scenario: &str) -> Vec<i32> {
    assert_same_on(&SimBoard::new, calls, scenario).0
}

/// The values a trace writes to gateware register `reg`, in order.
fn fpga_writes(ops: &[Op], reg: u16) -> Vec<u16> {
    ops.iter()
        .filter_map(|op| match *op {
            Op::FpgaWrite { addr, value } if addr == reg => Some(value),
            _ => None,
        })
        .collect()
}

#[test]
fn a_session_of_rates_tunings_gains_and_bandwidths_matches_libusdr() {
    let results = assert_same(
        &[
            Call::Rx(900_000_000),
            Call::Tx(1_500_000_000),
            Call::Rate(1_000_000),
            Call::Rate(10_000_000),
            Call::Rx(900_000_000),
            Call::Rx(2_400_000_000),
            Call::Rx(0),
            Call::Rx(4_100_000_000),
            Call::Gain(Gain::RxLna, 15),
            Call::Gain(Gain::RxLna, 30),
            Call::Gain(Gain::RxTia, -3),
            Call::Gain(Gain::RxPga, 12),
            Call::Gain(Gain::RxPga, 40),
            Call::Gain(Gain::RxLoopback, -20),
            Call::Gain(Gain::TxPad, -20),
            Call::Gain(Gain::TxPad, 5),
            Call::Gain(Gain::TxLoopback, -22),
            Call::Bandwidth(Direction::Rx, 5_000_000),
            Call::Bandwidth(Direction::Tx, 15_000_000),
            Call::Bandwidth(Direction::Rx, 30_000_000),
            Call::Bandwidth(Direction::Tx, 0),
            Call::Rate(30_720_000),
        ],
        "session",
    );
    assert_eq!(results[7], -ERANGE, "no VCO reaches a 4.1 GHz LO");
    assert_eq!(
        results.iter().filter(|&&r| r == 0).count(),
        results.len() - 1,
        "everything else succeeds"
    );
}

#[test]
fn each_rate_matches_libusdr_on_a_fresh_board() {
    for rate in [
        90_000,
        1_000_000,
        2_000_000,
        7_680_000,
        15_360_000,
        30_720_000,
        61_440_000,
        80_000_000,
        122_880_000,
    ] {
        let results = assert_same(&[Call::Rate(rate)], &format!("{rate} S/s"));
        assert_eq!(results, [0], "{rate} S/s: libusdr's outcome");
    }
}

#[test]
fn unequal_and_one_sided_rates_match_libusdr() {
    for (rx, tx) in [
        (1_000_000, 3_000_000),
        (0, 5_000_000),
        (5_000_000, 0),
        (40_000_000, 20_000_000),
    ] {
        assert_same(&[Call::Rates(rx, tx)], &format!("RX {rx}, TX {tx}"));
    }
}

#[test]
fn rates_out_of_range_are_refused_before_any_access_like_libusdr() {
    let results = assert_same(
        &[
            Call::Rate(89_999),
            Call::Rate(122_880_001),
            Call::Rates(0, 0),
            Call::Rates(1_000_000, 200_000_000),
        ],
        "refused rates",
    );
    assert_eq!(results, [-ERANGE, -ERANGE, -EINVAL, -ERANGE]);
}

#[test]
fn preparing_streams_on_a_fresh_board_sets_the_default_rate_like_libusdr() {
    let (results, ops) = assert_same_on(&SimBoard::new, &[Call::Stream], "fresh board");
    assert_eq!(results, [0]);
    assert_eq!(
        fpga_writes(&ops, 0x0005),
        [0x0001, 0x0003],
        "the default rate's direct clocking, before streaming up"
    );
    assert_eq!(
        fpga_writes(&ops, RF_SWITCHES).len(),
        2,
        "RX then TX switched"
    );
}

#[test]
fn streams_prepared_after_a_rate_tunings_and_bandwidths_match_libusdr() {
    assert_same(
        &[
            Call::Rx(2_400_000_000),
            Call::Tx(2_400_000_000),
            Call::Rate(5_000_000),
            Call::Bandwidth(Direction::Rx, 3_000_000),
            Call::Stream,
        ],
        "after a rate",
    );
}

#[test]
fn bands_and_antenna_switches_follow_the_los_once_streaming_like_libusdr() {
    // Bits other than the two switches' survive their read-modify-writes.
    let board = || SimBoard::new().with_fpga_reg(RF_SWITCHES, 0xcccc);
    let calls = [
        Call::Stream,
        Call::Rx(1_000_000_000),
        Call::Rx(1_700_000_000),
        Call::Rx(3_500_000_000),
        Call::Tx(1_000_000_000),
        Call::Tx(2_000_000_000),
        Call::Rate(10_000_000),
        Call::Gain(Gain::TxPad, -10),
        Call::Bandwidth(Direction::Tx, 25_000_000),
        Call::Stream,
    ];
    let (results, ops) = assert_same_on(&board, &calls, "band switching");
    assert!(results.iter().all(|&r| r == 0), "{results:?}");
    assert_eq!(
        fpga_writes(&ops, RF_SWITCHES),
        [
            // Streaming up with both LOs at 0: LNAW (RX 2), band 2 (TX 2).
            0xcecc, 0xeecc, //
            // RX 1 GHz and 1.7 GHz: LNAW, then LNAH (1); 3.5 GHz: LNAH.
            0xeecc, 0xedcc, 0xedcc, //
            // TX 1 GHz: band 2; 2 GHz: band 1 (1).
            0xedcc, 0xddcc, //
            // Streaming up again: the bands of the LOs tuned.
            0xddcc, 0xddcc,
        ],
        "each switch's two bits follow its LO; the rest survive"
    );
}

#[test]
fn a_failing_vco_comparator_fails_tuning_and_the_rate_like_libusdr() {
    let (results, _) = assert_same_on(
        &|| SimBoard::new().with_lms7_comparator_failure(128),
        &[Call::Rx(1_000_000_000), Call::Rate(1_000_000)],
        "comparator failure",
    );
    assert_eq!(
        results,
        [-EIO, -ERANGE],
        "SXX fails with -EIO; every CGEN attempt fails, so the rate -ERANGE"
    );
}
