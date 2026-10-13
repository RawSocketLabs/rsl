//! The multi-device aggregate on simulated boards, where libusdr cannot be the reference:
//! its refusals before any bus access, the order of the master's SYSREF across devices,
//! failures libusdr would not survive (a failed open's clean-up, a failed stop) and what a
//! partial receive leaves. `oracle/tests/multi_parity.rs` compares the rest with libusdr.

mod common {
    pub(crate) mod sim_bus;
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use common::sim_bus::SimBus;
use num_complex::Complex;
use rsl_usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr, TxCommit, TxFill};
use rsl_usdr::{
    Device, Error, HostFormat, MultiDevice, MultiError, MultiStep, Outcome, RxConfig,
    StreamsConfig, SyncMode, TxChannels, TxConfig, WireFormat,
};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};

/// The sync unit's register, which the TX core's commands share.
const REG_SYNC: u32 = 14;

/// Samples per packet.
const SAMPLES: u32 = 1024;

/// Every sync-unit write the aggregate's boards saw, in the order they saw them, as
/// (board, mode).
type SyncLog = Arc<Mutex<Vec<(usize, u32)>>>;

/// A simulated board's bus that logs its sync-unit writes in a log the boards share, and
/// fails every register write once `broken` is set.
struct Probe {
    /// The board.
    sim: SimBus,
    /// Its index in the aggregate.
    index: usize,
    /// The shared log.
    log: SyncLog,
    /// Register writes fail.
    broken: Arc<AtomicBool>,
    /// The write that starts the SYSREF generator fails.
    sysref_broken: Arc<AtomicBool>,
}

impl Bus for Probe {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.sim.read_regs(addr, out)
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        if self.broken.load(Ordering::Relaxed) {
            return Err(BusError::Unsupported(
                "the probe's register writes are broken",
            ));
        }
        let generator = addr == REG_SYNC && values.contains(&(1 << 31 | 9 << 16));
        if generator && self.sysref_broken.load(Ordering::Relaxed) {
            return Err(BusError::Unsupported("the probe's SYSREF write is broken"));
        }
        if addr == REG_SYNC {
            let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
            for value in values.iter().filter(|value| *value & 1 << 31 != 0) {
                log.push((self.index, value >> 16 & 0xf));
            }
        }
        self.sim.write_regs(addr, values)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.sim.spi32(target, word)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.sim.i2c(dev, write, read)
    }

    fn sleep(&mut self, duration: Duration) {
        self.sim.sleep(duration);
    }

    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.sim.rx_stream_open(block_bytes)
    }

    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        self.sim.rx_stream_recv(timeout, consume)
    }

    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        self.sim.rx_stream_close()
    }

    fn tx_stream_open(&mut self, block_bytes: u32, sample_bytes: u32) -> Result<(), BusError> {
        self.sim.tx_stream_open(block_bytes, sample_bytes)
    }

    fn tx_stream_send(
        &mut self,
        timeout: Duration,
        time: Option<u64>,
        fill: &mut TxFill<'_>,
    ) -> Result<TxCommit, BusError> {
        self.sim.tx_stream_send(timeout, time, fill)
    }

    fn tx_stream_close(&mut self) -> Result<(), BusError> {
        self.sim.tx_stream_close()
    }
}

/// Three boards, the aggregate's devices, and a switch per board that breaks its writes.
struct Rig {
    /// The boards.
    sims: Vec<SimBus>,
    /// Each board's switch.
    broken: Vec<Arc<AtomicBool>>,
    /// Each board's switch for the SYSREF generator's write.
    sysref_broken: Vec<Arc<AtomicBool>>,
    /// The shared sync log.
    log: SyncLog,
}

impl Rig {
    /// Three revision-3 boards, `adjust`ed by index.
    fn new(adjust: impl Fn(usize, SimBoard) -> SimBoard) -> Self {
        Self {
            sims: (0..3)
                .map(|index| SimBus::new(adjust(index, SimBoard::new(BoardRevision::Rev3))))
                .collect(),
            broken: (0..3).map(|_| Arc::default()).collect(),
            sysref_broken: (0..3).map(|_| Arc::default()).collect(),
            log: SyncLog::default(),
        }
    }

