//! A smoke test on a real uSDR over `PCIe`: needs the `usdr_pcie_uram` driver and a board at
//! `/dev/usdr0`. Run with `cargo test -p usdr --features pcie -- --ignored`.
#![cfg(all(feature = "pcie", target_os = "linux"))]

use std::os::fd::{AsFd, AsRawFd};
use std::time::Duration;

use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use num_complex::Complex;
use usdr::transport::pcie::PcieBus;
use usdr::{Device, HostFormat, RxConfig, StreamsConfig, WireFormat};

#[test]
#[ignore = "needs a uSDR on /dev/usdr0"]
fn a_real_board_powers_up_reports_its_temperature_and_powers_down() {
    let bus = PcieBus::open("/dev/usdr0").expect("opens /dev/usdr0");
    let mut device = Device::with_bus(bus).expect("powers up");
    let celsius = device.temperature().expect("reads the TMP114");
    assert!(
        (0.0..110.0).contains(&celsius),
        "a plausible board temperature: {celsius}"
    );
    device.close().expect("powers down");
}

#[test]
#[ignore = "needs a uSDR on /dev/usdr0"]
fn the_bus_descriptor_polls_readable_once_rx_blocks_arrive() {
    let bus = PcieBus::open("/dev/usdr0").expect("opens /dev/usdr0");
    let fd = bus
        .as_fd()
        .try_clone_to_owned()
        .expect("duplicates the node");
    let mut device = Device::with_bus(bus).expect("powers up");
    device.set_rx_sample_rate(1_000_000).expect("sets a rate");
    let config = RxConfig::new(4096, HostFormat::Ci16, WireFormat::Ci16);
    device
        .create_streams(StreamsConfig::rx(config))
        .expect("creates RX");
    device.start_streams().expect("starts");

    let mut poll = Poll::new().expect("a poll");
    let raw = fd.as_raw_fd();
    poll.registry()
        .register(&mut SourceFd(&raw), Token(0), Interest::READABLE)
        .expect("registers the descriptor");
    let mut events = Events::with_capacity(4);
    poll.poll(&mut events, Some(Duration::from_secs(1)))
        .expect("polls");
    assert!(
        events.iter().any(mio::event::Event::is_readable),
        "a block arrives within a second at 1 MS/s"
    );
    let mut samples = vec![Complex::new(0_i16, 0); 4096];
    device
        .receive_into(&mut [&mut samples], Duration::ZERO)
        .expect("a readable descriptor means a block is waiting");
    device.stop_streams().expect("stops");
    device.close().expect("powers down");
}
