//! rsl-limesdr's open, power-up and close against libusdr's, operation for operation
//! (frames and the register operations they decode to), on fresh simulated boards.
//!
//! Whitelisted divergences, each confined to the scenario that shows it:
//! - References libusdr's broken table walk refuses (10, 38.4 and 52 MHz measured): libusdr
//!   stops with `-EFAULT` after the count read; rsl-limesdr recognises them and carries on
//!   exactly as from a 40 MHz count.
//! - A detector that never finishes: after the same ten polls and waits libusdr reads
//!   whatever count the registers hold, here a stale 40 MHz one it then opens with;
//!   rsl-limesdr stops before that read. The stale count is seeded on purpose: a zero
//!   count makes libusdr's walk stop at its first entry and log `clkTbl[i - 1]` with
//!   unsigned `i` = 0, an out-of-bounds read that may crash the oracle.

#[path = "../../tests/common/sim_port.rs"]
mod sim_port;

use rsl_limesdr::{Device, Error};
use rsl_limesdr_oracle::Oracle;
use rsl_limesdr_sim::{DETECTOR_GATE_US, Op, SimBoard};
use sim_port::SimPort;

/// `EIO`: `limesdr_init`'s refusal of an LMS7002M version word other than 0x3841.
const EIO: i32 = 5;
/// `EFAULT`: `limesdr_detect_refclk`'s refusal of an unrecognised reference.
const EFAULT: i32 = 14;
/// The detector count's two registers, as the sim holds them for a 40 MHz reference.
const STALE_40_MHZ: [(u16, u16); 2] = [(0x0072, 0x6664), (0x0073, 0x0066)];

/// One driver's session on a board: the open's operations and outcome, then the power-up's
/// and the close's (empty when the open failed).
struct Session {
    /// The open's operations.
    open: Vec<Op>,
    /// The open's outcome: the nominal reference recognised (rsl-limesdr; libusdr does
    /// not expose its own, so `None`), or the device's own errno (libusdr) or 0
    /// (rsl-limesdr).
    outcome: Result<Option<u32>, i32>,
    /// The power-up's operations.
    power_up: Vec<Op>,
    /// The close's operations.
    close: Vec<Op>,
}

/// libusdr's session on `board`: open, `/dm/power/en` = 1, close.
fn libusdr(board: SimBoard) -> Session {
    let mut oracle = match Oracle::open(board) {
        Ok(oracle) => oracle,
        Err(refused) => {
            return Session {
                open: refused.board.trace().to_vec(),
                outcome: Err(refused.device_errno),
                power_up: Vec::new(),
                close: Vec::new(),
            };
        }
    };
    let opened = oracle.trace_len();
    oracle.power_up().expect("libusdr accepts /dm/power/en");
    let powered = oracle.trace_len();
    let board = oracle.close();
    let trace = board.trace();
    Session {
        open: trace[..opened].to_vec(),
        outcome: Ok(None),
        power_up: trace[opened..powered].to_vec(),
        close: trace[powered..].to_vec(),
    }
}

/// rsl-limesdr's session on `board`, as [`libusdr`]'s; also its open's error.
fn rsl_limesdr(board: SimBoard) -> (Session, Option<Error>) {
    let sim = SimPort::new(board);
    let mut device = match Device::open(sim.clone()) {
        Ok(device) => device,
        Err(error) => {
            let session = Session {
                open: sim.board().trace().to_vec(),
                outcome: Err(0),
                power_up: Vec::new(),
                close: Vec::new(),
            };
            return (session, Some(error));
        }
    };
    let reference_hz = device.reference_hz();
    let opened = sim.board().trace().len();
    device
        .power_up()
        .expect("rsl-limesdr's power-up does nothing");
    let powered = sim.board().trace().len();
    device.close().expect("rsl-limesdr's close does nothing");
    let board = sim.board();
    let trace = board.trace();
    let session = Session {
        open: trace[..opened].to_vec(),
        outcome: Ok(Some(reference_hz)),
        power_up: trace[opened..powered].to_vec(),
        close: trace[powered..].to_vec(),
    };
    (session, None)
}

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: rsl-limesdr diverges from libusdr at op {at}: libusdr {}, rsl-limesdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// Both drivers' whole sessions on boards from `board`, which must agree: rsl-limesdr's
/// reference, or its open's error.
fn assert_same_session(board: impl Fn() -> SimBoard, scenario: &str) -> Result<u32, Error> {
    let reference = libusdr(board());
    let (ours, error) = rsl_limesdr(board());
    assert_same_ops(&reference.open, &ours.open, &format!("{scenario}: open"));
    assert_same_ops(
        &reference.power_up,
        &ours.power_up,
        &format!("{scenario}: power-up"),
    );
    assert_same_ops(&reference.close, &ours.close, &format!("{scenario}: close"));
    assert_eq!(
        reference.outcome.is_ok(),
        ours.outcome.is_ok(),
        "{scenario}: both open or both refuse"
    );
    match (ours.outcome, error) {
        (_, Some(error)) => Err(error),
        (outcome, None) => Ok(outcome
            .expect("an open without an error succeeded")
            .expect("rsl-limesdr reports its reference")),
    }
}