    /// Opens the aggregate over the boards.
    fn open(&self) -> Result<MultiDevice, MultiError<()>> {
        let builder = |index: usize| {
            Device::builder(Probe {
                sim: self.sims[index].clone(),
                index,
                log: Arc::clone(&self.log),
                broken: Arc::clone(&self.broken[index]),
                sysref_broken: Arc::clone(&self.sysref_broken[index]),
            })
        };
        MultiDevice::open(builder(0), (1..3).map(builder))
    }

    /// Opens the aggregate, sets its rate and tunes its receivers.
    fn tuned(&self) -> MultiDevice {
        let mut multi = self.open().expect("opens");
        multi
            .broadcast(|device| device.set_rx_sample_rate(20_000_000))
            .expect("rate");
        multi
            .broadcast(|device| device.set_rx_frequency(1_000_000_000))
            .expect("tunes");
        multi
    }

    /// Each board's trace length now.
    fn lens(&self) -> Vec<usize> {
        self.sims
            .iter()
            .map(|sim| sim.board().trace().len())
            .collect()
    }
}

/// The boards unchanged.
fn plain(_: usize, board: SimBoard) -> SimBoard {
    board
}

/// An RX stream of [`SAMPLES`] `ci16` samples.
fn rx() -> StreamsConfig {
    StreamsConfig::rx(RxConfig::new(SAMPLES, HostFormat::Ci16, WireFormat::Ci16))
}

/// A device that fails to open powers the ones before it down again, as libusdr destroys
/// them, and the ones after it are never opened.
#[test]
fn a_device_that_fails_to_open_powers_the_ones_before_it_down() {
    let rig = Rig::new(|index, board| {
        if index == 1 {
            board.without_temperature_sensor()
        } else {
            board
        }
    });
    let result = rig.open();
    let Err(MultiError::Devices { step, outcomes }) = result else {
        panic!("device 1 fails to open: {result:?}");
    };
    assert_eq!(step, MultiStep::Open);
    assert!(
        matches!(
            outcomes[..],
            [
                Outcome::Done(()),
                Outcome::Failed(Error::ChipId { .. }),
                Outcome::NotReached
            ]
        ),
        "{outcomes:?}"
    );

    let alone = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    Device::with_bus(alone.clone())
        .expect("opens")
        .close()
        .expect("closes");
    assert_eq!(
        rig.sims[0].board().trace(),
        alone.board().trace(),
        "device 0 was opened and closed again, as one device alone is"
    );
    assert!(
        rig.sims[2].board().trace().is_empty(),
        "device 2 was never opened"
    );
}

/// A stream configuration one device's gateware cannot take (a second TX channel without
/// the TX chain) is refused on that device before any device's bus is touched.
#[test]
fn a_device_that_refuses_the_streams_stops_the_create_before_any_bus_access() {
    let rig = Rig::new(|index, board| {
        if index == 1 {
            board.with_rx_only()
        } else {
            board
        }
    });
    let mut multi = rig.tuned();
    let before = rig.lens();
    let tx = TxConfig::new(SAMPLES, HostFormat::Ci16, WireFormat::Ci16)
        .with_channels(TxChannels::Channel1);
    let result = multi.create_streams(rx().with_tx(tx));
    let Err(MultiError::Devices { step, outcomes }) = result else {
        panic!("device 1 refuses: {result:?}");
    };
    assert_eq!(step, MultiStep::Check);
    assert!(
        matches!(
            outcomes[..],
            [
                Outcome::NotReached,
                Outcome::Failed(Error::UnsupportedTxChannels { .. }),
                Outcome::NotReached
            ]
        ),
        "{outcomes:?}"
    );
    assert_eq!(rig.lens(), before, "no board was touched");
}

