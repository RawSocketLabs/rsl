//! The streams on the simulated LimeSDR-mini: the refusals before any exchange, which
//! libusdr does not make (it writes past buffers, takes a top-bit time for none, or keeps
//! a second stream), and a transport that cannot stream. Operation parity with libusdr
//! itself is `oracle/tests/stream_parity.rs`.

mod common;

use std::time::Duration;

use common::sim_port::SimPort;
use limesdr::control::{BusError, ControlPort};
use limesdr::lms64c::FRAME_LEN;
use limesdr::{Device, Error};
use limesdr_sim::SimBoard;

/// A timeout no simulated wait reaches.
const TIMEOUT: Duration = Duration::from_millis(100);

/// An opened LimeSDR-mini on a fresh board, and the port to inspect it through.
fn lime() -> (Device, SimPort) {
    let sim = SimPort::new(SimBoard::new());
    (Device::open(sim.clone()).expect("the board opens"), sim)
}

#[test]
fn stream_calls_out_of_turn_are_refused_before_any_exchange() {
    let (mut device, sim) = lime();
    let opened = sim.board().trace().len();
    let mut buffer = [[0; 2]; 1020];
    assert!(matches!(
        device.receive(&mut buffer, TIMEOUT),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(
        device.send(&buffer, None, TIMEOUT),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(
        device.destroy_rx_stream(),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(
        device.destroy_tx_stream(),
        Err(Error::NotStreaming)
    ));
    assert_eq!(sim.board().trace().len(), opened);

    device.create_rx_stream(1020).expect("RX stream");
    device.create_tx_stream(4096).expect("TX stream");
    let created = sim.board().trace().len();
    assert!(matches!(
        device.create_rx_stream(1020),
        Err(Error::AlreadyStreaming)
    ));
    assert!(matches!(
        device.create_tx_stream(4096),
        Err(Error::AlreadyStreaming)
    ));
    assert!(matches!(
        device.receive(&mut buffer[..1019], TIMEOUT),
        Err(Error::BufferTooSmall {
            needed: 1020,
            got: 1019,
            ..
        })
    ));
    assert!(matches!(
        device.send(&buffer[..4], Some(1 << 63), TIMEOUT),
        Err(Error::UnsupportedTimestamp(_))
    ));
    let too_many = vec![[0; 2]; 5 * 1020 + 4];
    assert!(matches!(
        device.send(&too_many, None, TIMEOUT),
        Err(Error::SendSize {
            max: 5100,
            multiple: 4,
            ..
        })
    ));
    assert_eq!(sim.board().trace().len(), created, "nothing exchanged");
}

#[test]
fn a_destroyed_stream_can_be_created_again_without_preparing_twice_on_the_bus() {
    let (mut device, sim) = lime();
    device.create_rx_stream(1020).expect("RX stream");
    let created = sim.board().trace().len();
    device.destroy_rx_stream().expect("destroyed");
    assert_eq!(
        sim.board().trace().len(),
        created,
        "destroying exchanges nothing"
    );
    device.create_rx_stream(2040).expect("created again");
    device.start_streams().expect("started");
    let mut buffer = [[0; 2]; 2040];
    let packet = device.receive(&mut buffer, TIMEOUT).expect("received");
    assert_eq!(
        (packet.samples, packet.time, packet.lost_packets),
        (2040, 0, 0)
    );
}

#[test]
fn a_tx_buffer_shorter_than_the_block_is_not_committed() {
    // Four samples frame to 32 bytes: a header and their 16.
    let sim = SimPort::new(SimBoard::new()).with_tx_buffer(31);
    let mut device = Device::open(sim.clone()).expect("the board opens");
    device.create_tx_stream(4096).expect("TX stream");
    let created = sim.board().trace().len();
    assert!(matches!(
        device.send(&[[0; 2]; 4], None, TIMEOUT),
        Err(Error::Stream(BusError::Unsupported(_)))
    ));
    assert_eq!(sim.board().trace().len(), created, "nothing committed");
}

/// A port that only controls the board: the default stream methods.
struct ControlOnly(SimPort);

impl ControlPort for ControlOnly {
    fn exchange(&mut self, request: &[u8; FRAME_LEN]) -> Result<[u8; FRAME_LEN], BusError> {
        self.0.exchange(request)
    }

    fn sleep(&mut self, duration: Duration) {
        self.0.sleep(duration);
    }
}

#[test]
fn a_port_without_streams_fails_the_create_after_the_preparation() {
    let mut device =
        Device::open(ControlOnly(SimPort::new(SimBoard::new()))).expect("the board opens");
    assert!(matches!(
        device.create_rx_stream(1020),
        Err(Error::Stream(BusError::Unsupported(_)))
    ));
    assert!(matches!(
        device.create_tx_stream(0),
        Err(Error::Stream(BusError::Unsupported(_)))
    ));
}

/// A port whose RX blocks are one burst short.
struct ShortBlocks(SimPort);

impl ControlPort for ShortBlocks {
    fn exchange(&mut self, request: &[u8; FRAME_LEN]) -> Result<[u8; FRAME_LEN], BusError> {
        self.0.exchange(request)
    }

    fn sleep(&mut self, duration: Duration) {
        self.0.sleep(duration);
    }

    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.0.rx_stream_open(block_bytes)
    }

    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8]),
    ) -> Result<(), BusError> {
        self.0
            .rx_stream_recv(timeout, &mut |block| consume(&block[4096..]))
    }
}

#[test]
fn a_short_rx_block_is_refused() {
    let mut device =
        Device::open(ShortBlocks(SimPort::new(SimBoard::new()))).expect("the board opens");
    device.create_rx_stream(2040).expect("RX stream");
    device.start_streams().expect("started");
    let mut buffer = [[0; 2]; 2040];
    assert!(matches!(
        device.receive(&mut buffer, TIMEOUT),
        Err(Error::ShortRxBlock {
            expected: 8192,
            got: 4096,
            ..
        })
    ));
}
