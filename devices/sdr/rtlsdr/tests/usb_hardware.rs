//! Opt-in USB smoke: opens and claims a real dongle through `transport::usb::UsbPort`.
//!
//! Blacklist `dvb_usb_rtl28xxu` first (`CAPTURES.md`). None of the identifiers is
//! evidenced yet (`RTL-GAP-01` to `RTL-GAP-03`), so the caller supplies all three from
//! the dongle's own descriptors (`lsusb -v`):
//!
//! ```sh
//! RSL_RTLSDR_USB_DEVICE=vvvv:pppp RSL_RTLSDR_USB_INTERFACE=0 RSL_RTLSDR_USB_RX_ENDPOINT=0x81 \
//!     cargo test -p rtlsdr --features usb --test usb_hardware -- --ignored
//! ```
#![cfg(all(feature = "usb", target_os = "linux"))]

use std::env;

use rtlsdr::rtl2832u::{UsbId, UsbInterface};
use rtlsdr::transport::usb::UsbPort;

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
    UsbPort::open(id, interface).expect("opens and claims the dongle");
}