/// Stream calls without the streams they need, and buffers that do not match the
/// aggregate's channels, are refused before any bus access.
#[test]
fn stream_calls_the_aggregate_cannot_make_touch_no_board() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    let before = rig.lens();
    let mut buffers = vec![vec![Complex::new(0_i16, 0); SAMPLES as usize]; 3];
    let mut channels: Vec<&mut [Complex<i16>]> =
        buffers.iter_mut().map(Vec::as_mut_slice).collect();
    let result = multi.receive_into(&mut channels, Duration::ZERO);
    assert!(
        matches!(
            result,
            Err(MultiError::Devices { step: MultiStep::Check, ref outcomes })
                if outcomes.iter().all(|o| matches!(o, Outcome::Failed(Error::NotStreaming)))
        ),
        "no device has an RX stream: {result:?}"
    );
    assert!(matches!(
        multi.stop_streams(),
        Err(MultiError::Refused(Error::NotStreaming))
    ));
    assert!(matches!(
        multi.start_streams(),
        Err(MultiError::Devices {
            step: MultiStep::Check,
            ..
        })
    ));

    multi.create_streams(rx()).expect("creates");
    let created = rig.lens();
    let result = multi.receive_into(&mut channels[..2], Duration::ZERO);
    assert!(
        matches!(
            result,
            Err(MultiError::Refused(Error::ChannelBuffers {
                expected: 3,
                got: 2,
                ..
            }))
        ),
        "one buffer per device: {result:?}"
    );
    assert_eq!(rig.lens(), created, "the refusal touched no board");
    assert_ne!(before, created);
}

/// With a generated SYSREF, every device arms on 1PPS before the master's generator
/// starts, the order libusdr's aggregate sync writes.
#[test]
fn the_master_generates_sysref_after_every_device_waits_for_it() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    multi
        .create_streams(rx().with_sync(SyncMode::GeneratedSysref))
        .expect("creates");
    rig.log
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    multi.start_streams().expect("starts");
    assert_eq!(
        *rig.log.lock().unwrap_or_else(PoisonError::into_inner),
        [(0, 1), (1, 1), (2, 1), (0, 9)],
        "each device waits for 1PPS, then the master alone generates SYSREF"
    );
}

/// A device of a generated-SYSREF aggregate keeps the 1PPS wait as its own mode: started
/// on its own, it does not generate SYSREF itself, which libusdr's aggregate refuses.
#[test]
fn a_member_started_on_its_own_waits_for_1pps() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    multi
        .create_streams(rx().with_sync(SyncMode::GeneratedSysref))
        .expect("creates");
    rig.log
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    multi
        .device_mut(1)
        .expect("has device 1")
        .start_streams()
        .expect("starts device 1");
    assert_eq!(
        *rig.log.lock().unwrap_or_else(PoisonError::into_inner),
        [(1, 1)]
    );
    assert!(
        matches!(
            multi.start_streams(),
            Err(MultiError::Devices { step: MultiStep::Check, ref outcomes })
                if matches!(outcomes[1], Outcome::Failed(Error::AlreadyStreaming))
        ),
        "the aggregate will not start a device twice"
    );
}

/// A stop attempts every device: one whose bus fails does not keep the others' groups
/// alive, and each device's outcome is reported.
#[test]
fn a_stop_attempts_every_device_when_one_fails() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    multi.create_streams(rx()).expect("creates");
    multi.start_streams().expect("starts");
    rig.broken[1].store(true, Ordering::Relaxed);
    let result = multi.stop_streams();
    let Err(MultiError::Devices { step, outcomes }) = result else {
        panic!("device 1 fails to stop: {result:?}");
    };
    assert_eq!(step, MultiStep::Stop);
    assert!(
        matches!(
            outcomes[..],
            [Outcome::Done(()), Outcome::Failed(_), Outcome::Done(())]
        ),
        "{outcomes:?}"
    );
    for index in 0..3 {
        assert_eq!(
            multi.device(index).expect("has it").rx_packet_samples(),
            None,
            "device {index}'s group is gone"
        );
    }
}

