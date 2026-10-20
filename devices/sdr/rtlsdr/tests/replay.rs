//! The replay harness: a driver session against a recorded trace. Only SYNTHETIC fixtures
//! exist until the owner approves captures (`CAPTURES.md`).

#[path = "common/replay_port.rs"]
mod replay_port;
#[path = "common/sim_port.rs"]
mod sim_port;
#[path = "common/synthetic.rs"]
mod synthetic;
#[path = "common/wire.rs"]
mod wire;

use replay_port::ReplayPort;
use rtlsdr::control::{BusError, ControlPort};
use rtlsdr::r820t2::field;
use rtlsdr_sim::{Op, SimDongle, Trace};
use sim_port::SimPort;
use synthetic::{STORE_REQUEST, TIMEOUT, TUNER_REQUEST, setup};

/// The SYNTHETIC session fixture.
fn fixture() -> Trace {
    Trace::parse(include_str!("fixtures/synthetic-session.trace")).expect("the fixture parses")
}

/// The SYNTHETIC stall fixture.
fn stall_fixture() -> Trace {
    Trace::parse(include_str!("fixtures/synthetic-stall.trace")).expect("the fixture parses")
}

#[test]
fn the_fixtures_are_labelled_synthetic() {
    for trace in [fixture(), stall_fixture()] {
        assert!(trace.is_synthetic(), "a hand-written trace must say so");
        assert!(trace.capture().starts_with("SYNTHETIC-"));
        assert_eq!(trace.sha256(), None, "no capture file stands behind it");
    }
}

#[test]
fn a_recorded_stall_fails_the_transfer_as_the_simulator_does() {
    let tuner = setup(TUNER_REQUEST, 0, 0);
    let mut sim = SimPort::new(SimDongle::new(synthetic::wiring()));
    let mut replay = ReplayPort::new(stall_fixture());
    for port in [&mut sim as &mut dyn ControlPort, &mut replay] {
        assert!(
            matches!(
                port.control_out(tuner, &[0x05, 0x00], TIMEOUT),
                Err(BusError::Io(_))
            ),
            "the gate is closed"
        );
    }
    replay.finish().expect("the stall was the whole trace");
    let recorded: Vec<_> = sim
        .dongle()
        .trace()
        .iter()
        .filter_map(|op| match op {
            Op::Control(transfer) => Some(transfer.clone()),
            _ => None,
        })
        .collect();
    let expected: Vec<_> = stall_fixture()
        .transfers()
        .iter()
        .map(|(_, t)| t.clone())
        .collect();
    assert_eq!(recorded, expected);
}

#[test]
fn the_simulator_records_the_session_the_fixture_holds() {
    let mut port = SimPort::new(SimDongle::new(synthetic::wiring()));
    synthetic::session(&mut port).expect("runs");
    let recorded: Vec<_> = port
        .dongle()
        .trace()
        .iter()
        .filter_map(|op| match op {
            Op::Control(transfer) => Some(transfer.clone()),
            _ => None,
        })
        .collect();
    let expected: Vec<_> = fixture()
        .transfers()
        .iter()
        .map(|(_, t)| t.clone())
        .collect();
    assert_eq!(recorded, expected);
}

#[test]
fn a_matching_session_replays_cleanly_and_reads_the_recorded_data() {
    let mut port = ReplayPort::new(fixture());
    let registers = synthetic::session(&mut port).expect("follows the trace");
    port.finish().expect("consumed the whole trace");
    assert!(registers.has_chip_id());
    assert_eq!(registers.get(field::LNA_GAIN), 0xA);
}

#[test]
fn a_divergent_transfer_is_reported_at_its_trace_line() {
    let mut port = ReplayPort::new(fixture());
    port.control_out(setup(STORE_REQUEST, 0x00E3, 0x00E4), &[0x09], TIMEOUT)
        .expect_err("the trace's first write sends 08");
    let divergence = port.finish().expect_err("diverged");
    assert_eq!(
        divergence.line,
        Some(11),
        "the fixture's first transfer line"
    );
    assert!(divergence.to_string().contains("out 40 e1 00e3 00e4 09"));
}

#[test]
fn a_session_that_stops_early_is_caught_at_finish() {
    let mut port = ReplayPort::new(fixture());
    port.control_out(setup(STORE_REQUEST, 0x00E3, 0x00E4), &[0x08], TIMEOUT)
        .expect("matches line 11");
    let divergence = port.finish().expect_err("four transfers unplayed");
    assert_eq!(divergence.line, Some(13));
    assert_eq!(divergence.actual, None);
}
