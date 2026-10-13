//! USB endpoint faults observed through the public Device API, without hardware.

use std::io::ErrorKind;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use num_complex::Complex;
use rsl_usdr_sim::{Op, SimBoard};

use super::{BoardRevision, BusError, StreamScript, simulated_device};
use crate::Device;
use crate::error::Error;
use crate::stream::{HostFormat, StreamTime, StreamsConfig, TxConfig, WireFormat};

/// A wait the fake endpoints never spend.
const TIMEOUT: Duration = Duration::ZERO;

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

/// Successful startup's wire commands after queue priming, independent of driver internals.
fn post_allocation_start_commands() -> Vec<Vec<u8>> {
    let (mut device, _, script) = simulated_device(BoardRevision::Rev3);
    device.start_rx_stream(1024).unwrap();
    let commands = script.lock().unwrap().control_after_rx.clone();
    assert!(
        !commands.is_empty(),
        "startup sends control commands after allocation"
    );
    device.close().unwrap();
    commands
}

#[test]
fn post_allocation_control_failures_retain_the_stream_and_require_reopening() {
    let commands = post_allocation_start_commands();
    // Fault every IQ-correction, DMA-enable, front-end/window and free-run transfer.
    for index in 0..commands.len() {
        for applied in [false, true] {
            let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
            script.lock().unwrap().fail_control = Some((index, applied));
            let result = device.start_rx_stream(1024);
            assert!(
                matches!(
                    result,
                    Err(Error::Bus {
                        source: BusError::Timeout,
                        ..
                    })
                ),
                "transfer {index}, applied={applied}: {result:?}"
            );
            assert_eq!(script.lock().unwrap().control_after_rx, commands[..=index]);
            if index == commands.len() - 1 {
                assert_eq!(
                    board.lock().unwrap().rx_next_block().is_some(),
                    applied,
                    "a failed final acknowledgment can leave gateware running"
                );
            }
            assert_eq!(script.lock().unwrap().pending, 8);
            assert_eq!(
                device.rx_packet_samples(),
                Some(1024),
                "allocated stream survives failed start"
            );
            assert!(matches!(
                device.start_rx_stream(1024),
                Err(Error::AlreadyStreaming)
            ));
            let sentinel = Complex::new(1234, -5678);
            let mut samples = vec![sentinel; 1024];
            assert!(matches!(
                device.receive(&mut samples, Duration::ZERO),
                Err(Error::Bus {
                    source: BusError::Unsupported(_),
                    ..
                })
            ));
            assert!(samples.iter().all(|sample| *sample == sentinel));
            assert!(matches!(
                device.stop_rx_stream(),
                Err(Error::Bus {
                    source: BusError::Unsupported(_),
                    ..
                })
            ));
            assert_eq!(device.rx_packet_samples(), None);
            assert_eq!(script.lock().unwrap().pending, 0);
            assert_eq!(script.lock().unwrap().cancellations, 1);
            assert!(
                matches!(
                    device.start_rx_stream(1024),
                    Err(Error::Bus {
                        source: BusError::Unsupported(_),
                        ..
                    })
                ),
                "draining RX cannot repair the poisoned control plane"
            );
            // With no stream left, this error proves close still attempts GPO power-down.
            assert!(matches!(
                device.close(),
                Err(Error::Bus {
                    chip: "FPGA",
                    reg: 0,
                    source: BusError::Unsupported(_),
                    ..
                })
            ));
            assert!(script.lock().unwrap().dropped);
            assert_eq!(
                script.lock().unwrap().control_after_rx,
                commands[..=index],
                "no commands reach the endpoint after ambiguous failure"
            );
        }
    }
}

#[test]
fn close_after_post_allocation_start_failure_still_drains_the_endpoint() {
    let commands = post_allocation_start_commands();
    for index in 0..commands.len() {
        for applied in [false, true] {
            let (mut device, _, script) = simulated_device(BoardRevision::Rev3);
            script.lock().unwrap().fail_control = Some((index, applied));
            assert!(matches!(
                device.start_rx_stream(1024),
                Err(Error::Bus {
                    source: BusError::Timeout,
                    ..
                })
            ));
            assert!(
                matches!(
                    device.close(),
                    Err(Error::Bus {
                        chip: "FPGA",
                        reg: 5,
                        source: BusError::Unsupported(_),
                        ..
                    })
                ),
                "transfer {index}, applied={applied}: close attempts DMA stop"
            );
            assert_eq!(script.lock().unwrap().cancellations, 1);
            assert_eq!(script.lock().unwrap().pending, 0);
            assert!(script.lock().unwrap().dropped);
        }
    }
}

#[test]
fn drop_after_post_allocation_start_failure_still_drains_the_endpoint() {
    let commands = post_allocation_start_commands();
    for index in 0..commands.len() {
        for applied in [false, true] {
            let (mut device, _, script) = simulated_device(BoardRevision::Rev3);
            script.lock().unwrap().fail_control = Some((index, applied));
            assert!(matches!(
                device.start_rx_stream(1024),
                Err(Error::Bus {
                    source: BusError::Timeout,
                    ..
                })
            ));
            drop(device);
            assert_eq!(
                script.lock().unwrap().cancellations,
                1,
                "transfer {index}, applied={applied}"
            );
            assert_eq!(script.lock().unwrap().pending, 0);
            assert!(script.lock().unwrap().dropped);
        }
    }
}

