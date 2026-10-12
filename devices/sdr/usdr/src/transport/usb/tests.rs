//! Whole-transfer fixtures at the USB boundary; no hardware or C runtime required.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rsl_usdr_sim::{BoardRevision, I2cAddress, SimBoard};

use super::UsbBus;
use super::bus::{Reply, UsbIo};
use super::protocol::I2cCache;
use super::rx::{Rx, RxQueue};
use crate::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};

/// Scripted complete endpoint transfers and observed outgoing packets.
#[derive(Default)]
struct Script {
    /// Packets in send order.
    writes: Vec<Vec<u8>>,
    /// Endpoint-specific replies, including failures.
    replies: VecDeque<(Reply, Result<Vec<u8>, BusError>)>,
    /// Inject an ambiguous send failure.
    fail_write: bool,
    /// RX cleanup is independent of failed control transactions.
    rx_closes: usize,
}

impl UsbIo for Arc<Mutex<Script>> {
    fn rx_open(&mut self, _: u32) -> Result<(), BusError> {
        Ok(())
    }

    fn rx_recv(&mut self, _: Duration, _: &mut dyn FnMut(&[u8], [u64; 2])) -> Result<(), BusError> {
        Err(BusError::Timeout)
    }

    fn rx_close(&mut self) -> Result<(), BusError> {
        self.lock().unwrap().rx_closes += 1;
        Ok(())
    }
    fn write(&mut self, packet: &[u8], _: Duration) -> Result<(), BusError> {
        let mut script = self.lock().unwrap();
        script.writes.push(packet.to_vec());
        if script.fail_write {
            Err(BusError::Timeout)
        } else {
            Ok(())
        }
    }

    fn read(&mut self, reply: Reply, _: Duration) -> Result<Vec<u8>, BusError> {
        let (expected, result) = self
            .lock()
            .unwrap()
            .replies
            .pop_front()
            .expect("scripted reply");
        assert_eq!(reply, expected);
        result
    }
}

/// Creates a bus with complete, prearranged replies.
fn scripted(replies: Vec<(Reply, Result<Vec<u8>, BusError>)>) -> (UsbBus, Arc<Mutex<Script>>) {
    let script = Arc::new(Mutex::new(Script {
        replies: replies.into(),
        ..Script::default()
    }));
    (UsbBus::from_io(Box::new(Arc::clone(&script))), script)
}

/// Serializes golden words independently of the production bitfield encoder.
fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn rx_timeout_keeps_control_usable_and_cleanup_survives_control_failure() {
    let (mut bus, script) = scripted(vec![
        (Reply::Registers, Ok(words(&[42]))),
        (Reply::Registers, Ok(vec![0])),
    ]);
    bus.rx_stream_open(4096).unwrap();
    assert!(matches!(
        bus.rx_stream_recv(Duration::ZERO, &mut |_, _| panic!()),
        Err(BusError::Timeout)
    ));
    let mut value = [0];
    bus.read_regs(20, &mut value).unwrap();
    assert_eq!(value, [42]);
    assert!(bus.read_regs(20, &mut value).is_err());
    assert!(
        bus.rx_stream_recv(Duration::ZERO, &mut |_, _| panic!())
            .is_err()
    );
    bus.rx_stream_close().unwrap();
    assert_eq!(script.lock().unwrap().rx_closes, 1);
}

/// Scalar IRQ format from `libusb_transfer_ntfy` at the pinned C revision.
fn notification(sequence: u16, event: u8, value: u32) -> Vec<u8> {
    words(&[(u32::from(sequence) << 16) | u32::from(event), value])
}

#[test]
fn raw_register_frames_and_chunk_limits_match_c() {
    let (mut bus, script) = scripted(vec![
        (Reply::Registers, Ok(words(&[0x1234_5678; 64]))),
        (Reply::Registers, Ok(words(&[0x8765_4321]))),
    ]);
    let mut read = [0; 65];
    bus.read_regs(0x1234, &mut read).unwrap();
    assert_eq!(read[..64], [0x1234_5678; 64]);
    assert_eq!(read[64], 0x8765_4321);
    bus.write_regs(0x1234, &[0x1122_3344; 64]).unwrap();
    let script = script.lock().unwrap();
    // usb_async_regread32: 0xc0000000 | ((count - 1) << 16) | addr.
    assert_eq!(script.writes[0], [0x34, 0x12, 0x3f, 0xc0]);
    assert_eq!(script.writes[1], [0x74, 0x12, 0, 0xc0]);
    // OUT_REGOUT_SIZE=256 includes the header, leaving only 63 payload words.
    assert_eq!(script.writes[2].len(), 256);
    assert_eq!(
        &script.writes[2][..8],
        &[0x34, 0x12, 0x3e, 0, 0x44, 0x33, 0x22, 0x11]
    );
    assert_eq!(script.writes[3], words(&[0x1273, 0x1122_3344]));
}

