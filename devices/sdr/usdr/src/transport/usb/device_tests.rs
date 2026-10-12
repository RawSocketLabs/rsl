//! USB endpoint faults observed through the public Device API, without hardware.

use std::io::ErrorKind;
use std::time::Duration;

use num_complex::Complex;

use super::{BoardRevision, BusError, simulated_device};
use crate::error::Error;

#[test]
fn receive_timeout_preserves_samples_queue_ownership_and_packet_accounting() {
    let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
    device.start_rx_stream(1024).unwrap();
    script.lock().unwrap().waits.push_back(Ok(None));
    let sentinel = Complex::new(1234, -5678);
    let mut samples = vec![sentinel; 1024];
    assert!(matches!(
        device.receive(&mut samples, Duration::ZERO),
        Err(Error::Timeout)
    ));
    assert!(samples.iter().all(|sample| *sample == sentinel));
    assert_eq!(script.lock().unwrap().pending, 8);
    assert_eq!(script.lock().unwrap().cancellations, 0);
    assert_eq!(
        device.temperature().unwrap(),
        25.0,
        "control survives RX timeout"
    );
    let packet = device.receive(&mut samples, Duration::ZERO).unwrap();
    assert_eq!((packet.first_sample, packet.lost_packets), (0, 0));
    assert_eq!(samples[0], Complex::new(0, 0));
    assert_eq!(samples[1023], Complex::new(1023, -1023));
    device.close().unwrap();
    assert_eq!(script.lock().unwrap().pending, 0);
    assert!(script.lock().unwrap().dropped);
    assert_eq!(board.lock().unwrap().gpo(0), Some(0));
}

#[test]
fn partial_setup_failure_drains_the_queue_and_allows_a_fresh_start() {
    let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
    script.lock().unwrap().fail_submit = Some(3);
    assert!(matches!(
        device.start_rx_stream(1024),
        Err(Error::Stream(BusError::Disconnected))
    ));
    assert_eq!(device.rx_packet_samples(), None);
    assert_eq!(script.lock().unwrap().submitted, 3);
    assert_eq!(script.lock().unwrap().pending, 0);
    assert_eq!(script.lock().unwrap().cancellations, 1);
    script.lock().unwrap().fail_submit = None;
    device.start_rx_stream(1024).unwrap();
    let mut samples = vec![Complex::new(0, 0); 1024];
    let packet = device.receive(&mut samples, Duration::ZERO).unwrap();
    assert_eq!((packet.first_sample, packet.lost_packets), (0, 0));
    device.close().unwrap();
    assert!(script.lock().unwrap().dropped);
    assert_eq!(board.lock().unwrap().gpo(0), Some(0));
}

#[test]
fn failed_setup_cancellation_blocks_restart_but_close_drops_the_retained_endpoint() {
    let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
    {
        let mut script = script.lock().unwrap();
        script.fail_submit = Some(3);
        script.fail_cancel = true;
    }
    assert!(matches!(
        device.start_rx_stream(1024),
        Err(Error::Stream(BusError::Disconnected))
    ));
    assert_eq!(device.rx_packet_samples(), None);
    assert_eq!(script.lock().unwrap().pending, 3);
    // The original submit failure takes precedence over the failed cancellation.
    assert!(matches!(
        device.start_rx_stream(1024),
        Err(Error::Stream(BusError::Unsupported(_)))
    ));
    assert_eq!(
        script.lock().unwrap().submitted,
        3,
        "no replacement transfers can be posted"
    );
    assert_eq!(
        script.lock().unwrap().cancellations,
        1,
        "restart cannot retry cleanup behind the caller's back"
    );
    device.close().unwrap();
    assert!(
        script.lock().unwrap().dropped,
        "close destroys even a retained setup-failure endpoint"
    );
    assert_eq!(board.lock().unwrap().gpo(0), Some(0));
}

#[test]
fn malformed_usb_blocks_leave_samples_untouched_then_stop_and_restart_cleanly() {
    for length in [0, 15, 16, 4096, 4111, 4113, 4608] {
        let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
        device.start_rx_stream(1024).unwrap();
        script
            .lock()
            .unwrap()
            .waits
            .push_back(Ok(Some(vec![0; length])));
        let sentinel = Complex::new(1234, -5678);
        let mut samples = vec![sentinel; 1024];
        let result = device.receive(&mut samples, Duration::ZERO);
        assert!(
            matches!(result, Err(Error::Stream(BusError::Io(ref error))) if error.kind() == ErrorKind::InvalidData),
            "frame length {length}: {result:?}"
        );
        assert!(
            samples.iter().all(|sample| *sample == sentinel),
            "frame length {length}"
        );
        assert!(matches!(
            device.receive(&mut samples, Duration::ZERO),
            Err(Error::Stream(BusError::Unsupported(_)))
        ));
        assert!(matches!(
            device.start_rx_stream(1024),
            Err(Error::AlreadyStreaming)
        ));
        device.stop_rx_stream().unwrap();
        assert_eq!(script.lock().unwrap().pending, 0);
        device.start_rx_stream(1024).unwrap();
        let packet = device.receive(&mut samples, Duration::ZERO).unwrap();
        assert_eq!((packet.first_sample, packet.lost_packets), (0, 0));
        assert_eq!(samples[1023], Complex::new(1023, -1023));
        device.close().unwrap();
        assert_eq!(board.lock().unwrap().gpo(0), Some(0));
    }
}

