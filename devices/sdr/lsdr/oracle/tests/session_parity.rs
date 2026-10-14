//! lsdr's sessions against libusdr's, operation for operation, on fresh simulated
//! boards: open, ADC power, the receive PGA gain and close.
//!
//! Every open ends with its front-end probe (`device_fe_probe`, through
//! `sdr-frontend`), compared whole with the I2C core each transfer addresses.
//!
//! Whitelisted divergence, confined to the scenario that shows it:
//! - A PGA setting above 31: libusdr clamps it to 31; lsdr's `PgaGain` cannot hold it.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;

use lsdr::{Device, Error, Fitted, OpenOptions, PgaGain};
use lsdr_oracle::Oracle;
use lsdr_sim::{Op, SimBoard, Variant};
use sim_bus::SimBus;

/// `EIO`: open's "Device is dead!".
const EIO: i32 = 5;
/// The front-end GPIO core's register, the first thing `device_fe_probe` writes.
const REG_FRONT_END_GPIO: u32 = 3;

/// One step of a session after open.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// `/dm/power/en`.
    Power(bool),
    /// `/dm/sdr/0/rx/gain/pga`, as libusdr receives it.
    Pga(u8),
}

/// A session's operations, phase by phase: open (libusdr's up to its front-end probe),
/// each step, close.
#[derive(Debug, PartialEq, Eq)]
struct Session {
    /// The open's operations.
    open: Vec<Op>,
    /// Each step's operations.
    steps: Vec<Vec<Op>>,
    /// The close's operations.
    close: Vec<Op>,
}

/// Splits `trace` at the given phase ends.
fn phases(trace: &[Op], ends: &[usize]) -> Vec<Vec<Op>> {
    let mut start = 0;
    let mut phases = Vec::new();
    for &end in ends.iter().chain([&trace.len()]) {
        phases.push(trace[start..end].to_vec());
        start = end;
    }
    phases
}

/// libusdr's session on `variant`, opened with `connection`.
fn libusdr(variant: Variant, connection: &CStr, steps: &[Step]) -> Session {
    let mut oracle =
        Oracle::open(SimBoard::new(variant), connection).expect("libusdr opens the board");
    let mut ends = vec![oracle.trace_len()];
    for step in steps {
        match *step {
            Step::Power(on) => oracle.set_power(on),
            Step::Pga(value) => oracle.set_rx_pga_gain(value.into()),
        }
        .expect("libusdr runs the step");
        ends.push(oracle.trace_len());
    }
    let board = oracle.close();
    let mut phases = phases(board.trace(), &ends);
    let close = phases.pop().expect("a close phase");
    let open = phases.remove(0);
    assert!(
        open.iter().any(|op| matches!(
            op,
            Op::RegWrite {
                addr: REG_FRONT_END_GPIO,
                ..
            }
        )),
        "libusdr probes the LSDr's front-end connector"
    );
    Session {
        open,
        steps: phases,
        close,
    }
}

/// lsdr's session on `variant`, opened with `options`, and what it found.
fn lsdr(variant: Variant, options: OpenOptions, steps: &[Step]) -> (Session, Fitted) {
    let sim = SimBus::new(SimBoard::new(variant));
    let mut device = Device::open(sim.clone(), options).expect("lsdr opens the board");
    let fitted = device.fitted();
    let mut ends = vec![sim.board().trace().len()];
    for step in steps {
        match *step {
            Step::Power(on) => device.set_power(on),
            Step::Pga(value) => device.set_rx_pga_gain(
                PgaGain::new(value).expect("lsdr is given representable settings"),
            ),
        }
        .expect("lsdr runs the step");
        ends.push(sim.board().trace().len());
    }
    device.close().expect("lsdr closes the board");
    let board = sim.board();
    let mut phases = phases(board.trace(), &ends);
    let close = phases.pop().expect("a close phase");
    let open = phases.remove(0);
    (
        Session {
            open,
            steps: phases,
            close,
        },
        fitted,
    )
}

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: lsdr diverges from libusdr at op {at}: libusdr {}, lsdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// Compares the two sessions phase by phase.
fn assert_same_session(libusdr: &Session, lsdr: &Session, scenario: &str) {
    assert_same_ops(&libusdr.open, &lsdr.open, &format!("{scenario}, open"));
    assert_eq!(libusdr.steps.len(), lsdr.steps.len());
    for (at, (expected, actual)) in libusdr.steps.iter().zip(&lsdr.steps).enumerate() {
        assert_same_ops(expected, actual, &format!("{scenario}, step {at}"));
    }
    assert_same_ops(&libusdr.close, &lsdr.close, &format!("{scenario}, close"));
}

