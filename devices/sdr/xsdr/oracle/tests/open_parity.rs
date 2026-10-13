//! rsl-xsdr's open and RFIC power-up against libusdr's, operation for operation, on fresh
//! simulated boards.
//!
//! Whitelisted divergences, each confined to the scenario that shows it:
//! - libusdr ends an XSDR open with its front-end probe on I2C bus 1 (`device_fe_probe`),
//!   which rsl-xsdr does not port yet. The cut starts at libusdr's first write to the
//!   front-end GPIO core (FPGA register 3) and must hold nothing but probe traffic.
//! - Variants rsl-xsdr identifies but does not bring up are refused after the HWID reads
//!   libusdr makes too; libusdr carries on.
//! - A PMIC revision libusdr rejects: libusdr fails the power-up after holding the LMS7002M
//!   in reset and reading the temperature sensor's ID for its log; rsl-xsdr refuses before
//!   any bus access.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use rsl_xsdr::{Device, Error};
use rsl_xsdr_oracle::Oracle;
use rsl_xsdr_sim::{Op, SimBoard, Variant};
use sim_bus::SimBus;

/// `EINVAL`: `xsdr_init`'s refusal of an unknown device ID.
const EINVAL: i32 = 22;
/// `EIO`: `_xsdr_pwren_revx`'s refusal of an unaccepted PMIC.
const EIO: i32 = 5;
/// The front-end GPIO core's register, the first thing `device_fe_probe` writes.
const REG_FRONT_END_GPIO: u32 = 3;
/// The front-end connector's I2C bus.
const FRONT_END_BUS: u8 = 1;

/// libusdr's open and power-up of `board`: the open's operations before its front-end
/// probe (checked to hold only probe traffic) and the power-up's (or its errno).
struct Reference {
    /// The open up to the probe.
    open: Vec<Op>,
    /// The power-up's operations and outcome.
    power_up: (Vec<Op>, Result<(), i32>),
}

/// Runs libusdr on `board`.
fn reference(board: SimBoard) -> Reference {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let outcome = oracle.power_up();
    let powered = oracle.trace_len();
    let board = oracle.close();
    let trace = board.trace();

    let (open, probe) = trace[..opened].split_at(
        trace[..opened]
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
            .expect("libusdr probes the XSDR's front-end connector"),
    );
    assert!(
        probe.iter().all(|op| match op {
            Op::RegWrite { addr, .. } => *addr == REG_FRONT_END_GPIO,
            Op::I2c { addr, .. } => addr.bus == FRONT_END_BUS,
            Op::Sleep { .. } => true,
            Op::RegRead { .. } | Op::Spi { .. } => false,
        }),
        "the cut holds only front-end probe traffic: {probe:?}"
    );
    Reference {
        open: open.to_vec(),
        power_up: (trace[opened..powered].to_vec(), outcome),
    }
}

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: rsl-xsdr diverges from libusdr at op {at}: libusdr {}, rsl-xsdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

#[test]
fn xsdr_open_and_power_up_match_libusdr_up_to_its_front_end_probe() {
    let libusdr = reference(SimBoard::new(Variant::Xsdr));
    assert_eq!(libusdr.power_up.1, Ok(()), "libusdr powers the RFIC up");

    let sim = SimBus::new(SimBoard::new(Variant::Xsdr));
    let mut device = Device::open(sim.clone()).expect("rsl-xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    device.power_up().expect("rsl-xsdr powers the RFIC up");

    let board = sim.board();
    let (open, power_up) = board.trace().split_at(opened);
    assert_same_ops(&libusdr.open, open, "open");
    assert_same_ops(&libusdr.power_up.0, power_up, "power-up");
}

#[test]
fn an_unknown_device_id_is_refused_like_libusdr() {
    let board = || SimBoard::new(Variant::Xsdr).with_hwid(0x0032_0000);
    let libusdr = Oracle::open(board()).err().expect("libusdr refuses it");
    assert_eq!(libusdr.errno, -EINVAL);

    let sim = SimBus::new(board());
    assert!(matches!(
        Device::open(sim.clone()),
        Err(Error::UnknownDevice(0x32))
    ));
    assert_same_ops(libusdr.board.trace(), sim.board().trace(), "unknown device");
}

#[test]
fn unported_variants_stop_after_the_identification_libusdr_shares() {
    for variant in [Variant::Xtrx, Variant::Ssdr, Variant::SsdrPro] {
        let reference = match Oracle::open(SimBoard::new(variant)) {
            Ok(oracle) => oracle.close(),
            // sSDR: refused later, for want of the LMS8001 the sim lacks.
            Err(refused) => *refused.board,
        };
        let sim = SimBus::new(SimBoard::new(variant));
        assert!(matches!(
            Device::open(sim.clone()),
            Err(Error::UnsupportedVariant(_))
        ));

        let board = sim.board();
        let identification = board.trace();
        assert_eq!(
            identification.len(),
            2,
            "{variant:?}: the factory's and xsdr_init's HWID reads"
        );
        assert_same_ops(
            &reference.trace()[..identification.len()],
            identification,
            &format!("{variant:?}"),
        );
    }
}

#[test]
fn an_unaccepted_pmic_is_refused_at_power_up_before_any_bus_access() {
    let board = || SimBoard::new(Variant::Xsdr).with_fpga_pmic_revision(0xe101);
    let libusdr = reference(board());
    let (ops, outcome) = &libusdr.power_up;
    assert_eq!(
        *outcome,
        Err(-EIO),
        "libusdr opens, then refuses the power-up"
    );
    assert!(
        ops.iter().all(|op| match op {
            Op::RegWrite { addr: 0, value } => *value == 0,
            Op::Spi { .. } => false,
            _ => true,
        }),
        "libusdr's refusal powers nothing: {ops:?}"
    );

    let sim = SimBus::new(board());
    let mut device = Device::open(sim.clone()).expect("rsl-xsdr opens it, as libusdr");
    assert_same_ops(
        &libusdr.open,
        sim.board().trace(),
        "open with an unaccepted PMIC",
    );
    let opened = sim.board().trace().len();
    assert!(matches!(
        device.power_up(),
        Err(Error::ChipId { chip: "LP8758", .. })
    ));
    assert_eq!(
        sim.board().trace().len(),
        opened,
        "rsl-xsdr's refusal drives nothing"
    );
}