#[test]
fn a_fresh_session_matches_libusdr() {
    assert_eq!(
        assert_same_session(SimBoard::new, "fresh board").ok(),
        Some(40_000_000)
    );
}

#[test]
fn a_detector_taking_its_gate_time_is_polled_and_waited_for_like_libusdr() {
    let board = || SimBoard::new().with_detector_delay_us(DETECTOR_GATE_US);
    let reference = libusdr(board());
    assert!(
        reference
            .open
            .iter()
            .any(|op| matches!(op, Op::Sleep { us: 50_000 })),
        "libusdr waits between polls"
    );
    assert!(assert_same_session(board, "timed detector").is_ok());
}

#[test]
fn an_lms7002m_other_than_mask_1_is_refused_like_libusdr() {
    let board = || SimBoard::new().with_lms7_mask(0);
    assert_eq!(libusdr(board()).outcome, Err(-EIO));
    let outcome = assert_same_session(board, "LMS7002M mask 0");
    assert!(
        matches!(outcome, Err(Error::ChipId { found: 0x3840, .. })),
        "{outcome:?}"
    );
}

#[test]
fn references_libusdr_decides_reliably_are_decided_alike() {
    // Measured from 30.72 to 31.72 MHz libusdr opens as 30.72 MHz, from 40 to 41 MHz as
    // 40 MHz, and refuses from 31.72 to 38.4 MHz and from 41 to 52 MHz.
    for (hz, nominal) in [
        (31_000_000, Some(30_720_000)),
        (35_000_000, None),
        (40_000_000, Some(40_000_000)),
        (40_900_000, Some(40_000_000)),
        (41_100_000, None),
    ] {
        let board = || SimBoard::new().with_reference_hz(hz);
        let outcome = assert_same_session(board, &format!("{hz} Hz reference"));
        assert_eq!(
            outcome.as_ref().ok(),
            nominal.as_ref(),
            "{hz} Hz: {outcome:?}"
        );
        if nominal.is_none() {
            assert_eq!(libusdr(board()).outcome, Err(-EFAULT));
            assert!(matches!(outcome, Err(Error::UnknownReference { .. })));
        }
    }
}

#[test]
fn references_libusdrs_walk_misses_are_recognised_and_opened_as_from_40_mhz() {
    let from_40_mhz = libusdr(SimBoard::new()).open;
    let count_read = from_40_mhz
        .iter()
        .position(|op| matches!(op, Op::FpgaRead { addr: 0x0073, .. }))
        .expect("libusdr reads the count");
    // The count read's reply follows its two decoded register reads.
    let after_count = count_read + 2;

    for hz in [10_000_000, 38_400_000, 52_000_000] {
        let reference = libusdr(SimBoard::new().with_reference_hz(hz));
        assert_eq!(reference.outcome, Err(-EFAULT), "{hz} Hz: libusdr refuses");
        let (ours, error) = rsl_limesdr(SimBoard::new().with_reference_hz(hz));
        assert!(error.is_none(), "{hz} Hz: rsl-limesdr opens: {error:?}");
        assert_eq!(ours.outcome, Ok(Some(hz)), "{hz} Hz: recognised as itself");

        let (measured, rest) = ours.open.split_at(reference.open.len());
        assert_same_ops(&reference.open, measured, &format!("{hz} Hz: measurement"));
        assert_same_ops(
            &from_40_mhz[after_count..],
            rest,
            &format!("{hz} Hz: after the measurement"),
        );
    }
}

#[test]
fn a_stalled_detector_is_refused_where_libusdr_opens_on_a_stale_count() {
    let board = || {
        STALE_40_MHZ
            .into_iter()
            .fold(SimBoard::new(), |board, (addr, value)| {
                board.with_fpga_reg(addr, value)
            })
            .with_detector_delay_us(u64::MAX)
    };
    let reference = libusdr(board());
    assert_eq!(
        reference.outcome,
        Ok(None),
        "libusdr takes the stale count for a measurement"
    );
    let (ours, error) = rsl_limesdr(board());
    assert!(matches!(error, Some(Error::DetectorStalled)), "{error:?}");

    let polls = |ops: &[Op]| {
        ops.iter()
            .filter(|op| matches!(op, Op::FpgaRead { addr: 0x0065, .. }))
            .count()
    };
    assert_eq!(polls(&ours.open), 10, "libusdr's poll budget");
    let (shared, libusdr_only) = reference.open.split_at(ours.open.len());
    assert_same_ops(shared, &ours.open, "stalled detector, up to the count read");
    assert!(
        matches!(libusdr_only.get(1), Some(Op::FpgaRead { addr: 0x0072, .. })),
        "libusdr's next exchange reads the count: {:?}",
        libusdr_only.first()
    );
}

#[test]
fn the_revision_is_libusdrs_emulated_constant() {
    let mut oracle = Oracle::open(SimBoard::new()).expect("libusdr opens the LimeSDR-mini");
    let revision = oracle.get_uint(c"/dm/revision").expect("readable");
    drop(oracle.close());
    let device = Device::open(SimPort::new(SimBoard::new())).expect("rsl-limesdr opens it");
    assert_eq!(device.revision(), revision);
}
