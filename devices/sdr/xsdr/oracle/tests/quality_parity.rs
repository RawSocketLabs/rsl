//! A sample rate set while TX streams, against libusdr's, operation for operation: the
//! link calibration with the TX MMCM and TX phase, then the TX quality check
//! (`xsdr_check_rxtx_quality`), on fresh simulated XSDRs with a TX MMCM.
//!
//! libusdr's quality check ends dividing the rates by interpolation and decimation factors
//! only DUC/DDC gateware sets (`s_tx_int`, `s_rx_dec`, 0 here): its integer division traps
//! (SIGFPE). The oracle catches the trap and abandons libusdr's call there
//! (`Oracle::set_uint_trapping`); xsdr divides by the board's factors, 1, and sets the
//! running channels' baseband filters. That tail of LMS7002M writes is the whitelisted
//! divergence. Everything before the trapped call is compared whole, the trapped call up
//! to the trap; nothing after it is (libusdr's process dies there on hardware), but xsdr's
//! later calls and close must succeed. A session without a trap is compared whole.
//!
//! The sim's capture brings the generator's tone back as libusdr's fit expects
//! (`xsdr-sim`'s `RX_PHY_CAPTURE`), so the check passes at once, unless the scenario reads
//! the tone back reversed, which no attempt mends.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::time::Duration;

use sim_bus::SimBus;
use xsdr::{Device, TxChannels};
use xsdr_oracle::Oracle;
use xsdr_sim::{Op, SimBoard, Variant};

/// An XSDR with a TX MMCM.
const TX_MMCM: u32 = 0x0030_0020;
/// libusdr's oldest TX gateware (`MINIM_FWID_COMPAT`).
const TX_GATEWARE: u32 = 0xd2b1_0c09;
/// A send's wait; the sim answers at once.
const TIMEOUT: Duration = Duration::from_millis(100);
/// Samples per packet.
const SAMPLES: u32 = 256;

/// One step of a session.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `/dm/rate/master`.
    Rate(u32),
    /// Create the TX stream on channel A.
    CreateTx,
    /// Start TX, then sync free-running.
    StartTx,
    /// Send one packet now.
    Send,
    /// Stop and destroy TX.
    StopTx,
}

/// The board's operations each call made, then the close's; and which calls libusdr
/// abandoned at a division trap.
struct Session {
    /// Each call's operations, then the close's last.
    slices: Vec<Vec<Op>>,
    /// Each call's outcome: 0 or a negative errno.
    outcomes: Vec<i32>,
    /// Calls libusdr abandoned at its division by zero.
    trapped: Vec<bool>,
}

/// Splits `trace` at `marks` (each call's end), the close last.
fn slices(trace: &[Op], marks: &[usize]) -> Vec<Vec<Op>> {
    let mut start = 0;
    let mut slices = Vec::new();
    for &end in marks.iter().chain([&trace.len()]) {
        slices.push(trace[start..end].to_vec());
        start = end;
    }
    slices
}

/// The samples of one send.
fn samples() -> Vec<[i16; 2]> {
    (0..SAMPLES)
        .map(|n| {
            let n = i16::try_from(n).expect("small");
            [n, -n]
        })
        .collect()
}

/// libusdr's session.
fn libusdr(board: SimBoard, calls: &[Call]) -> Session {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let (mut marks, mut outcomes, mut trapped) = (Vec::new(), Vec::new(), Vec::new());
    let host: Vec<u8> = samples()
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    for &call in calls {
        let mut trap = false;
        let result = match call {
            Call::Rate(rate) => {
                // SAFETY: the master rate takes its value as a number.
                unsafe { oracle.set_uint_trapping(c"/dm/rate/master", rate.into()) }
                    .map(|caught| trap = caught)
            }
            Call::CreateTx => oracle.create_tx_stream(1, SAMPLES),
            Call::StartTx => oracle
                .start_tx_stream()
                .and_then(|()| oracle.sync_tx_stream(c"none")),
            Call::Send => oracle.send_tx(&[&host], SAMPLES, u64::MAX, 100).map(|_| ()),
            Call::StopTx => {
                let stopped = oracle.stop_tx_stream();
                oracle.destroy_tx_stream().and(stopped)
            }
        };
        marks.push(oracle.trace_len() - opened);
        outcomes.push(result.err().unwrap_or(0));
        trapped.push(trap);
    }
    let board = oracle.close();
    Session {
        slices: slices(&board.trace()[opened..], &marks),
        outcomes,
        trapped,
    }
}

