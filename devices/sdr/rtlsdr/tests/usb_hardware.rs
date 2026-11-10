//! Opt-in USB smoke: opens and claims a real dongle through `transport::usb::UsbPort`.
//!
//! Blacklist `dvb_usb_rtl28xxu` first (`CAPTURES.md`). The caller supplies all three
//! identifiers from the dongle's own descriptors (`lsusb -v`); the Blog V4's are
//! `RTL-DESC-01` and `RTL-DESC-02` (`0bda:2838`, interface 0, endpoint `0x81`):
//!
//! ```sh
//! RSL_RTLSDR_USB_DEVICE=vvvv:pppp RSL_RTLSDR_USB_INTERFACE=0 RSL_RTLSDR_USB_RX_ENDPOINT=0x81 \
//!     cargo test -p rtlsdr --features usb --test usb_hardware -- --ignored
//! ```
#![cfg(all(feature = "usb", target_os = "linux"))]

use std::env;

use rtlsdr::Device;
use rtlsdr::rtl2832u::{UsbId, UsbInterface};
use rtlsdr::transport::usb::UsbPort;
use rtlsdr::v5::{Gain, GainStep};

/// The port the environment names.
fn port() -> UsbPort {
    let device = env::var("RSL_RTLSDR_USB_DEVICE").expect("set RSL_RTLSDR_USB_DEVICE=vvvv:pppp");
    let (vendor, product) = device.split_once(':').expect("vvvv:pppp");
    let id = UsbId {
        vendor: u16::from_str_radix(vendor, 16).expect("hex VID"),
        product: u16::from_str_radix(product, 16).expect("hex PID"),
    };
    let interface = UsbInterface {
        number: u8::try_from(number("RSL_RTLSDR_USB_INTERFACE")).expect("an interface number"),
        rx_endpoint: u8::try_from(number("RSL_RTLSDR_USB_RX_ENDPOINT")).expect("an endpoint"),
    };
    UsbPort::open(id, interface).expect("opens and claims the dongle")
}

/// Environment variable `name`, parsed as hex (with or without `0x`) or decimal.
fn number(name: &str) -> u16 {
    let text = env::var(name).unwrap_or_else(|_| panic!("set {name}"));
    let parsed = match text.strip_prefix("0x") {
        Some(hex) => u16::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.unwrap_or_else(|_| panic!("{name} is a number"))
}

#[test]
#[ignore = "needs RSL_RTLSDR_USB_{DEVICE,INTERFACE,RX_ENDPOINT}, a real dongle and write access to its USB node"]
fn a_real_dongle_opens_and_claims_its_interface() {
    port();
}

/// The V5 only (`RTL-DESC-03`, `-04`): the real tuner's PLL must lock at each frequency.
#[test]
#[ignore = "needs a NESDR SMArt v5 and the env vars above"]
fn a_real_v5_sets_rates_locks_across_its_range_and_closes() {
    let mut device = Device::open(Box::new(port())).expect("the V5 opens");
    for rate in [250_000, 1_024_000, 2_048_000, 2_400_000, 3_200_000] {
        device.set_sample_rate(rate).expect("the rate is set");
        for frequency in [
            26_032_000,
            50_000_000,
            100_000_000,
            433_920_000,
            900_000_000,
            1_766_000_000,
        ] {
            device
                .tune(frequency)
                .unwrap_or_else(|error| panic!("{frequency} Hz at {rate} S/s: {error}"));
        }
    }
    device.set_gain(Gain::Auto).expect("automatic gain");
    device
        .set_gain(Gain::Manual(GainStep::new(28).expect("a step")))
        .expect("the top step");
    device.close().expect("the V5 closes");
}