#[test]
fn the_golden_sessions_match_libusdr_front_end_probe_included() {
    // The characterization goldens' scenarios: open, /dm/power/en = 1, close.
    let steps = [Step::Power(true)];
    let single = libusdr(Variant::SingleAdc, c"", &steps);
    let (rust, fitted) = lsdr(Variant::SingleAdc, OpenOptions::new(), &steps);
    assert_same_session(&single, &rust, "single ADC");
    assert_eq!(fitted.adcs, [true, false]);

    let dual = libusdr(Variant::DualAdcClockGen, c"lmk=1", &steps);
    let options = OpenOptions::new().probe_clock_generator(true);
    let (rust, fitted) = lsdr(Variant::DualAdcClockGen, options, &steps);
    assert_same_session(&dual, &rust, "dual ADC, lmk=1");
    assert_eq!(
        fitted,
        Fitted {
            adcs: [true, true],
            clock_generator: true,
        }
    );
}

#[test]
fn the_clock_generator_test_follows_the_option_on_every_fit() {
    for (variant, connection, probe) in [
        (Variant::DualAdcClockGen, c"", false),
        (Variant::SingleAdc, c"lmk=1", true),
    ] {
        let scenario = format!("{variant:?}, {connection:?}");
        let options = OpenOptions::new().probe_clock_generator(probe);
        let (rust, fitted) = lsdr(variant, options, &[]);
        assert_same_session(&libusdr(variant, connection, &[]), &rust, &scenario);
        assert!(!fitted.clock_generator, "{scenario}: none found");
    }
}

#[test]
fn power_down_and_pga_gains_match_libusdr() {
    let steps = [
        Step::Power(true),
        Step::Pga(0),
        Step::Pga(10),
        Step::Pga(31),
        Step::Power(false),
    ];
    let (rust, _) = lsdr(Variant::SingleAdc, OpenOptions::new(), &steps);
    assert_same_session(
        &libusdr(Variant::SingleAdc, c"", &steps),
        &rust,
        "power and gain",
    );
}

#[test]
fn a_pga_setting_past_31_is_clamped_by_libusdr_and_unrepresentable_in_rsl_lsdr() {
    assert_eq!(PgaGain::new(40), None);
    let clamped = libusdr(Variant::SingleAdc, c"", &[Step::Pga(40)]);
    let (rust, _) = lsdr(Variant::SingleAdc, OpenOptions::new(), &[Step::Pga(31)]);
    assert_same_session(&clamped, &rust, "libusdr's 40 against lsdr's 31");
}

#[test]
fn a_dead_link_is_refused_like_libusdr() {
    let libusdr = Oracle::open(SimBoard::new(Variant::SingleAdc).dead(), c"")
        .err()
        .expect("libusdr refuses it");
    assert_eq!(libusdr.errno, -EIO);

    let sim = SimBus::new(SimBoard::new(Variant::SingleAdc).dead());
    assert!(matches!(
        Device::open(sim.clone(), OpenOptions::new()),
        Err(Error::DeadLink)
    ));
    assert_same_ops(libusdr.board.trace(), sim.board().trace(), "dead link");
}