#[test]
fn indexed_registers_are_selected_for_every_word() {
    let (mut bus, script) = scripted(vec![
        (Reply::Registers, Ok(words(&[11]))),
        (Reply::Registers, Ok(words(&[12]))),
    ]);
    bus.write_regs(0x1_0010, &[21, 22]).unwrap();
    let mut read = [0; 2];
    bus.read_regs(0x1_0010, &mut read).unwrap();
    assert_eq!(read, [11, 12]);
    assert_eq!(
        script.lock().unwrap().writes,
        [
            words(&[6, 0x10]),
            words(&[7, 21]),
            words(&[6, 0x11]),
            words(&[7, 22]),
            words(&[6, 0x10]),
            words(&[0xc000_0007]),
            words(&[6, 0x11]),
            words(&[0xc000_0007]),
        ]
    );
}

#[test]
fn spi_and_i2c_use_separate_irq_completions() {
    let (mut bus, script) = scripted(vec![
        (Reply::Notification, Ok(notification(0xffff, 2, 0xabcd))),
        (Reply::Notification, Ok(notification(0, 6, 0x4433_2211))),
    ]);
    assert_eq!(bus.spi32(SpiAddr(0), 0x8000_0001).unwrap(), 0xabcd);
    bus.i2c(I2cAddr::new(0, 0x60), &[1, 2], &mut []).unwrap();
    let mut read = [0; 4];
    bus.i2c(I2cAddr::new(0, 0x60), &[3], &mut read).unwrap();
    assert_eq!(read, [0x11, 0x22, 0x33, 0x44]);
    assert_eq!(
        script.lock().unwrap().writes,
        [
            words(&[2, 0x8000_0001]),
            words(&[0x0001_0000, 0x8000_0060, 0x7800_0201]),
            words(&[0x0001_0000, 0x8000_0060, 0xb400_0003]),
        ]
    );
}

#[test]
fn final_address_is_read_without_range_iteration_overflow() {
    let (mut bus, script) = scripted(vec![
        (Reply::Registers, Ok(words(&[11]))),
        (Reply::Registers, Ok(words(&[12]))),
        (Reply::Registers, Ok(words(&[13]))),
    ]);
    let mut single = [0];
    bus.read_regs(u32::MAX, &mut single).unwrap();
    let mut pair = [0; 2];
    bus.read_regs(u32::MAX - 1, &mut pair).unwrap();
    assert_eq!(single, [11]);
    assert_eq!(pair, [12, 13]);
    assert_eq!(
        script.lock().unwrap().writes,
        [
            words(&[6, 0xfffe_ffff]),
            words(&[0xc000_0007]),
            words(&[6, 0xfffe_fffe]),
            words(&[0xc000_0007]),
            words(&[6, 0xfffe_ffff]),
            words(&[0xc000_0007]),
        ]
    );
}

#[test]
fn cache_replacement_and_bus_flag_match_si2c() {
    let mut cache = I2cCache::default();
    assert_eq!(
        cache.command(0x60, 0, &[0x12, 0x34, 0x56], 2),
        [0x8000_0060, 0x9c56_3412]
    );
    assert_eq!(cache.command(0x6a, 0, &[], 1), [0x8000_6a60, 0x8100_0000]);
    assert_eq!(cache.command(0x75, 0, &[], 0), [0x8075_6a60, 0x7200_0000]);
    assert_eq!(cache.command(0x60, 0, &[], 1), [0x8075_6a60, 0x8000_0000]);
    assert_eq!(cache.command(0x4e, 0, &[], 1), [0x804e_6a60, 0x8200_0000]);
    assert_eq!(cache.command(0x12, 1, &[], 4), [0x924e_6a60, 0xb300_0000]);
}

