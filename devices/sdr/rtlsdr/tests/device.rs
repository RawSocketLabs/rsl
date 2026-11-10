//! [`Device`]'s open and close against the captured session, and its refusal of a tuner
//! whose open is not evidenced.

#[path = "common/replay_port.rs"]
mod replay_port;
#[path = "common/wire.rs"]
mod wire;

use std::num::NonZeroUsize;
use std::time::Duration;

use replay_port::ReplayPort;
use rtlsdr::control::{BusError, ControlPort, Setup};
use rtlsdr::r820t2::registers::I2C_ADDRESS;
use rtlsdr::rtl2832u::{I2C_BLOCK, Location, WRITE};
use rtlsdr::{Device, Error};
use rtlsdr_sim::{Trace, Transfer};

/// Characterization: the driver against the capture it was transcribed from, which is the
/// specification here; GPLOBS-V5-0001, 2026-11-09. A new capture of the same tool that
/// differs would be the reason to change the fixture.
#[test]
fn open_then_close_sends_exactly_the_captured_session() {
    let trace = Trace::parse(include_str!("fixtures/gplobs-v5-0001-open-close.trace"))
        .expect("the fixture parses");
    let port = ReplayPort::new(trace);
    let device = Device::open(Box::new(port.clone())).expect("the V5 opens");
    device.close().expect("and closes");
    port.finish()
        .expect("every transfer the capture's, in order, none left over");
}

#[test]
fn the_open_is_the_prefix_of_the_eeprom_session_too() {
    let trace =
        Trace::parse(include_str!("fixtures/gplobs-v5-0004.trace")).expect("the fixture parses");
    let port = ReplayPort::new(trace);
    Device::open(Box::new(port.clone())).expect("the V5 opens");
    let Err(rest) = port.finish() else {
        panic!("rtl_eeprom goes on to read the EEPROM, so transfers remain");
    };
    assert!(
        rest.actual.is_none(),
        "the open matched the capture: {rest:?}"
    );
    // Transfer #143, the first after the open, is rtl_eeprom's first EEPROM access.
    let Some(Transfer::Out { control, .. }) = rest.expected else {
        panic!("the session goes on with a write: {rest:?}");
    };
    assert_eq!(
        (control.value, control.index),
        (0x00a0, 0x0610),
        "an I2C write to 0x50"
    );
}

#[test]
fn the_open_is_the_prefix_of_every_other_v5_session() {
    for text in [
        include_str!("fixtures/gplobs-v5-0002.trace"),
        include_str!("fixtures/gplobs-v5-0003.trace"),
    ] {
        let trace = Trace::parse(text).expect("the fixture parses");
        let capture = trace.capture().to_owned();
        let port = ReplayPort::new(trace);
        Device::open(Box::new(port.clone())).expect("the V5 opens");
        let rest = port.finish().expect_err("the session goes on to stream");
        assert!(rest.actual.is_none(), "{capture}'s open matched: {rest:?}");
    }
}

/// A board as far as the open sees it: the probed addresses answer or stall, and the
/// tuner answers R0 with `r0`; every other access succeeds, reads answering zeros.
struct Board {
    /// Whether the probed addresses (`0x64`, `0x63`) answer.
    probes_answer: bool,

    /// The tuner's R0 on the wire.
    r0: u8,
}

impl Board {
    /// Whether `setup` is an I2C access to a probed address.
    fn probed(setup: Setup) -> bool {
        (setup.index & !WRITE) >> 8 == u16::from(I2C_BLOCK)
            && setup.value != Location::i2c(I2C_ADDRESS).expect("7-bit").value()
    }
}

impl ControlPort for Board {
    fn control_in(
        &mut self,
        setup: Setup,
        data: &mut [u8],
        _: Duration,
    ) -> Result<usize, BusError> {
        if Board::probed(setup) && !self.probes_answer {
            return Err(BusError::Timeout);
        }
        data.fill(0);
        let tuner = Location::i2c(I2C_ADDRESS).expect("7-bit");
        if setup.value == tuner.value() && setup.index == tuner.index() {
            data[0] = self.r0;
        }
        Ok(data.len())
    }

    fn control_out(&mut self, setup: Setup, _: &[u8], _: Duration) -> Result<(), BusError> {
        if Board::probed(setup) && !self.probes_answer {
            return Err(BusError::Timeout);
        }
        Ok(())
    }

    fn rx_start(&mut self, _: usize, _: NonZeroUsize) -> Result<(), BusError> {
        Err(BusError::Timeout)
    }

    fn rx_read(&mut self, _: &mut [u8], _: Duration) -> Result<usize, BusError> {
        Err(BusError::Timeout)
    }

    fn rx_stop(&mut self) -> Result<(), BusError> {
        Ok(())
    }

    fn sleep(&mut self, _: Duration) {}
}

/// The gap `Device::open` refuses `board` with.
fn refusal(board: Board) -> String {
    let Err(Error::Gap(gap)) = Device::open(Box::new(board)) else {
        panic!("only the V5's open is evidenced");
    };
    gap.ledger_id().to_owned()
}

#[test]
fn a_board_like_the_v5_opens() {
    let v5 = Board {
        probes_answer: false,
        r0: 0x69,
    };
    assert!(
        Device::open(Box::new(v5)).is_ok(),
        "probes stall, R0 is 0x96 LSB first"
    );
}

#[test]
fn open_refuses_a_board_where_a_probed_address_answers() {
    let board = Board {
        probes_answer: true,
        r0: 0x69,
    };
    assert_eq!(refusal(board), "RTL-GAP-07");
}

#[test]
fn open_refuses_a_tuner_whose_r0_is_not_the_r820t2s() {
    let board = Board {
        probes_answer: false,
        r0: 0x00,
    };
    assert_eq!(refusal(board), "RTL-GAP-07");
}
