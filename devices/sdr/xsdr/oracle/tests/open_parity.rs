//! xsdr's open, RFIC power-up and close against libusdr's, operation for operation, on
//! fresh simulated boards.
//!
//! Each open includes its front-end probe on I2C bus 1 (`device_fe_probe`, through
//! `sdr-frontend`), compared whole.
//!
//! The XTRX has no front-end connector; its open ends at its reference-trim DAC.
//!
//! Whitelisted divergences, each confined to the scenario that shows it:
//! - Variants xsdr identifies but does not bring up (sSDR, sSDR Pro) are refused after the
//!   HWID reads libusdr makes too; libusdr carries on (and fails for want of the LMS8001
//!   the sim lacks).
//! - A PMIC revision libusdr rejects: libusdr fails the power-up after holding the LMS7002M
//!   in reset and reading the temperature sensor's ID for its log; xsdr refuses before
//!   any bus access.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use sim_bus::SimBus;
use xsdr::{Device, Error, FrontEndKind};
use xsdr_oracle::Oracle;
use xsdr_sim::{Op, SimBoard, Variant};

/// `EINVAL`: `xsdr_init`'s refusal of an unknown device ID.
const EINVAL: i32 = 22;
/// `EIO`: `_xsdr_pwren_revx`'s refusal of an unaccepted PMIC.
const EIO: i32 = 5;
/// The front-end GPIO core's register, the first thing `device_fe_probe` writes.
const REG_FRONT_END_GPIO: u32 = 3;

/// libusdr's session on `board`: the open's operations (front-end probe included), the
/// power-up's (or its errno) and the close's.
struct Reference {
    /// The open.
    open: Vec<Op>,
    /// The power-up's operations and outcome; empty and `Ok` when not powered up.
    power_up: (Vec<Op>, Result<(), i32>),
    /// The close's operations.
    close: Vec<Op>,
}

/// Runs libusdr on an XSDR `board`: open, `/dm/power/en` = 1 if `power_up`, close.
fn reference(board: SimBoard, power_up: bool) -> Reference {
    let (reference, probed) = session(board, power_up);
    assert!(probed, "libusdr probes the XSDR's front-end connector");
    reference
}

/// Runs libusdr on `board` as [`reference`], and whether its open probed the front-end
/// connector.
fn session(board: SimBoard, power_up: bool) -> (Reference, bool) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let outcome = if power_up { oracle.power_up() } else { Ok(()) };
    let powered = oracle.trace_len();
    let board = oracle.close();
    let trace = board.trace();
    let open = &trace[..opened];
    let probed = open.iter().any(|op| {
        matches!(
            op,
            Op::RegWrite {
                addr: REG_FRONT_END_GPIO,
                ..
            }
        )
    });
    let reference = Reference {
        open: open.to_vec(),
        power_up: (trace[opened..powered].to_vec(), outcome),
        close: trace[powered..].to_vec(),
    };
    (reference, probed)
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

#[test]
fn an_xsdr_session_matches_libusdr_front_end_probe_included() {
    let libusdr = reference(SimBoard::new(Variant::Xsdr), true);
    assert_eq!(libusdr.power_up.1, Ok(()), "libusdr powers the RFIC up");

    let sim = SimBus::new(SimBoard::new(Variant::Xsdr));
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    assert_eq!(
        device.front_end(),
        Some(FrontEndKind::Exm2pe { dac: false }),
        "libusdr's probe ends at the breakout without a DAC (`Detected external FE: exm2pe`)"
    );
    let opened = sim.board().trace().len();
    device.power_up().expect("xsdr powers the RFIC up");
    let powered = sim.board().trace().len();
    device.close().expect("xsdr closes the XSDR");

    let board = sim.board();
    let trace = board.trace();
    assert_same_ops(&libusdr.open, &trace[..opened], "open");
    assert_same_ops(&libusdr.power_up.0, &trace[opened..powered], "power-up");
    assert_same_ops(&libusdr.close, &trace[powered..], "close");
}

#[test]
fn closing_an_xsdr_never_powered_up_matches_libusdrs_zeroed_chip_state() {
    let libusdr = reference(SimBoard::new(Variant::Xsdr), false);
    assert!(
        matches!(
            libusdr.close.first(),
            Some(Op::Spi {
                out: 0x8020_0003,
                ..
            })
        ),
        "libusdr selects both banks first, from its zeroed cache: {:?}",
        libusdr.close.first()
    );

    let sim = SimBus::new(SimBoard::new(Variant::Xsdr));
    let device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    device.close().expect("xsdr closes the XSDR");
    assert_same_ops(&libusdr.close, &sim.board().trace()[opened..], "close");
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
fn an_xtrx_session_matches_libusdr_with_either_trim_dac() {
    let revision_5 = || SimBoard::new(Variant::Xtrx);
    let revision_4 = || SimBoard::new(Variant::Xtrx).without_trim_dac();
    for (board, dac, revision) in [
        (revision_5 as fn() -> SimBoard, Some(0xa8f5), "revision 5"),
        (revision_4, None, "revision 4"),
    ] {
        let (libusdr, probed) = session(board(), true);
        assert!(!probed, "{revision}: the XTRX has no connector to probe");
        assert_eq!(
            libusdr.power_up.1,
            Ok(()),
            "{revision}: libusdr powers the RFIC up"
        );

        let sim = SimBus::new(board());
        let mut device = Device::open(sim.clone()).expect("xsdr opens the XTRX");
        assert_eq!(device.front_end(), None);
        assert_eq!(sim.board().trim_dac_code(), dac, "{revision}: mid-range");
        let opened = sim.board().trace().len();
        device.power_up().expect("xsdr powers the RFIC up");
        let powered = sim.board().trace().len();
        device.close().expect("xsdr closes the XTRX");

        let board = sim.board();
        let trace = board.trace();
        assert_same_ops(&libusdr.open, &trace[..opened], &format!("{revision} open"));
        assert_same_ops(
            &libusdr.power_up.0,
            &trace[opened..powered],
            &format!("{revision} power-up"),
        );
        assert_same_ops(
            &libusdr.close,
            &trace[powered..],
            &format!("{revision} close"),
        );
    }
}

#[test]
fn an_xtrx_with_an_unaccepted_pmic_fails_its_open_like_libusdr() {
    let board = || SimBoard::new(Variant::Xtrx).with_fpga_pmic_revision(0xe101);
    let libusdr = Oracle::open(board()).err().expect("libusdr refuses it");
    assert_eq!(libusdr.errno, -EIO);

    let sim = SimBus::new(board());
    assert!(matches!(
        Device::open(sim.clone()),
        Err(Error::ChipId {
            chip: "LP8758",
            found: 0xe101,
            ..
        })
    ));
    assert_same_ops(libusdr.board.trace(), sim.board().trace(), "refused PMIC");
}

#[test]
fn unported_variants_stop_after_the_identification_libusdr_shares() {
    for variant in [Variant::Ssdr, Variant::SsdrPro] {
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
    let libusdr = reference(board(), true);
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
    let mut device = Device::open(sim.clone()).expect("xsdr opens it, as libusdr");
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
        "xsdr's refusal drives nothing"
    );
    device.close().expect("xsdr closes it");
    assert_same_ops(
        &libusdr.close,
        &sim.board().trace()[opened..],
        "close after the refused power-up",
    );
}