#[test]
fn coalesced_unrelated_records_and_sequence_wrap_are_consumed() {
    let mut packet = words(&[0xffff_1001, 7, 8]); // IRQ1 with one extra word.
    packet.extend(notification(0, 2, 42));
    packet.extend(notification(1, 3, 99));
    let (mut bus, _) = scripted(vec![
        (Reply::Notification, Ok(packet)),
        (Reply::Notification, Ok(notification(2, 2, 43))),
    ]);
    assert_eq!(bus.spi32(SpiAddr(0), 1).unwrap(), 42);
    assert_eq!(bus.spi32(SpiAddr(0), 2).unwrap(), 43);
}

#[test]
fn malformed_notification_poisons_even_when_target_precedes_bad_tail() {
    let target = notification(1, 2, 42);
    let mut bad_tail = target.clone();
    bad_tail.extend(words(&[2 << 16]));
    let mut duplicate = target.clone();
    duplicate.extend(notification(2, 2, 43));
    let mut gap = target.clone();
    gap.extend(notification(3, 1, 0));
    for packet in [
        vec![],
        vec![0; 3],
        vec![0; 68],
        words(&[0x1002, 0]),
        notification(1, 32, 0),
        bad_tail,
        duplicate,
        gap,
    ] {
        let (mut bus, script) = scripted(vec![(Reply::Notification, Ok(packet))]);
        assert!(bus.spi32(SpiAddr(0), 1).is_err());
        assert!(bus.write_regs(4, &[9]).is_err());
        assert_eq!(script.lock().unwrap().writes.len(), 1);
    }
}

#[test]
fn malformed_register_reply_preserves_output_and_poisons() {
    for reply in [vec![], vec![1; 3], words(&[1, 2])] {
        let (mut bus, script) = scripted(vec![(Reply::Registers, Ok(reply))]);
        let mut value = [0xaaaa_aaaa];
        assert!(bus.read_regs(4, &mut value).is_err());
        assert_eq!(value, [0xaaaa_aaaa]);
        assert!(bus.write_regs(4, &[9]).is_err());
        assert_eq!(script.lock().unwrap().writes.len(), 1);
    }
}

#[test]
fn timeout_never_assigns_delayed_reply_to_a_new_request() {
    for endpoint in [Reply::Registers, Reply::Notification] {
        let (mut bus, script) = scripted(vec![
            (endpoint, Err(BusError::Timeout)),
            (endpoint, Ok(words(&[123]))),
        ]);
        if endpoint == Reply::Registers {
            assert!(matches!(bus.read_regs(4, &mut [0]), Err(BusError::Timeout)));
        } else {
            assert!(matches!(bus.spi32(SpiAddr(0), 1), Err(BusError::Timeout)));
        }
        assert!(bus.write_regs(4, &[9]).is_err());
        assert!(bus.read_regs(4, &mut [0]).is_err());
        let script = script.lock().unwrap();
        assert_eq!(script.writes.len(), 1);
        assert_eq!(script.replies.len(), 1); // Late reply was never reassociated.
    }
}

#[test]
fn ambiguous_write_failure_poisons() {
    let (mut bus, script) = scripted(vec![]);
    script.lock().unwrap().fail_write = true;
    assert!(matches!(bus.write_regs(4, &[9]), Err(BusError::Timeout)));
    assert!(bus.write_regs(4, &[10]).is_err());
    assert_eq!(script.lock().unwrap().writes.len(), 1);
}

#[test]
fn unsupported_requests_do_not_send_or_poison() {
    let (mut bus, script) = scripted(vec![]);
    assert!(bus.spi32(SpiAddr(1), 0).is_err());
    assert!(bus.i2c(I2cAddr::new(2, 0x60), &[], &mut []).is_err());
    assert!(bus.i2c(I2cAddr::new(0, 0x60), &[0; 4], &mut []).is_err());
    assert!(bus.i2c(I2cAddr::new(0, 0x60), &[], &mut [0; 5]).is_err());
    assert!(bus.read_regs(u32::MAX, &mut [0; 2]).is_err());
    assert!(bus.write_regs(u32::MAX, &[0; 2]).is_err());
    bus.read_regs(0, &mut []).unwrap();
    bus.write_regs(0, &[]).unwrap();
    assert_eq!(script.lock().unwrap().writes, Vec::<Vec<u8>>::new());
    bus.write_regs(4, &[9]).unwrap();
}