/// A receive that fails on device 1 leaves device 0 a packet ahead: the next receive's
/// times show it.
#[test]
fn a_failed_receive_leaves_the_devices_before_it_a_packet_ahead() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    multi.create_streams(rx()).expect("creates");
    multi.start_streams().expect("starts");
    let mut buffers = vec![vec![Complex::new(0_i16, 0); SAMPLES as usize]; 3];
    let mut channels: Vec<&mut [Complex<i16>]> =
        buffers.iter_mut().map(Vec::as_mut_slice).collect();

    rig.sims[1].board().write_reg(REG_SYNC, 1 << 31);
    let result = multi.receive_into(&mut channels, Duration::ZERO);
    assert!(
        matches!(
            result,
            Err(MultiError::Devices { step: MultiStep::Call, ref outcomes })
                if matches!(outcomes[..], [Outcome::Done(_), Outcome::Failed(Error::Timeout), Outcome::NotReached])
        ),
        "{result:?}"
    );

    rig.sims[1].board().write_reg(REG_SYNC, 1 << 31 | 7 << 16);
    let metadata = multi
        .receive_into(&mut channels, Duration::ZERO)
        .expect("every device delivers");
    let times: Vec<u64> = metadata.iter().map(|m| m.time.samples()).collect();
    assert_eq!(
        times,
        [u64::from(SAMPLES), 0, 0],
        "device 0 delivered its first packet in the failed receive"
    );
}

/// A device's own setting reaches that device alone.
#[test]
fn a_devices_own_setting_reaches_it_alone() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    let before = rig.lens();
    multi
        .device_mut(2)
        .expect("has device 2")
        .set_rx_frequency(1_200_000_000)
        .expect("tunes device 2");
    let after = rig.lens();
    assert_eq!(after[..2], before[..2], "devices 0 and 1 untouched");
    assert!(after[2] > before[2]);
    assert!(
        rig.sims[2].board().trace()[before[2]..]
            .iter()
            .any(|op| matches!(op, Op::Spi { .. })),
        "device 2 retuned its PLL"
    );
    assert_eq!(multi.device_count(), 3);
    assert!(multi.device(3).is_none());
}

/// RX and TX streams of [`SAMPLES`] `ci16` samples.
fn duplex() -> StreamsConfig {
    rx().with_tx(TxConfig::new(SAMPLES, HostFormat::Ci16, WireFormat::Ci16))
}

/// A started group starts no second time: every device refuses before any bus access.
#[test]
fn a_started_group_is_refused_a_second_start_on_every_device() {
    for config in [rx(), duplex()] {
        let rig = Rig::new(plain);
        let mut multi = rig.tuned();
        multi.create_streams(config).expect("creates");
        multi.start_streams().expect("starts");
        let started = rig.lens();
        let result = multi.start_streams();
        assert!(
            matches!(
                result,
                Err(MultiError::Devices { step: MultiStep::Check, ref outcomes })
                    if outcomes.iter().all(|o| matches!(o, Outcome::Failed(Error::AlreadyStreaming)))
            ),
            "{config:?}: {result:?}"
        );
        assert_eq!(rig.lens(), started, "{config:?}: no board touched");
    }
}

/// After a TX create failed on device 1, devices 1 and 2 hold RX alone while the master
/// holds both: the aggregate will not start them together.
#[test]
fn devices_left_with_other_streams_than_the_master_are_not_started() {
    let rig = Rig::new(|index, board| {
        if index == 1 {
            board.with_gateware_build(0xd2b1_0c08)
        } else {
            board
        }
    });
    let mut multi = rig.tuned();
    assert!(
        multi.create_streams(duplex()).is_err(),
        "device 1's TX fails"
    );
    let before = rig.lens();
    let result = multi.start_streams();
    assert!(
        matches!(
            result,
            Err(MultiError::Devices { step: MultiStep::Check, ref outcomes })
                if matches!(
                    outcomes[..],
                    [
                        Outcome::NotReached,
                        Outcome::Failed(Error::UnmatchedStreams),
                        Outcome::Failed(Error::UnmatchedStreams)
                    ]
                )
        ),
        "{result:?}"
    );
    assert_eq!(rig.lens(), before, "no board touched");
    multi.stop_streams().expect("stops what the create left");
}

