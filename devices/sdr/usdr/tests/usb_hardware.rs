//! Opt-in USB RX smoke: resets and claims the uSDR at `RSL_USDR_USB_PORT` (sysfs port path).
//! Run `RSL_USDR_USB_PORT=1-2 cargo test -p usdr --features usb --test usb_hardware -- --ignored`;
//! add `--features mio` for the reactor-driven temperature read.
#![cfg(all(feature = "usb", target_os = "linux"))]

use std::env;
use std::time::Duration;

use num_complex::Complex;
use usdr::Device;
use usdr::transport::usb::UsbBus;

/// The uSDR at `RSL_USDR_USB_PORT`.
fn selected() -> usb::DeviceInfo {
    let port = env::var("RSL_USDR_USB_PORT").expect("select the uSDR's sysfs USB port path");
    UsbBus::discover()
        .expect("enumerates USB")
        .into_iter()
        .find(|info| info.port_path().to_string() == port)
        .expect("the selected uSDR is attached")
}

#[test]
#[ignore = "needs RSL_USDR_USB_PORT, a real uSDR and write access to its USB node"]
fn a_real_usb_board_receives_and_restarts_including_packet_aligned_frames() {
    let info = selected();
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

#[cfg(feature = "mio")]
#[test]
#[ignore = "needs RSL_USDR_USB_PORT, a real uSDR and write access to its USB node"]
fn a_real_usb_board_reports_its_temperature_through_a_caller_owned_reactor() {
    use std::sync::Arc;
    use std::time::Instant;

    use mio::{Events, Poll, Token, Waker};
    use usdr::transport::usb::NativeEvent;

    const DEVICE: Token = Token(0);
    const WAKE: Token = Token(1);
    let mut poll = Poll::new().unwrap();
    let waker = Arc::new(Waker::new(poll.registry(), WAKE).unwrap());
    let bus = UsbBus::open(&selected()).expect("opens USB");
    let mut native = bus
        .into_native(poll.registry(), DEVICE, waker)
        .expect("checks the sensor and hands the device to the reactor");
    let mut events = Events::with_capacity(8);
    for _ in 0..2 {
        native
            .start_temperature(Instant::now())
            .expect("starts a read");
        let celsius = loop {
            if let Some(NativeEvent::Temperature(result)) = native.poll(Instant::now()) {
                break result.expect("reads the TMP114");
            }
            let timeout = native
                .deadline()
                .map(|deadline| deadline.saturating_duration_since(Instant::now()));
            poll.poll(&mut events, timeout).unwrap();
            for event in &events {
                native.on_ready(event.token());
            }
        };
        assert!(
            (-40.0..=125.0).contains(&celsius),
            "TMP114 range, got {celsius} °C"
        );
    }
    native.close().expect("deregisters");
    let cleaned = loop {
        if let Some(result) = native.try_cleanup() {
            break result;
        }
        poll.poll(&mut events, None).unwrap();
    };
    cleaned.expect("the worker cancels and releases the device");
}

#[cfg(feature = "mio")]
#[test]
#[ignore = "needs RSL_USDR_USB_PORT, a real uSDR and write access to its USB node"]
fn a_set_up_real_device_streams_rx_after_handing_over_to_the_native_handle() {
    use usdr::transport::usb::{ManagedUsb, NativeEvent};
    use usdr::{HostFormat, RxConfig, StreamsConfig, WireFormat};

    /// The next event that is not an RX block.
    fn next_operation(managed: &mut ManagedUsb) -> NativeEvent {
        loop {
            match managed.next_event(Duration::from_secs(2)).unwrap() {
                Some(NativeEvent::Received(_)) => {}
                Some(event) => return event,
                None => panic!("the operation finishes within 2 s"),
            }
        }
    }

    let mut device =
        Device::with_bus(UsbBus::open(&selected()).expect("opens USB")).expect("powers up");
    device.set_rx_sample_rate(20_000_000).expect("sets RX rate");
    device.set_rx_frequency(320_000_000).expect("tunes RX");
    let mut managed = ManagedUsb::from_device(device).expect("hands the device over");
    let config = StreamsConfig::rx(RxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16));
    let now = std::time::Instant::now;
    managed
        .native()
        .start_create_streams(config, now())
        .unwrap();
    let created = next_operation(&mut managed);
    assert!(
        matches!(created, NativeEvent::StreamsCreated(Ok(()))),
        "{created:?}"
    );
    managed.native().start_streams(now()).unwrap();
    let started = next_operation(&mut managed);
    assert!(
        matches!(started, NativeEvent::StreamsStarted(Ok(()))),
        "{started:?}"
    );
    managed.native().start_rx_ready(now()).unwrap();
    let ready = next_operation(&mut managed);
    assert!(matches!(ready, NativeEvent::RxReady(Ok(()))), "{ready:?}");
    for _ in 0..3 {
        match managed.next_event(Duration::from_secs(2)).unwrap() {
            Some(NativeEvent::Received(Ok(block))) => assert_eq!(block.payload.len(), 4096),
            other => panic!("a 1024-sample ci16 block, got {other:?}"),
        }
    }
    managed.native().start_stop_streams(now()).unwrap();
    let stopped = next_operation(&mut managed);
    assert!(
        matches!(stopped, NativeEvent::StreamsStopped(Ok(()))),
        "{stopped:?}"
    );
    managed
        .close(Duration::from_secs(2))
        .expect("the worker releases the device");
    // The hand-over left the board powered: reopen it to power it down.
    let device =
        Device::with_bus(UsbBus::open(&selected()).expect("reopens USB")).expect("powers up");
    device.close().expect("powers down");
}
