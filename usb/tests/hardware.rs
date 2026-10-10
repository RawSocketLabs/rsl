//! Against a real device, opt-in: `RSL_USB_TEST_DEVICE=vvvv:pppp cargo test -p rsl-usb --
//! --ignored`. It claims interface `RSL_USB_TEST_INTERFACE` (default 0), detaching any kernel
//! driver from it until the device is replugged, so point it at a device you can spare.
#![cfg(target_os = "linux")]

use std::env;
use std::time::Duration;

use rsl_usb::{Setup, devices};

/// `GET_DESCRIPTOR` for the device descriptor (USB 2.0 §9.4.3, §9.6.1).
const GET_DEVICE_DESCRIPTOR: Setup = Setup {
    request_type: 0x80,
    request: 6,
    value: 0x0100,
    index: 0,
};

#[test]
#[ignore = "needs RSL_USB_TEST_DEVICE and write access to its node"]
fn the_device_descriptor_names_the_device_that_was_opened() {
    let wanted = env::var("RSL_USB_TEST_DEVICE").expect("RSL_USB_TEST_DEVICE=vvvv:pppp");
    let (vendor, product) = wanted.split_once(':').expect("vvvv:pppp");
    let ids = (
        u16::from_str_radix(vendor, 16).unwrap(),
        u16::from_str_radix(product, 16).unwrap(),
    );
    let interface: u8 = env::var("RSL_USB_TEST_INTERFACE").map_or(0, |n| n.parse().unwrap());
    let info = devices()
        .unwrap()
        .into_iter()
        .find(|d| (d.vendor_id(), d.product_id()) == ids)
        .expect("the device is attached");

    let device = info.open().unwrap().reset().unwrap();
    let claimed = device.claim_interface(interface).unwrap();
    let mut descriptor = [0; 18];
    let read = claimed
        .control_in(
            GET_DEVICE_DESCRIPTOR,
            &mut descriptor,
            Duration::from_secs(1),
        )
        .unwrap();

    assert_eq!(read, 18, "a device descriptor is 18 bytes");
    assert_eq!(descriptor[1], 1, "bDescriptorType is DEVICE");
    let vendor = u16::from_le_bytes([descriptor[8], descriptor[9]]);
    let product = u16::from_le_bytes([descriptor[10], descriptor[11]]);
    assert_eq!(
        (vendor, product),
        ids,
        "the descriptor matches what sysfs listed"
    );
}