#[test]
fn initialization_matches_pinned_router_sequence() {
    let (mut bus, script) = scripted(vec![(Reply::Registers, Ok(words(&[0x4000_0300])))]);
    bus.initialize().unwrap();
    let mut expected = vec![words(&[0xc000_0013]), words(&[15, 0x44])];
    for value in [
        0xdead_b000,
        0xeead_b001,
        0xfead_b002,
        0xaead_b003,
        0xbead_b004,
        0xcead_b005,
        0x9ead_b006,
        0x8ead_b007,
    ] {
        expected.push(words(&[8, value]));
    }
    for event in 0..8 {
        expected.push(words(&[15, event | event << 8 | 0x0071_0000]));
        expected.push(words(&[9, event << 16]));
    }
    expected.extend([
        words(&[0, 0x0f00_0080]),
        words(&[6, 0xe0]),
        words(&[7, u32::MAX]),
    ]);
    assert_eq!(script.lock().unwrap().writes, expected);
    let (mut bus, script) = scripted(vec![(Reply::Registers, Ok(words(&[0x0300])))]);
    assert!(matches!(bus.initialize(), Err(BusError::Unsupported(_))));
    assert_eq!(script.lock().unwrap().writes, [words(&[0xc000_0013])]);
}

/// FPGA endpoint emulator: decodes C wire fields, then drives the existing board model.
struct BoardUsb {
    /// The production RX adapter, backed by completed simulator frames.
    rx: Rx<BoardQueue>,
    /// Shared board for final chip-state assertions.
    board: Arc<Mutex<SimBoard>>,
    /// Complete endpoint readbacks.
    registers: VecDeque<Vec<u8>>,
    /// IRQ records.
    notifications: VecDeque<Vec<u8>>,
    /// Global IRQ counter.
    sequence: u16,
    /// Simple I²C lookup-table image.
    lut: u32,
    /// Current virtual register.
    index: u32,
}

/// Whole USB transfers over the board's sample generator, below the production RX adapter.
struct BoardQueue {
    /// The same board the control endpoint configures.
    board: Arc<Mutex<SimBoard>>,
    /// Owned pending transfer buffers.
    pending: VecDeque<Vec<u8>>,
}

impl RxQueue for BoardQueue {
    type Buffer = Vec<u8>;

    fn data(buffer: &Vec<u8>) -> &[u8] {
        buffer
    }

    fn packet_size(&self) -> usize {
        512
    }

    fn alloc(&mut self, capacity: usize) -> Result<Vec<u8>, BusError> {
        Ok(vec![0; capacity])
    }

    fn submit(&mut self, buffer: Vec<u8>) -> Result<(), BusError> {
        self.pending.push_back(buffer);
        Ok(())
    }

    fn wait(&mut self, _: Duration) -> Result<Option<Vec<u8>>, BusError> {
        let Some((block, oob)) = self.board.lock().unwrap().rx_next_block() else {
            return Ok(None);
        };
        let mut buffer = self.pending.pop_front().expect("queued RX buffer");
        buffer.clear();
        buffer.extend(block);
        buffer.extend(oob[0].to_le_bytes());
        buffer.extend(oob[1].to_le_bytes());
        Ok(Some(buffer))
    }

    fn cancel(&mut self) -> Result<(), BusError> {
        self.pending.clear();
        Ok(())
    }
}

impl BoardUsb {
    /// Enqueues a scalar IRQ result.
    fn notify(&mut self, event: u8, value: u32) {
        self.notifications
            .push_back(notification(self.sequence, event, value));
        self.sequence = self.sequence.wrapping_add(1);
    }

    /// Dispatches a physical FPGA register write, independently decoding the I²C fields.
    fn write_reg(&mut self, address: u32, value: u32) {
        match address {
            0 if value & 0x8000_0000 != 0 => self.lut = value,
            1 => {
                let slot = usize::try_from((value >> 24) & 3).unwrap();
                let encoded = self.lut.to_le_bytes()[slot];
                let bytes = value.to_le_bytes();
                let write_count = usize::try_from((value >> 26) & 3).unwrap();
                let read_count = if value >> 31 != 0 {
                    usize::try_from(((value >> 28) & 7) + 1).unwrap()
                } else {
                    0
                };
                let read = self.board.lock().unwrap().i2c(
                    I2cAddress {
                        bus: encoded >> 7,
                        addr: u16::from(encoded & 0x7f),
                    },
                    &bytes[..write_count],
                    read_count,
                );
                if read_count > 0 {
                    let mut data = [0; 4];
                    data[..read.len()].copy_from_slice(&read);
                    self.notify(6, u32::from_le_bytes(data));
                }
            }
            2 => {
                let read = self.board.lock().unwrap().spi(0, value).unwrap();
                self.notify(2, read);
            }
            6 => self.index = value,
            7 => self
                .board
                .lock()
                .unwrap()
                .write_reg(0x1_0000 + self.index, value),
            _ => self.board.lock().unwrap().write_reg(address, value),
        }
    }
}

