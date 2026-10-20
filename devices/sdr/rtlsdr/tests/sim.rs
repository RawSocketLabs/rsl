//! The driver against `rtlsdr-sim` through the control seam.

#[path = "common/sim_port.rs"]
mod sim_port;
#[path = "common/synthetic.rs"]
mod synthetic;
#[path = "common/wire.rs"]
mod wire;

use std::num::NonZeroUsize;
use std::time::Duration;

use rtlsdr::control::{BusError, ControlPort};
use rtlsdr::r820t2::{Registers, field};
use rtlsdr::{Device, Error};
use rtlsdr_sim::{Op, SimDongle};
use sim_port::SimPort;
use synthetic::{TIMEOUT, TUNER_REQUEST, setup, wiring};

/// A fresh dongle on the synthetic wiring.
fn port() -> SimPort {
    SimPort::new(SimDongle::new(wiring()))
}

#[test]
fn opening_refuses_with_the_open_gap_before_touching_the_dongle() {
    let port = port();
    let Err(Error::Gap(gap)) = Device::open(Box::new(port.clone())) else {
        panic!("the open sequence is not evidenced, so open must refuse");
    };
    assert_eq!(gap.ledger_id(), "RTL-GAP-07");
    assert!(port.dongle().trace().is_empty(), "nothing reaches the port");
}

#[test]
fn the_tuner_answers_only_while_the_repeater_is_open() {
    let mut port = port();
    let tuner = setup(TUNER_REQUEST, 0, 0);
    assert!(
        port.control_out(tuner, &[0x05, 0x00], TIMEOUT).is_err(),
        "the gate starts closed"
    );
    let registers = synthetic::session(&mut port).expect("the session runs with the gate open");
    assert!(
        port.control_in(tuner, &mut [0; 1], TIMEOUT).is_err(),
        "the session closed the gate again"
    );
    assert!(matches!(port.dongle().trace()[0], Op::Stall { .. }));

    // The driver's page write landed where the independent tuner model puts it, and the
    // driver's decoding of the model's LSB-first read gets the same registers back.
    let dongle = port.dongle();
    assert_eq!(dongle.tuner().registers()[5..8], [0x0A, 0x00, 0x05]);
    assert!(registers.has_chip_id(), "R0 decodes to 0x96");
    assert_eq!(registers.get(field::LNA_GAIN), 0xA);
    assert_eq!(registers.get(field::MIX_GAIN), 0x5);
    assert_eq!(
        registers,
        Registers::from_read(&dongle.tuner().read(8).expect("eight registers")).unwrap()
    );
}

#[test]
fn the_rx_stream_hands_over_fed_transfers_in_order_then_times_out() {
    let mut port = port();
    port.dongle().feed_rx(vec![1, 2, 3, 4]);
    port.dongle().feed_rx(vec![5, 6, 7, 8]);
    let depth = NonZeroUsize::new(2).unwrap();
    port.rx_start(4, depth).unwrap();
    let mut data = [0; 4];
    assert_eq!(port.rx_read(&mut data, TIMEOUT).unwrap(), 4);
    assert_eq!(data, [1, 2, 3, 4]);
    assert_eq!(port.rx_read(&mut data, TIMEOUT).unwrap(), 4);
    assert_eq!(data, [5, 6, 7, 8]);
    assert!(matches!(
        port.rx_read(&mut data, TIMEOUT),
        Err(BusError::Timeout)
    ));
    port.rx_stop().unwrap();
    assert!(port.rx_read(&mut data, TIMEOUT).is_err(), "stopped");
}

#[test]
fn sleeping_advances_only_the_virtual_clock() {
    let mut port = port();
    port.sleep(Duration::from_millis(3));
    port.sleep(Duration::from_micros(250));
    assert_eq!(port.dongle().now_us(), 3_250);
    assert_eq!(port.dongle().trace().last(), Some(&Op::Sleep { us: 250 }));
}
