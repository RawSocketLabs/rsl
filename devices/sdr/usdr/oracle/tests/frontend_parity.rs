//! rsl-usdr's front-end probe and controls against libusdr's (`fe=`), operation for
//! operation over whole power-ups.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;
#[path = "common/thermal_gate.rs"]
mod thermal_gate;

use std::ffi::CStr;

use rsl_usdr::frontend::{Exm2peOptions, PciefeOptions, PciefeRevision};
use rsl_usdr::{Device, Error, FrontEndKind, FrontEndSelection};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, SimBoard};
use sim_bus::SimBus;
use thermal_gate::{assert_same_ops, without_thermal_gate};

/// The revision every scenario runs on; the probe does not depend on it.
const REVISION: BoardRevision = BoardRevision::Rev3;

/// Opens fresh copies of `board` through libusdr with `parameters` and through rsl-usdr
/// with `selection`, closes whichever opened, and compares the whole traces. Returns
/// rsl-usdr's open result with the front end it reports.
fn assert_open_matches(
    board: impl Fn() -> SimBoard,
    parameters: &CStr,
    selection: FrontEndSelection,
) -> Result<Option<FrontEndKind>, Error> {
    let reference = match Oracle::open_with(board(), parameters) {
        Ok(oracle) => oracle.close(),
        Err(error) => *error.board,
    };

    let sim = SimBus::new(board());
    let opened = Device::builder(sim.clone()).front_end(selection).open();
    let result = opened.map(|device| {
        let kind = device.front_end();
        device.close().expect("rsl-usdr powers down");
        kind
    });

    let actual = without_thermal_gate(sim.board().trace(), REVISION);
    assert_same_ops(reference.trace(), &actual, &format!("{parameters:?}"));
    result
}

/// A board with nothing on its connector.
fn bare() -> SimBoard {
    SimBoard::new(REVISION)
}

/// libusdr's default probe finds nothing that identifies itself and settles on the
/// breakout, which has no ID; so does rsl-usdr.
#[test]
fn the_automatic_probe_settles_on_a_breakout_without_a_dac() {
    let kind = assert_open_matches(bare, c"", FrontEndSelection::Auto).expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Exm2pe { dac: false }));
}

/// `fe=` naming no board probes nothing at all.
#[test]
fn naming_no_board_probes_nothing() {
    let kind = assert_open_matches(bare, c"fe=none", FrontEndSelection::None).expect("both open");
    assert_eq!(kind, None);
}

/// The 100–5000 MHz front end is admitted on `LSDr` only: nothing is probed, nothing found.
#[test]
fn the_lsdr_front_end_is_not_admitted() {
    let kind =
        assert_open_matches(bare, c"fe=fe1005000", FrontEndSelection::Fe1005000).expect("open");
    assert_eq!(kind, None);
}

/// The clock boards each probe their chip and, finding none, leave the uSDR without a
/// front end.
#[test]
fn absent_clock_boards_are_not_found() {
    for (parameters, selection) in [
        (c"fe=xmass", FrontEndSelection::Xmass),
        (c"fe=supersync", FrontEndSelection::SuperSync),
        (c"fe=simplesync", FrontEndSelection::SimpleSync),
    ] {
        let kind = assert_open_matches(bare, parameters, selection).expect("both open");
        assert_eq!(kind, None, "{parameters:?}");
    }
}

/// A hinted `PCIe` front end that is not there fails the open where libusdr's does: its
/// expanders read the idle bus, so the bring-up reaches the DAC, which does not identify.
#[test]
fn a_missing_pciefe_fails_the_open_at_its_dac() {
    let result = assert_open_matches(
        bare,
        c"fe=pciefe:v1",
        FrontEndSelection::Pciefe(PciefeOptions::default()),
    );
    assert!(
        matches!(
            result,
            Err(Error::FrontEnd {
                board: "pciefe",
                ..
            })
        ),
        "{result:?}"
    );
}

/// On the V0 board an unrecognised DAC also fails the open.
#[test]
fn a_missing_v0_pciefe_fails_the_open_at_its_dac() {
    let mut options = PciefeOptions::default();
    options.revision = PciefeRevision::V0;
    let result = assert_open_matches(bare, c"fe=pciefev0", FrontEndSelection::Pciefe(options));
    assert!(
        matches!(
            result,
            Err(Error::FrontEnd {
                board: "pciefe",
                ..
            })
        ),
        "{result:?}"
    );
}

/// The breakout's options apply even without its DAC.
#[test]
fn a_hinted_breakout_without_a_dac_takes_its_options() {
    let mut options = Exm2peOptions::default();
    options.oscillator = Some(true);
    options.gps = Some(false);
    options.dac = Some(0x8000);
    let kind = assert_open_matches(
        bare,
        c"fe=exm2pe:osc_on:gps_off:dac_32768",
        FrontEndSelection::Exm2pe(options),
    )
    .expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Exm2pe { dac: false }));
}

/// The four-channel front end needs a second FPGA I2C core: the open fails at its first
/// transfer, after its pins are configured.
#[test]
fn the_four_channel_front_end_fails_for_want_of_an_i2c_core() {
    let result = assert_open_matches(bare, c"fe=fe4ch4007200", FrontEndSelection::Fe4ch4007200);
    assert!(
        matches!(
            result,
            Err(Error::FrontEnd {
                board: "fe4ch4007200",
                ..
            })
        ),
        "{result:?}"
    );
}