impl UsbIo for BoardUsb {
    fn rx_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.rx.open(block_bytes)
    }

    fn rx_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        self.rx.receive(timeout, consume)
    }

    fn rx_close(&mut self) -> Result<(), BusError> {
        self.rx.close()
    }
    fn write(&mut self, packet: &[u8], _: Duration) -> Result<(), BusError> {
        let header = u32::from_le_bytes(packet[..4].try_into().unwrap());
        let address = header & 0xffff;
        let count = ((header >> 16) & 0x3f) + 1;
        if header >> 30 == 3 {
            let values: Vec<_> = (address..address + count)
                .map(|reg| {
                    let reg = if reg == 7 { 0x1_0000 + self.index } else { reg };
                    self.board.lock().unwrap().read_reg(reg)
                })
                .collect();
            self.registers.push_back(words(&values));
        } else {
            assert_eq!(packet.len(), usize::try_from((count + 1) * 4).unwrap());
            for (reg, bytes) in (address..).zip(packet[4..].chunks_exact(4)) {
                self.write_reg(reg, u32::from_le_bytes(bytes.try_into().unwrap()));
            }
        }
        Ok(())
    }

    fn read(&mut self, reply: Reply, _: Duration) -> Result<Vec<u8>, BusError> {
        match reply {
            Reply::Registers => self.registers.pop_front(),
            Reply::Notification => self.notifications.pop_front(),
        }
        .ok_or(BusError::Timeout)
    }

    fn sleep(&mut self, duration: Duration) {
        self.board
            .lock()
            .unwrap()
            .sleep_us(u64::try_from(duration.as_micros()).unwrap());
    }
}

#[test]
fn usb_control_plane_identifies_powers_reads_temperature_and_closes_all_revisions() {
    for revision in [
        BoardRevision::Rev1,
        BoardRevision::Rev2,
        BoardRevision::Rev3,
    ] {
        let mut board = SimBoard::new(revision);
        let hwid = board.read_reg(19);
        board.write_reg(19, hwid | 0x4000_0000);
        let board = Arc::new(Mutex::new(board));
        let io = BoardUsb {
            rx: Rx::new(BoardQueue {
                board: Arc::clone(&board),
                pending: VecDeque::new(),
            }),
            board: Arc::clone(&board),
            registers: VecDeque::new(),
            notifications: VecDeque::new(),
            sequence: 0,
            lut: 0,
            index: 0,
        };
        let mut bus = UsbBus::from_io(Box::new(io));
        bus.initialize().unwrap();
        let mut device = crate::Device::with_bus(bus).unwrap();
        assert_eq!(device.temperature().unwrap(), 25.0);
        device.set_rx_sample_rate(20_000_000).unwrap();
        device.set_rx_frequency(1_000_000_000).unwrap();
        let mut generated = 0_u64;
        for count in [1024, 16383, 1024] {
            device.start_rx_stream(count).unwrap();
            let mut samples =
                vec![num_complex::Complex::new(0, 0); usize::try_from(count).unwrap()];
            for packet in 0..2 {
                let received = device.receive(&mut samples, Duration::ZERO).unwrap();
                assert_eq!(received.samples, usize::try_from(count).unwrap());
                assert_eq!(received.first_sample, u64::from(count) * packet);
                assert_eq!(received.lost_packets, 0);
                let expected: Vec<_> = (0..count)
                    .map(|index| {
                        let raw = u64::from(index) + generated;
                        let bytes = raw.to_le_bytes();
                        let value = i16::from_le_bytes([bytes[0], bytes[1]]);
                        num_complex::Complex::new(value, value.wrapping_neg())
                    })
                    .collect();
                assert!(
                    samples == expected,
                    "revision {revision:?}, packet size {count}: first mismatch {:?}",
                    samples.iter().zip(&expected).position(|(a, b)| a != b)
                );
                generated += u64::from(count);
            }
            device.stop_rx_stream().unwrap();
        }
        device.close().unwrap();
        assert_eq!(board.lock().unwrap().gpo(0), Some(0));
    }
}