/// xsdr's session.
fn rsl(board: SimBoard, calls: &[Call]) -> Session {
    let sim = SimBus::new(board);
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    let (mut marks, mut outcomes) = (Vec::new(), Vec::new());
    let samples = samples();
    for &call in calls {
        let result = match call {
            Call::Rate(rate) => device.set_sample_rate(rate, rate),
            Call::CreateTx => device.create_tx_stream(TxChannels::A, SAMPLES),
            Call::StartTx => device.start_tx_stream(),
            Call::Send => device.send(&[&samples], None, TIMEOUT).map(|_| ()),
            Call::StopTx => device.stop_tx_stream(),
        };
        marks.push(sim.board().trace().len() - opened);
        outcomes.push(match result {
            Ok(()) => 0,
            Err(error) => panic!("{call:?} failed in xsdr: {error:?}"),
        });
    }
    device.close().expect("xsdr closes the XSDR");
    let trace = sim.board().trace()[opened..].to_vec();
    Session {
        slices: slices(&trace, &marks),
        outcomes,
        trapped: vec![false; calls.len()],
    }
}

/// Runs `calls` on fresh boards from `board` through both and compares them up to
/// libusdr's division trap, if any: every call before it and the close (without a trap)
/// whole, the trapped call up to the trap, its rest in xsdr only LMS7002M writes. On
/// hardware libusdr's process dies at the trap, so nothing after it is compared; xsdr's
/// later calls must still succeed. Returns libusdr's session.
fn compare(board: impl Fn() -> SimBoard, calls: &[Call]) -> Session {
    let theirs = libusdr(board(), calls);
    let ours = rsl(board(), calls);
    let trap = theirs.trapped.iter().position(|&trapped| trapped);
    let compared = trap.map_or(calls.len(), |index| index + 1);
    assert_eq!(
        ours.outcomes[..compared],
        theirs.outcomes[..compared],
        "each call's outcome"
    );
    let whole = trap.unwrap_or(theirs.slices.len());
    for (index, (ours, theirs)) in ours.slices.iter().zip(&theirs.slices).enumerate() {
        if index > whole {
            break;
        }
        for (at, (our_op, their_op)) in ours.iter().zip(theirs).enumerate() {
            assert_eq!(our_op, their_op, "call {index}, operation {at}");
        }
        if index < whole {
            assert_eq!(ours.len(), theirs.len(), "call {index}: as many operations");
            continue;
        }
        let tail = ours
            .get(theirs.len()..)
            .expect("libusdr's operations, then xsdr's tail");
        assert!(
            !tail.is_empty(),
            "call {index}: the bandwidth writes libusdr owes"
        );
        assert!(
            tail.iter()
                .all(|op| matches!(op, Op::Spi { out, .. } if out >> 31 == 1)),
            "call {index}: past the trap, only LMS7002M writes"
        );
    }
    theirs
}

/// A fresh XSDR with a TX MMCM and gateware libusdr streams TX from.
fn xsdr() -> SimBoard {
    SimBoard::new(Variant::Xsdr)
        .with_hwid(TX_MMCM)
        .with_gateware_build(TX_GATEWARE)
}

/// How many times `slice` starts a capture (`xsdr_phy_capture_start(d, 1)`).
fn captures_started(slice: &[Op]) -> usize {
    let start = 1 << 31 | 5 << 24 | 1;
    slice
        .iter()
        .filter(|op| {
            **op == Op::RegWrite {
                addr: 56,
                value: start,
            }
        })
        .count()
}

/// A session streaming TX with a rate change in the middle.
const SESSION: [Call; 7] = [
    Call::Rate(10_000_000),
    Call::CreateTx,
    Call::StartTx,
    Call::Send,
    Call::Rate(20_000_000),
    Call::Send,
    Call::StopTx,
];

#[test]
fn a_rate_while_tx_streams_matches_libusdr_up_to_its_division_trap() {
    let session = compare(xsdr, &SESSION);
    assert_eq!(
        session.trapped,
        [false, false, false, false, true, false, false],
        "libusdr traps in the rate while TX streams, and only there"
    );
    assert_eq!(
        captures_started(&session.slices[4]),
        256,
        "one check of 256 captures: the tone passes at once"
    );
}

#[test]
fn a_tone_that_never_passes_takes_eight_attempts_like_libusdr() {
    let session = compare(|| xsdr().with_reversed_capture(), &SESSION);
    assert!(session.trapped[4], "libusdr traps in the rate");
    assert_eq!(
        captures_started(&session.slices[4]),
        8 * 256,
        "eight checks, the clock toggled and delayed between them"
    );
}

#[test]
fn a_rate_in_the_core_supply_fixup_band_matches_libusdr() {
    // 55 MS/s of TX: the core supply raised to 1.36 V before the calibration.
    let session = compare(
        xsdr,
        &[
            Call::Rate(10_000_000),
            Call::CreateTx,
            Call::Rate(55_000_000),
        ],
    );
    let fixup = Op::I2c {
        addr: xsdr_sim::I2cAddress { bus: 0, addr: 0x60 },
        write: vec![0x0e, 0x96],
        read: vec![],
    };
    assert!(
        session.slices[2].contains(&fixup),
        "the 1.36 V request, code 0x96"
    );
}