/// A powered USB board with a started free-running `ci16` TX stream of 256-sample packets.
fn usb_transmitting() -> (Device, Arc<Mutex<SimBoard>>, Arc<Mutex<StreamScript>>) {
    let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
    device.set_rx_sample_rate(20_000_000).unwrap();
    let config = TxConfig::new(256, HostFormat::Ci16, WireFormat::Ci16);
    device.create_streams(StreamsConfig::tx(config)).unwrap();
    device.start_streams().unwrap();
    (device, board, script)
}

/// Reads of the TX core's first status word (register 28) since trace entry `start`.
fn status_reads(board: &Mutex<SimBoard>, start: usize) -> usize {
    board.lock().unwrap().trace()[start..]
        .iter()
        .filter(|op| matches!(op, Op::RegRead { addr: 28, .. }))
        .count()
}

/// The little-endian words of a byte slice.
fn words_of(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect()
}

#[test]
fn usb_tx_frames_each_block_with_its_time_and_announces_nothing_at_the_registers() {
    let (mut device, board, script) = usb_transmitting();
    // USB layout (bit 3): 5 front-end drops in word 0, 3 DMA drops in word 1's top half.
    board
        .lock()
        .unwrap()
        .set_tx_status([0x0508, 0x0003_0000, 0x1234, 0x5678]);
    let start = board.lock().unwrap().trace().len();
    let samples: Vec<Complex<i16>> = (0..100).map(|n| Complex::new(n, -n)).collect();
    let telemetry = device
        .send_with_stat(&[&samples], Some(StreamTime::from_samples(1000)), TIMEOUT)
        .unwrap()
        .expect("USB reports status with every buffer");
    assert_eq!(
        (
            telemetry.hardware_time,
            telemetry.kernel_time,
            telemetry.underruns,
            telemetry.fifo_used
        ),
        (0x1234, 0x5678, 8, 0)
    );
    device.send(&[&samples[..16]], None, TIMEOUT).unwrap();

    let frames = script.lock().unwrap().tx_frames.clone();
    // `usb_uram_send_dma_commit`'s header, worked by hand: the time; its bits 47:32,
    // samples less one in bits 30:16 and now in 31; bytes less one in bits 31:15 over the
    // time's bits 62:48; zero.
    assert_eq!(words_of(&frames[0][..16]), [1000, 99 << 16, 399 << 15, 0]);
    let payload: Vec<u8> = samples
        .iter()
        .flat_map(|s| [s.re.to_le_bytes(), s.im.to_le_bytes()].concat())
        .collect();
    assert_eq!(frames[0][16..], payload);
    assert_eq!(
        words_of(&frames[1][..16]),
        [u32::MAX, 1 << 31 | 15 << 16 | 0xffff, 63 << 15 | 0x7fff, 0]
    );
    assert_eq!(frames[1].len(), 16 + 64);
    let announced = board.lock().unwrap().trace()[start..]
        .iter()
        .any(|op| matches!(op, Op::RegWrite { addr: 12 | 13, .. }));
    assert!(
        !announced,
        "the header announces the block, not registers 12 and 13"
    );
    assert_eq!(
        status_reads(&board, start),
        1,
        "the first send reads the status"
    );
}

#[test]
fn usb_tx_reads_the_status_every_64th_send_and_repeats_it_between() {
    let (mut device, board, script) = usb_transmitting();
    script.lock().unwrap().tx_finished = usize::MAX;
    let start = board.lock().unwrap().trace().len();
    let samples = vec![Complex::new(0_i16, 0); 16];
    let mut kernel_times = Vec::new();
    for send in 0_u32..130 {
        board.lock().unwrap().set_tx_status([8, 0, 0, send]);
        let telemetry = device
            .send_with_stat(&[&samples], None, TIMEOUT)
            .unwrap()
            .expect("USB reports status with every buffer");
        kernel_times.push(telemetry.kernel_time);
    }
    assert_eq!(status_reads(&board, start), 3);
    let expected: Vec<u32> = (0..130).map(|send| send / 64 * 64).collect();
    assert_eq!(kernel_times, expected, "fresh on sends 0, 64 and 128 only");
}

#[test]
fn a_full_usb_tx_ring_times_out_keeping_its_transfers_and_stop_cancels_them() {
    let (mut device, board, script) = usb_transmitting();
    let samples = vec![Complex::new(0_i16, 0); 16];
    for _ in 0..32 {
        device.send(&[&samples], None, TIMEOUT).unwrap();
    }
    let start = board.lock().unwrap().trace().len();
    assert!(matches!(
        device.send(&[&samples], None, TIMEOUT),
        Err(Error::Timeout)
    ));
    assert_eq!(script.lock().unwrap().tx_frames.len(), 32);
    assert_eq!(
        status_reads(&board, start),
        1,
        "a timeout reads the status for its report; libusdr does so only after a wait, \
         its zero-wait EAGAIN skips the read (divergence)"
    );
    script.lock().unwrap().tx_finished = 1;
    device.send(&[&samples], None, TIMEOUT).unwrap();
    assert_eq!(script.lock().unwrap().tx_frames.len(), 33);
    device.stop_streams().unwrap();
    assert_eq!(script.lock().unwrap().tx_cancellations, 1);
    device.close().unwrap();
}
