//! Opt-in USB RX smoke: resets and claims the uSDR at `RSL_USDR_USB_PORT` (sysfs port path).
//! Run `RSL_USDR_USB_PORT=1-2 cargo test -p rsl-usdr --features usb --test usb_hardware -- --ignored`.
#![cfg(all(feature = "usb", target_os = "linux"))]

use std::env;
use std::time::Duration;

use num_complex::Complex;
use rsl_usdr::Device;
use rsl_usdr::transport::usb::UsbBus;

#[test]
#[ignore = "needs RSL_USDR_USB_PORT, a real uSDR and write access to its USB node"]
fn a_real_usb_board_receives_and_restarts_including_packet_aligned_frames() {
    let port = env::var("RSL_USDR_USB_PORT").expect("select the uSDR's sysfs USB port path");
    let info = UsbBus::discover()
        .expect("enumerates USB")
        .into_iter()
        .find(|info| info.port_path().to_string() == port)
        .expect("the selected uSDR is attached");
    let mut device =
        Device::with_bus(UsbBus::open(&info).expect("opens USB")).expect("powers up the board");
    device.set_rx_sample_rate(20_000_000).expect("sets RX rate");
    device.set_rx_frequency(320_000_000).expect("tunes RX");
    // 124/252 ci16 samples plus trailer occupy exactly 512/1024 bytes.
    for count in [1024, 124, 252, 1024] {
        device.start_rx_stream(count).expect("starts RX");
        let mut samples = vec![Complex::new(0, 0); usize::try_from(count).unwrap()];
        for _ in 0..3 {
            let packet = device
                .receive(&mut samples, Duration::from_secs(2))
                .expect("receives another complete USB packet");
            assert_eq!(packet.samples, samples.len());
        }
        device.stop_rx_stream().expect("stops and drains RX");
    }
    device.close().expect("powers down");
}