/// The master failing its SYSREF write is reported at that step, after every device's
/// sync; the members are not part of it.
#[test]
fn a_master_that_fails_to_generate_sysref_is_reported_at_that_step() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    multi
        .create_streams(rx().with_sync(SyncMode::GeneratedSysref))
        .expect("creates");
    rig.sysref_broken[0].store(true, Ordering::Relaxed);
    rig.log
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    let result = multi.start_streams();
    assert!(
        matches!(
            result,
            Err(MultiError::Devices { step: MultiStep::MasterSysref, ref outcomes })
                if matches!(
                    outcomes[..],
                    [Outcome::Failed(_), Outcome::NotReached, Outcome::NotReached]
                )
        ),
        "{result:?}"
    );
    assert_eq!(
        *rig.log.lock().unwrap_or_else(PoisonError::into_inner),
        [(0, 1), (1, 1), (2, 1)],
        "every device waits for 1PPS; the generator never started"
    );
}

/// A member whose group was recreated on its own in another mode no longer waits for
/// 1PPS, so the master generates no SYSREF for it.
#[test]
fn the_master_generates_no_sysref_once_a_member_left_the_1pps_wait() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    multi
        .create_streams(rx().with_sync(SyncMode::GeneratedSysref))
        .expect("creates");
    let member = multi.device_mut(2).expect("has device 2");
    member.stop_streams().expect("stops device 2's group");
    member
        .create_streams(rx())
        .expect("creates it free-running");
    rig.log
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    multi.start_streams().expect("starts");
    assert_eq!(
        *rig.log.lock().unwrap_or_else(PoisonError::into_inner),
        [(0, 1), (1, 1), (2, 7)],
        "no generator write"
    );
}

/// Buffers any device's stream cannot take, and sends of different lengths per device,
/// are refused before any bus access.
#[test]
fn buffers_a_device_cannot_take_are_refused_before_any_bus_access() {
    let rig = Rig::new(plain);
    let mut multi = rig.tuned();
    multi.create_streams(duplex()).expect("creates");
    multi.start_streams().expect("starts");
    let before = rig.lens();

    let lengths = [SAMPLES as usize, SAMPLES as usize, 10];
    let mut buffers: Vec<Vec<Complex<i16>>> = lengths
        .iter()
        .map(|&length| vec![Complex::new(0, 0); length])
        .collect();
    let mut channels: Vec<&mut [Complex<i16>]> =
        buffers.iter_mut().map(Vec::as_mut_slice).collect();
    let result = multi.receive_into(&mut channels, Duration::ZERO);
    assert!(
        matches!(
            result,
            Err(MultiError::Devices { step: MultiStep::Check, ref outcomes })
                if matches!(
                    outcomes[..],
                    [
                        Outcome::NotReached,
                        Outcome::NotReached,
                        Outcome::Failed(Error::BufferTooSmall { .. })
                    ]
                )
        ),
        "device 2's buffer is short: {result:?}"
    );

    let packets: Vec<Vec<Complex<i16>>> = [16, 16, 8]
        .iter()
        .map(|&length| vec![Complex::new(1, 1); length])
        .collect();
    let channels: Vec<&[Complex<i16>]> = packets.iter().map(Vec::as_slice).collect();
    let result = multi.send(&channels, None, Duration::ZERO);
    assert!(
        matches!(
            result,
            Err(MultiError::Refused(Error::SendSize {
                max: 16,
                got: 8,
                ..
            }))
        ),
        "one sample count for every device: {result:?}"
    );

    let floats = vec![vec![Complex::new(0.0_f32, 0.0); 16]; 3];
    let channels: Vec<&[Complex<f32>]> = floats.iter().map(Vec::as_slice).collect();
    let result = multi.send(&channels, None, Duration::ZERO);
    assert!(
        matches!(
            result,
            Err(MultiError::Devices { step: MultiStep::Check, ref outcomes })
                if outcomes.iter().all(|o| matches!(o, Outcome::Failed(Error::HostFormatMismatch { .. })))
        ),
        "the streams take ci16: {result:?}"
    );
    assert_eq!(rig.lens(), before, "no board touched");
}
