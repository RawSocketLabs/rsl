//! Handing a powered device over to a native USB handle, on the simulated board.
#![cfg(all(feature = "mio", target_os = "linux"))]

mod common {
    pub(crate) mod sim_bus;
}

use std::sync::Arc;

use common::sim_bus::SimBus;
use mio::{Poll, Token, Waker};
use rsl_usdr::lowlevel::BusError;
use rsl_usdr::{Device, Error};
use rsl_usdr_sim::{BoardRevision, SimBoard};

/// GPO bank driving the LMS6002D reset line (active low).
const GPO_LMS_RST: u8 = 0;

#[test]
fn a_device_on_another_bus_is_refused_and_powered_down() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let device = Device::with_bus(sim.clone()).expect("a healthy board powers up");
    let poll = Poll::new().expect("a reactor");
    let waker = Arc::new(Waker::new(poll.registry(), Token(1)).expect("a waker"));
    let refused = device.into_native(poll.registry(), Token(0), waker);
    assert!(
        matches!(refused, Err(Error::Open(BusError::Unsupported(_)))),
        "only a USB bus hands over: {refused:?}"
    );
    assert_eq!(
        sim.board().gpo(GPO_LMS_RST),
        Some(0),
        "the refused device powered the board down, as dropping it does"
    );
}