#[test]
fn receive_disconnect_does_not_skip_queue_cleanup_when_control_also_disappears() {
    let (mut device, _, script) = simulated_device(BoardRevision::Rev3);
    device.start_rx_stream(1024).unwrap();
    let mut samples = vec![Complex::new(0, 0); 1024];
    device.receive(&mut samples, Duration::ZERO).unwrap();
    let before = samples.clone();
    script.lock().unwrap().unplugged = true;
    assert!(matches!(
        device.receive(&mut samples, Duration::ZERO),
        Err(Error::Stream(BusError::Disconnected))
    ));
    assert_eq!(samples, before);
    assert!(matches!(
        device.stop_rx_stream(),
        Err(Error::Bus {
            source: BusError::Disconnected,
            ..
        })
    ));
    assert_eq!(device.rx_packet_samples(), None);
    assert_eq!(
        script.lock().unwrap().cancellations,
        1,
        "stop still drains RX after its register write fails"
    );
    assert_eq!(script.lock().unwrap().pending, 0);
    assert!(
        device.close().is_err(),
        "unplugged board cannot acknowledge power-down"
    );
    assert!(
        script.lock().unwrap().dropped,
        "close destroys the endpoint even when power-down fails"
    );
}

#[test]
fn dropping_after_an_rx_transfer_failure_still_drains_and_powers_down() {
    let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
    device.start_rx_stream(1024).unwrap();
    script
        .lock()
        .unwrap()
        .waits
        .push_back(Err(BusError::Disconnected));
    let sentinel = Complex::new(1234, -5678);
    let mut samples = vec![sentinel; 1024];
    assert!(matches!(
        device.receive(&mut samples, Duration::ZERO),
        Err(Error::Stream(BusError::Disconnected))
    ));
    assert!(samples.iter().all(|sample| *sample == sentinel));
    assert_eq!(
        script.lock().unwrap().pending,
        7,
        "completed failure returns one buffer"
    );
    drop(device);
    assert_eq!(script.lock().unwrap().pending, 0);
    assert_eq!(script.lock().unwrap().cancellations, 1);
    assert!(script.lock().unwrap().dropped);
    assert_eq!(board.lock().unwrap().gpo(0), Some(0));
}

#[test]
fn failed_stop_cancellation_prevents_restart_and_does_not_prevent_power_down() {
    let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
    device.start_rx_stream(1024).unwrap();
    script.lock().unwrap().fail_cancel = true;
    assert!(matches!(
        device.stop_rx_stream(),
        Err(Error::Stream(BusError::Timeout))
    ));
    assert_eq!(device.rx_packet_samples(), None);
    assert_eq!(script.lock().unwrap().pending, 8);
    assert!(matches!(device.stop_rx_stream(), Err(Error::NotStreaming)));
    assert!(matches!(
        device.start_rx_stream(1024),
        Err(Error::Stream(BusError::Unsupported(_)))
    ));
    assert_eq!(
        script.lock().unwrap().submitted,
        8,
        "retained transfers prohibit restart"
    );
    assert_eq!(script.lock().unwrap().cancellations, 1);
    device.close().unwrap();
    assert!(script.lock().unwrap().dropped);
    assert_eq!(board.lock().unwrap().gpo(0), Some(0));
}

#[test]
fn usb_loss_trailer_advances_sample_positions_and_accumulates_only_low24_bits() {
    let (mut device, _, script) = simulated_device(BoardRevision::Rev3);
    device.start_rx_stream(1024).unwrap();
    // Reserved upper bits and the completed-burst mask must not count as losses.
    script.lock().unwrap().losses.extend([0, 0xa500_0003, 2]);
    let mut samples = vec![Complex::new(0, 0); 1024];
    for (first, total_lost) in [(0, 0), (4096, 3), (7168, 5)] {
        let packet = device.receive(&mut samples, Duration::ZERO).unwrap();
        assert_eq!(
            (packet.first_sample, packet.lost_packets),
            (first, total_lost)
        );
        let value = i16::try_from(first).unwrap();
        assert_eq!(samples[0], Complex::new(value, -value));
    }
    device.stop_rx_stream().unwrap();
    device.start_rx_stream(1024).unwrap();
    let packet = device.receive(&mut samples, Duration::ZERO).unwrap();
    assert_eq!(
        (packet.first_sample, packet.lost_packets),
        (0, 0),
        "restart resets both counters"
    );
    device.close().unwrap();
}
