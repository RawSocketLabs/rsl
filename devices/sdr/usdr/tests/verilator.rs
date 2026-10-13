//! The Verilator host bridge against a fake model: what it refuses, how it fails, and what
//! it reads. Its wire protocol is compared with libusdr's bridge in
//! `oracle/tests/verilator_parity.rs`.

#![cfg(all(feature = "verilator", target_os = "linux"))]

mod common {
    pub(crate) mod verilator_peer;
}

use std::io::ErrorKind;
use std::time::Duration;

use common::verilator_peer::{Peer, i2c_reply, spi_reply, unwritten};
use rsl_usdr::Error;
use rsl_usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use rsl_usdr::transport::verilator::VerilatorBus;

/// A model greeting with `uuid`.
fn hello(uuid: [u8; 16]) -> Vec<u8> {
    let mut hello = vec![255, 0];
    hello.extend(20u16.to_ne_bytes());
    hello.extend(uuid);
    hello
}

#[test]
fn a_model_of_another_board_is_refused() {
    let peer = Peer::start_with_hello("other-board", hello([7; 16]));
    let refused = VerilatorBus::connect(peer.socket());
    assert!(matches!(
        refused,
        Err(Error::Open(BusError::Unsupported(_)))
    ));
    assert!(
        peer.finish().bytes.is_empty(),
        "nothing is sent to another board"
    );
}

#[test]
fn a_first_packet_other_than_the_uuid_is_refused() {
    // An interrupt where the UUID belongs.
    let mut first = vec![7, 0];
    first.extend(4u16.to_ne_bytes());
    let peer = Peer::start_with_hello("no-uuid", first);
    let refused = VerilatorBus::connect(peer.socket());
    assert!(
        matches!(refused, Err(Error::Open(BusError::Io(ref error))) if error.kind() == ErrorKind::InvalidData)
    );
}

#[test]
fn registers_read_back_what_was_written_directly_and_through_the_window() {
    let peer = Peer::start("readback");
    let mut bus = VerilatorBus::connect(peer.socket()).expect("connects");
    bus.write_regs(20, &[1, 2, 3]).expect("direct write");
    bus.write_regs(0x1_0010, &[4, 5]).expect("window write");
    let mut direct = [0; 4];
    bus.read_regs(20, &mut direct).expect("direct read");
    let mut windowed = [0; 3];
    bus.read_regs(0x1_0010, &mut windowed).expect("window read");
    assert_eq!(direct, [1, 2, 3, unwritten(23)]);
    assert_eq!(windowed, [4, 5, unwritten(0x1_0012)]);
}

#[test]
fn spi_and_i2c_reads_return_the_reply_their_notification_carries() {
    let peer = Peer::start("replies");
    let mut bus = VerilatorBus::connect(peer.socket()).expect("connects");
    let word = 0x1234_5678;
    assert_eq!(bus.spi32(SpiAddr(0), word).expect("SPI"), spi_reply(word));
    let mut read = [0; 3];
    bus.i2c(I2cAddr::new(0, 0x48), &[0x0f], &mut read)
        .expect("I2C");
    // Read bit, two more bytes, one written, slot 0 (0x48's low bits), register 0x0f.
    let command = 1 << 31 | 2 << 28 | 1 << 26 | 0x0f;
    assert_eq!(read, i2c_reply(command).to_le_bytes()[..3]);
}

