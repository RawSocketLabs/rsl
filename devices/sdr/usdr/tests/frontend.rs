//! Front-end behaviour the oracle cannot compare: where libusdr's own behaviour is
//! undefined.

mod common {
    pub(crate) mod sim_bus;
}

use common::sim_bus::SimBus;
use rsl_usdr::{Device, Error, FrontEndSelection};
use rsl_usdr_sim::{BoardRevision, SimBoard};

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
