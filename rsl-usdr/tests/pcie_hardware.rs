//! A smoke test on a real uSDR over `PCIe`: needs the `usdr_pcie_uram` driver and a board at
//! `/dev/usdr0`. Run with `cargo test -p rsl-usdr --features pcie -- --ignored`.
#![cfg(all(feature = "pcie", target_os = "linux"))]

use rsl_usdr::Device;
use rsl_usdr::transport::pcie::PcieBus;

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