#[test]
fn accesses_the_bridge_mishandles_are_refused_before_anything_is_sent() {
    let peer = Peer::start("refusals");
    let mut bus = VerilatorBus::connect(peer.socket()).expect("connects");
    let unsupported =
        |result: Result<(), BusError>| matches!(result, Err(BusError::Unsupported(_)));
    assert!(
        unsupported(bus.spi32(SpiAddr(1), 0).map(drop)),
        "SPI target 1"
    );
    assert!(
        unsupported(bus.i2c(I2cAddr::new(1, 0x20), &[], &mut [0])),
        "I2C bus 1"
    );
    assert!(
        unsupported(bus.i2c(I2cAddr::on_core(1, 0, 0x20), &[], &mut [0])),
        "I2C core 1"
    );
    assert!(
        unsupported(bus.i2c(I2cAddr::new(0, 0x48), &[0; 4], &mut [])),
        "four bytes out"
    );
    assert!(
        unsupported(bus.i2c(I2cAddr::new(0, 0x48), &[], &mut [0; 5])),
        "five bytes in"
    );
    assert!(unsupported(bus.read_regs(0, &mut [0; 65])), "65-word read");
    assert!(
        unsupported(bus.rx_stream_open((1 << 20) + 1)),
        "block over 1 MiB"
    );
    let unopened = bus.rx_stream_recv(Duration::ZERO, &mut |_, _| {});
    assert!(unsupported(unopened), "receive without a stream");
    drop(bus);
    let trace = peer.finish();
    // The connection's 32 router writes, the ring's address, four acknowledgements and
    // the ring's mapping: nothing after them.
    assert_eq!(trace.ops.len(), 38, "{:?}", trace.ops.get(38..));
}

#[test]
fn an_rx_wait_with_no_block_times_out_and_leaves_the_bus_usable() {
    let peer = Peer::start("rx-timeout");
    let mut bus = VerilatorBus::connect(peer.socket()).expect("connects");
    bus.rx_stream_open(4096).expect("stream opens");
    let waited = bus.rx_stream_recv(Duration::from_millis(20), &mut |_, _| {
        panic!("no block was delivered")
    });
    assert!(matches!(waited, Err(BusError::Timeout)));
    let mut word = [0];
    bus.read_regs(16, &mut word).expect("the bus still reads");
    assert_eq!(word, [unwritten(16)]);
}

#[test]
fn a_reply_of_the_wrong_length_leaves_the_bus_unusable() {
    let peer = Peer::start("bad-reply");
    let mut bus = VerilatorBus::connect(peer.socket()).expect("connects");
    // Two words, where the read asks for one.
    let mut reply = vec![3, 0];
    reply.extend(12u16.to_ne_bytes());
    reply.extend([0; 8]);
    peer.send_raw(&reply);
    let mut word = [0];
    let first = bus.read_regs(16, &mut word);
    assert!(
        matches!(first, Err(BusError::Io(ref error)) if error.kind() == ErrorKind::InvalidData)
    );
    let next = bus.write_regs(16, &[1]);
    assert!(
        matches!(next, Err(BusError::Disconnected)),
        "nothing more is trusted"
    );
}

#[test]
fn rx_blocks_arrive_in_their_buffers_with_their_status_words_and_are_confirmed() {
    let peer = Peer::start("rx-blocks");
    let mut bus = VerilatorBus::connect(peer.socket()).expect("connects");
    bus.rx_stream_open(64).expect("stream opens");
    let blocks: [&[u8]; 2] = [&[1; 64], &[2; 64]];
    let statuses = peer.deliver_rx(&blocks);
    for (block, status) in blocks.iter().zip(&statuses) {
        let mut received = None;
        bus.rx_stream_recv(Duration::from_secs(1), &mut |data, oob| {
            received = Some((data.to_vec(), oob));
        })
        .expect("a block");
        let (data, oob) = received.expect("lent");
        assert_eq!(data, *block);
        // The lost-packet word and burst mask, then the DMA status.
        let words = [
            u64::from(status[1]) << 32 | u64::from(status[0]),
            u64::from(status[2]),
        ];
        assert_eq!(oob, words);
    }
    drop(bus);
    let confirms: Vec<_> = peer
        .finish()
        .ops
        .into_iter()
        .filter_map(|op| match op {
            common::verilator_peer::Op::Write { reg: 4, words } => Some(words),
            _ => None,
        })
        .collect();
    assert_eq!(
        confirms,
        [vec![0], vec![1]],
        "each buffer confirmed by its number"
    );
}
