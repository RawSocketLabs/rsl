//! Power-up and power-down on the simulated board.

mod common {
    pub(crate) mod sim_bus;
}

use common::sim_bus::SimBus;
use rsl_usdr::{Device, Error};
use rsl_usdr_sim::{BoardRevision, SimBoard};

/// GPO bank driving the LMS6002D reset line (active low).
const GPO_LMS_RST: u8 = 0;
/// GPO bank enabling the RF booster.
const GPO_BOOSTER: u8 = 7;

#[test]
fn powers_up_and_down_again() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let device = Device::with_bus(sim.clone()).expect("a healthy board powers up");
    assert_eq!(
        sim.board().gpo(GPO_LMS_RST),
        Some(1),
        "RF chip out of reset"
    );
    assert_eq!(sim.board().gpo(GPO_BOOSTER), Some(1));

    device.close().expect("power-down succeeds");
    assert_eq!(sim.board().gpo(GPO_LMS_RST), Some(0));
    assert_eq!(sim.board().gpo(GPO_BOOSTER), Some(0));
}

#[test]
fn dropping_the_device_powers_down() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    drop(Device::with_bus(sim.clone()).expect("a healthy board powers up"));
    assert_eq!(sim.board().gpo(GPO_LMS_RST), Some(0));
}

#[test]
fn rejects_an_unknown_board_revision() {
    // A board whose HWID reports an unknown revision is refused before any power-up.
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    sim.board().write_reg(19, 9 << 8);
    let err = Device::with_bus(sim.clone()).expect_err("revision 9 is refused");
    assert!(matches!(err, Error::UnsupportedRevision(9)), "{err:?}");
}
