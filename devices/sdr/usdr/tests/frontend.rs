//! Front-end behaviour the oracle cannot compare: where libusdr's own behaviour is
//! undefined, or depends on the host's wall clock.

mod common {
    pub(crate) mod sim_bus;
}

use std::mem;

use common::sim_bus::SimBus;
use usdr::{Device, Error, FrontEndKind, FrontEndSelection};
use usdr_sim::{BoardRevision, FrontEnd, Op, SimBoard};

/// With no front end, the oscillator DAC is refused before any bus access. libusdr reads an
/// uninitialised pointer here (its probe returns early for an `fe=` that finds nothing).
#[test]
fn without_a_front_end_the_oscillator_dac_is_refused_untouched() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::builder(sim.clone())
        .front_end(FrontEndSelection::None)
        .open()
        .expect("powers up");
    assert_eq!(device.front_end(), None);
    let before = sim.board().trace().len();
    assert!(matches!(
        device.set_oscillator_dac(0x8000),
        Err(Error::NoOscillatorDac)
    ));
    assert_eq!(sim.board().trace().len(), before);
}

/// The 100 µs sleeps between lock polls in `trace`.
fn lock_polls(trace: &[Op]) -> usize {
    trace
        .iter()
        .filter(|op| matches!(op, Op::Sleep { us: 100 }))
        .count()
}

/// An XMASS whose LMK05318 never locks fails a hinted open once APLL1's 100 ms are spent in
/// 100 µs polls. libusdr spends them in wall-clock time, so its poll count depends on the
/// host; only usdr's is checked.
#[test]
fn an_xmass_clock_that_never_locks_fails_the_hint_after_its_timeout() {
    let board = SimBoard::new(BoardRevision::Rev3)
        .with_front_end(FrontEnd::Xmass)
        .with_unlocked_front_end_clock();
    let sim = SimBus::new(board);
    let result = Device::builder(sim.clone())
        .front_end(FrontEndSelection::Xmass)
        .open();
    assert!(
        matches!(result, Err(Error::ClockUnlocked("LMK05318 APLL1"))),
        "{result:?}"
    );
    assert_eq!(lock_polls(sim.board().trace()), 1000);
}

/// The automatic probe skips a clock board whose LMK05318 never locks, as libusdr skips a
/// failing board: past XMASS, then past `SimpleSync` (whose probe meets the same chip at
/// the same address), to the breakout.
#[test]
fn the_automatic_probe_skips_a_clock_board_that_never_locks() {
    let board = SimBoard::new(BoardRevision::Rev3)
        .with_front_end(FrontEnd::Xmass)
        .with_unlocked_front_end_clock();
    let device = Device::builder(SimBus::new(board))
        .open()
        .expect("powers up");
    assert_eq!(
        device.front_end(),
        Some(FrontEndKind::Exm2pe { dac: false })
    );
}

/// A `SimpleSync` LO retune whose APLL2 never locks fails after its 10 ms of polls.
#[test]
fn a_simplesync_retune_fails_when_apll2_never_locks() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3).with_front_end(FrontEnd::SimpleSync));
    let mut device = Device::builder(sim.clone())
        .front_end(FrontEndSelection::SimpleSync)
        .open()
        .expect("powers up");
    {
        let mut board = sim.board();
        let attached = mem::replace(&mut *board, SimBoard::new(BoardRevision::Rev3));
        *board = attached.with_unlocked_front_end_clock();
    }
    let start = sim.board().trace().len();
    let result = device.set_sync_calibration_frequency(444_000_000);
    assert!(
        matches!(result, Err(Error::ClockUnlocked("LMK05318 APLL2"))),
        "{result:?}"
    );
    assert_eq!(lock_polls(&sim.board().trace()[start..]), 100);
}
