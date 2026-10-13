//! rsl-lsdr's sessions against libusdr's, operation for operation, on fresh simulated
//! boards: open, ADC power, the receive PGA gain and close.
//!
//! Whitelisted divergences, each confined to the scenario that shows it:
//! - libusdr ends every open with its front-end probe (`device_fe_probe`), which rsl-lsdr
//!   does not port yet. The cut starts at libusdr's first write to the front-end GPIO core
//!   (FPGA register 3) and must hold nothing but probe traffic: GPIO-core and SPI-extension
//!   configuration writes, the front-end connector's SPI bus, I2C away from the board's
//!   PMIC and oscillator, and waits. Every later phase is compared whole.
//! - A PGA setting above 31: libusdr clamps it to 31; rsl-lsdr's `PgaGain` cannot hold it.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;

use rsl_lsdr::{Device, Error, Fitted, OpenOptions, PgaGain};
use rsl_lsdr_oracle::Oracle;
use rsl_lsdr_sim::{I2cAddress, Op, SimBoard, Variant};
use sim_bus::SimBus;

/// `EIO`: open's "Device is dead!".
const EIO: i32 = 5;
/// The front-end GPIO core's register, the first thing `device_fe_probe` writes.
const REG_FRONT_END_GPIO: u32 = 3;
/// The SPI-extension configuration register (`spiext_cfg`), written before each exchange
/// on the front-end connector.
const REG_SPI_EXT_CONFIG: u32 = 58;
/// The front-end connector's SPI bus (`SPI_EXTERNAL`).
const SPI_EXTERNAL: u32 = 4;
/// The board's own I2C devices, which the probe must not reach: TPS6594, Si549.
const BOARD_I2C: [I2cAddress; 2] = [
    I2cAddress { bus: 0, addr: 0x48 },
    I2cAddress { bus: 0, addr: 0x67 },
];

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

/// Removes libusdr's front-end probe from the end of its open, checking that the cut holds
/// nothing else.
fn without_front_end_probe(mut open: Vec<Op>) -> Vec<Op> {
    let cut = open
        .iter()
        .position(|op| {
            matches!(
                op,
                Op::RegWrite {
                    addr: REG_FRONT_END_GPIO,
                    ..
                }
            )
        })
        .expect("libusdr probes the LSDr's front-end connector");
    let probe = open.split_off(cut);
    assert!(
        probe.iter().all(|op| match op {
            Op::RegWrite { addr, .. } => [REG_FRONT_END_GPIO, REG_SPI_EXT_CONFIG].contains(addr),
            Op::Spi { bus, .. } => *bus == SPI_EXTERNAL,
            Op::I2c { addr, .. } => !BOARD_I2C.contains(addr),
            Op::Sleep { .. } => true,
            Op::RegRead { .. } => false,
        }),
        "the cut holds only front-end probe traffic: {probe:?}"
    );
    open
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
    let open = without_front_end_probe(phases.remove(0));
    Session {
        open,
        steps: phases,
        close,
    }
}

/// rsl-lsdr's session on `variant`, opened with `options`, and what it found.
fn rsl_lsdr(variant: Variant, options: OpenOptions, steps: &[Step]) -> (Session, Fitted) {
    let sim = SimBus::new(SimBoard::new(variant));
    let mut device = Device::open(sim.clone(), options).expect("rsl-lsdr opens the board");
    let fitted = device.fitted();
    let mut ends = vec![sim.board().trace().len()];
    for step in steps {
        match *step {
            Step::Power(on) => device.set_power(on),
            Step::Pga(value) => device.set_rx_pga_gain(
                PgaGain::new(value).expect("rsl-lsdr is given representable settings"),
            ),
        }
        .expect("rsl-lsdr runs the step");
        ends.push(sim.board().trace().len());
    }
    device.close().expect("rsl-lsdr closes the board");
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
            "{scenario}: rsl-lsdr diverges from libusdr at op {at}: libusdr {}, rsl-lsdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// Compares the two sessions phase by phase.
fn assert_same_session(libusdr: &Session, rsl_lsdr: &Session, scenario: &str) {
    assert_same_ops(&libusdr.open, &rsl_lsdr.open, &format!("{scenario}, open"));
    assert_eq!(libusdr.steps.len(), rsl_lsdr.steps.len());
    for (at, (expected, actual)) in libusdr.steps.iter().zip(&rsl_lsdr.steps).enumerate() {
        assert_same_ops(expected, actual, &format!("{scenario}, step {at}"));
    }
    assert_same_ops(
        &libusdr.close,
        &rsl_lsdr.close,
        &format!("{scenario}, close"),
    );
}

#[test]
fn the_golden_sessions_match_libusdr_except_its_front_end_probe() {
    // The characterization goldens' scenarios: open, /dm/power/en = 1, close.
    let steps = [Step::Power(true)];
    let single = libusdr(Variant::SingleAdc, c"", &steps);
    let (rust, fitted) = rsl_lsdr(Variant::SingleAdc, OpenOptions::new(), &steps);
    assert_same_session(&single, &rust, "single ADC");
    assert_eq!(fitted.adcs, [true, false]);

    let dual = libusdr(Variant::DualAdcClockGen, c"lmk=1", &steps);
    let options = OpenOptions::new().probe_clock_generator(true);
    let (rust, fitted) = rsl_lsdr(Variant::DualAdcClockGen, options, &steps);
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
        let (rust, fitted) = rsl_lsdr(variant, options, &[]);
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
    let (rust, _) = rsl_lsdr(Variant::SingleAdc, OpenOptions::new(), &steps);
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
    let (rust, _) = rsl_lsdr(Variant::SingleAdc, OpenOptions::new(), &[Step::Pga(31)]);
    assert_same_session(&clamped, &rust, "libusdr's 40 against rsl-lsdr's 31");
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
