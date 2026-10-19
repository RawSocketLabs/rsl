//! xsdr's front-end probe at open against libusdr's, operation for operation, with each
//! board libusdr brings up on the XSDR's M.2 connector attached (`sdr-frontend-sim`'s
//! models, as on the uSDR's sim). Each scenario compares the whole open and close.
//!
//! No divergence is whitelisted. Clock boards whose LMK05318 never locks spend a lock wait
//! that libusdr times on the host's wall clock and xsdr in counted sleeps; their scenario
//! compares the traces with each run of identical polls collapsed to its first.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use sim_bus::SimBus;
use xsdr::{Device, FrontEndKind};
use xsdr_oracle::Oracle;
use xsdr_sim::{FrontEnd, Op, SimBoard, Variant};

/// An XSDR with `front_end` on its connector.
fn with(front_end: FrontEnd) -> SimBoard {
    SimBoard::new(Variant::Xsdr).with_front_end(front_end)
}

/// libusdr's open and close of `board`, and xsdr's, with what xsdr's probe brought up.
fn sessions(board: impl Fn() -> SimBoard) -> (Vec<Op>, Vec<Op>, Option<FrontEndKind>) {
    let libusdr = Oracle::open(board())
        .expect("libusdr opens the board")
        .close();
    let sim = SimBus::new(board());
    let device = Device::open(sim.clone()).expect("xsdr opens the board");
    let kind = device.front_end();
    device.close().expect("xsdr closes the board");
    let xsdr = sim.board().trace().to_vec();
    (libusdr.trace().to_vec(), xsdr, kind)
}

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: xsdr diverges from libusdr at op {at}: libusdr {}, xsdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// Each board comes up as libusdr brings it up, after the boards before it in libusdr's
/// order are probed and found absent: the `PCIe` front end (expanders, RF path, DAC);
/// XMASS (expander check, LMK05318 bring-up, lock waits, `SYNC_N` pulse; on revision 1a
/// the sensors and serial number); `SuperSync` (the LMK5C33216's ID and register load);
/// `SimpleSync` (enables, LMK05318 bring-up, LO outputs off, lock waits, sync). A lone
/// breakout's DAC passes for the `PCIe` front end's, whose expanders the idle bus mimics,
/// so libusdr's probe, and xsdr's, take the breakout for a `PCIe` front end. Coverage gap:
/// the breakout's own bring-up with its DAC is never reached automatically, and the XSDR
/// takes no hint; the uSDR's oracle compares it through `fe=exm2pe`.
#[test]
fn each_board_comes_up_like_libusdr() {
    for (board, kind) in [
        (FrontEnd::Pciefe, FrontEndKind::Pciefe),
        (FrontEnd::Xmass, FrontEndKind::Xmass),
        (FrontEnd::XmassRev1a, FrontEndKind::Xmass),
        (FrontEnd::SuperSync, FrontEndKind::SuperSync),
        (FrontEnd::SimpleSync, FrontEndKind::SimpleSync),
        (FrontEnd::Exm2pe, FrontEndKind::Pciefe),
    ] {
        let (libusdr, xsdr, found) = sessions(|| with(board));
        assert_same_ops(&libusdr, &xsdr, &format!("{board:?}"));
        assert_eq!(found, Some(kind), "{board:?}");
    }
}

/// With XMASS's LMK05318 silent, libusdr logs the failed identity check and carries on:
/// the idle bus reads as a locked APLL1, APLL2 is unused, so the board comes up. So does
/// xsdr's.
#[test]
fn xmass_without_its_clock_carries_on_like_libusdr() {
    let (libusdr, xsdr, found) = sessions(|| with(FrontEnd::Xmass).without_front_end_clock());
    assert_same_ops(&libusdr, &xsdr, "XMASS without its LMK05318");
    assert_eq!(found, Some(FrontEndKind::Xmass));
}

/// One poll of an LMK05318 lock wait: its VCO status, the three lock-status reads, the
/// 100 µs sleep.
const POLL_LEN: usize = 5;

/// `trace` with each run of identical lock polls collapsed to its first: libusdr's lock
/// waits time out on the host's wall clock, so how many polls it makes depends on the
/// host; xsdr counts its sleeps instead. Everything else must agree.
fn collapse_polls(trace: &[Op]) -> Vec<Op> {
    let mut kept: Vec<Op> = Vec::with_capacity(trace.len());
    let mut index = 0;
    while index < trace.len() {
        let poll = trace.get(index..index + POLL_LEN);
        let repeats = poll.is_some_and(|poll| {
            poll[POLL_LEN - 1] == Op::Sleep { us: 100 }
                && kept.len() >= POLL_LEN
                && kept[kept.len() - POLL_LEN..] == *poll
        });
        if repeats {
            index += POLL_LEN;
        } else {
            kept.push(trace[index].clone());
            index += 1;
        }
    }
    kept
}

/// Clock boards whose LMK05318 never locks fail their lock wait, and the automatic probe
/// skips them as libusdr does: past XMASS (and `SimpleSync`, whose probe meets the same
/// chip), or past `SimpleSync`, to the breakout without a DAC.
#[test]
fn clock_boards_that_never_lock_are_skipped_like_libusdr() {
    for board in [FrontEnd::Xmass, FrontEnd::SimpleSync] {
        let (libusdr, xsdr, found) = sessions(|| with(board).with_unlocked_front_end_clock());
        assert_same_ops(
            &collapse_polls(&libusdr),
            &collapse_polls(&xsdr),
            &format!("{board:?}, never locking"),
        );
        assert_eq!(
            found,
            Some(FrontEndKind::Exm2pe { dac: false }),
            "{board:?}"
        );
    }
}
