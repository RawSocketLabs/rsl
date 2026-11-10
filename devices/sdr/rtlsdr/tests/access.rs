//! Register and tuner I2C access ([`Access`]) against a captured session and against the
//! simulator wired as the captures show.

#[path = "common/replay_port.rs"]
mod replay_port;
#[path = "common/sim_port.rs"]
mod sim_port;
#[path = "common/wire.rs"]
mod wire;

use replay_port::ReplayPort;
use rtlsdr::access::{Access, I2cAddress};
use rtlsdr::r820t2::Registers;
use rtlsdr::r820t2::registers::I2C_ADDRESS;
use rtlsdr::rtl2832u::{GATE, I2C_BLOCK, Location, WRITE};
use rtlsdr_sim::{Control, GateBit, Route, SimDongle, Trace, Wiring};
use sim_port::SimPort;

/// The R820T2's I2C address.
fn tuner() -> I2cAddress {
    I2cAddress::new(I2C_ADDRESS).expect("a 7-bit address")
}

#[test]
fn the_driver_reproduces_the_captured_tuner_probe() {
    let trace = Trace::parse(include_str!("fixtures/gplobs-v5-0001-tuner-probe.trace"))
        .expect("the fixture parses");
    let mut port = ReplayPort::new(trace);
    let mut access = Access::new(&mut port);
    access
        .set_gate(true)
        .expect("the gate write and its read-back");
    // The capture probed two addresses where nothing answers. A trace does not record a
    // stalled transfer's data, so replay matches these on their setup only; the bytes are
    // the pcap's (GPLOBS-V5-0001, `0x02` and `0x00`).
    for (absent, byte) in [(0x64, 0x02), (0x63, 0x00)] {
        let absent = I2cAddress::new(absent).expect("a 7-bit address");
        assert!(
            access.i2c_write(absent, &[byte]).is_err(),
            "no device: a stall"
        );
        assert!(
            access.i2c_read(absent, &mut [0]).is_err(),
            "no device: a stall"
        );
    }
    access.i2c_write(tuner(), &[0x00]).expect("register 0");
    let mut r0 = [0];
    assert_eq!(access.i2c_read(tuner(), &mut r0).expect("R0"), 1);
    assert!(
        Registers::from_read(&r0)
            .expect("one register")
            .has_chip_id(),
        "the tuner is an R820T2"
    );
    port.finish()
        .expect("the driver's transfers are the capture's, in order");
}

#[test]
fn a_stalled_paged_write_is_still_followed_by_its_read_back() {
    let trace = Trace::parse(include_str!("fixtures/gplobs-v5-0002-teardown.trace"))
        .expect("the fixture parses");
    let mut port = ReplayPort::new(trace);
    let mut access = Access::new(&mut port);
    assert!(access.set_gate(true).is_err(), "the gate write stalls");
    // The stalled write's data is the pcap's (GPLOBS-V5-0002); replay matches its setup.
    assert!(
        access.i2c_write(tuner(), &[0x06, 0xb1]).is_err(),
        "so does the tuner's"
    );
    access.set_gate(false).expect("the closing write completes");
    port.finish()
        .expect("the read-back followed the stalled write, as captured");
}

/// The simulator routed as the captures show: the I2C block reaches the tuner at its
/// address and stalls elsewhere; everything else is the register store; the gate is
/// bit 3 of the gate register's written byte.
fn captured_wiring() -> Wiring {
    fn route(control: Control) -> Route {
        let tuner = Location::i2c(I2C_ADDRESS).expect("a 7-bit address");
        match (control.index & !WRITE) >> 8 {
            block if block == u16::from(I2C_BLOCK) && control.value == tuner.value() => {
                Route::Tuner { register: None }
            }
            block if block == u16::from(I2C_BLOCK) => Route::Stall,
            _ => Route::Store,
        }
    }
    Wiring {
        route,
        gate: Some(GateBit {
            value: GATE.value(),
            index: GATE.write_index(),
            bit: 3,
        }),
    }
}

#[test]
fn the_tuner_answers_only_between_opening_and_closing_the_gate() {
    let mut port = SimPort::new(SimDongle::new(captured_wiring()));
    let mut access = Access::new(&mut port);
    assert!(
        access.i2c_read(tuner(), &mut [0]).is_err(),
        "closed at power-up"
    );
    access.set_gate(true).expect("store write");
    let mut r0 = [0];
    access.i2c_read(tuner(), &mut r0).expect("the gate is open");
    assert!(
        Registers::from_read(&r0)
            .expect("one register")
            .has_chip_id(),
        "the tuner answers R0 with the R820T2's chip id"
    );
    access.set_gate(false).expect("store write");
    assert!(access.i2c_read(tuner(), &mut [0]).is_err(), "closed again");
}
