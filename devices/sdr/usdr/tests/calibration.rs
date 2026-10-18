//! The calibration's refusals, on the simulated board: where libusdr divides by zero
//! (no sample rate) or would send its test tone in place of a stream's samples, usdr
//! refuses before any bus access, so the oracle has nothing to compare. Also the setters
//! that reapply a saved result, which libusdr has no control for.

mod common {
    pub(crate) mod sim_bus;
}

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::sim_bus::SimBus;
use usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use usdr::{
    Calibration, CalibrationReport, Device, Error, HostFormat, IqCalibration, RxConfig,
    StreamsConfig, WireFormat,
};
use usdr_sim::{BoardRevision, Imbalance, Loopback, Op, SimBoard};

/// Every calibration.
const ALL: Calibration = Calibration {
    tx_lo_leakage: true,
    tx_iq_imbalance: true,
    rx_iq_imbalance: true,
    feedback: usdr::Feedback::Internal,
    analog_lo_first: false,
};

#[test]
fn calibrating_before_any_sample_rate_is_refused_before_the_bus() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    let start = sim.board().trace().len();
    let result = device.calibrate(ALL);
    assert!(matches!(result, Err(Error::NoSampleRate)), "{result:?}");
    assert_eq!(sim.board().trace().len(), start, "no bus access");
}

#[test]
fn calibrating_with_a_stream_group_is_refused_before_the_bus() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    device.set_rx_sample_rate(10_000_000).expect("rate");
    device
        .create_streams(StreamsConfig::rx(RxConfig::new(
            1024,
            HostFormat::Ci16,
            WireFormat::Ci16,
        )))
        .expect("stream group");
    let start = sim.board().trace().len();
    let result = device.calibrate(ALL);
    assert!(matches!(result, Err(Error::AlreadyStreaming)), "{result:?}");
    assert_eq!(sim.board().trace().len(), start, "no bus access");
}

/// The RX IQ correction words (sub-registers 2 to 5 of register 56) in `trace`, last first.
fn rx_iq_words(trace: &[Op]) -> Vec<u32> {
    trace
        .iter()
        .rev()
        .filter_map(|op| match *op {
            Op::RegWrite { addr: 56, value } if (2..=5).contains(&(value >> 24)) => Some(value),
            _ => None,
        })
        .take(4)
        .collect()
}

#[test]
fn a_saved_rx_iq_result_reapplies_the_calibrated_correction() {
    let loopback = Loopback {
        converter_clock_hz: 40_000_000,
        tx_lo_leakage: [0.0, 0.0],
        tx_imbalance: Imbalance::default(),
        rx_imbalance: Imbalance {
            gain: 0.03,
            phase: 2_f64.to_radians(),
        },
        gain: 1.0,
        external: false,
    };
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3).with_loopback(loopback));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    device.set_rx_sample_rate(10_000_000).expect("rate");
    device.set_rx_frequency(1_000_000_000).expect("tunes");
    let report = device
        .calibrate(Calibration {
            rx_iq_imbalance: true,
            ..Calibration::default()
        })
        .expect("calibrates");
    let Some(IqCalibration::Corrected(found)) = report.rx_iq_imbalance else {
        panic!("the RX IQ search ran: {report:?}");
    };
    let calibrated = rx_iq_words(sim.board().trace());

    let fresh = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(fresh.clone()).expect("powers up");
    device.set_rx_iq_imbalance(found).expect("reapplies");
    assert_eq!(rx_iq_words(fresh.board().trace()), calibrated);
}

/// One bus access a [`Failing`] bus saw: FPGA register writes with their value, other
/// accesses by kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Access {
    /// A register write.
    Write(u32, u32),
    /// A register read.
    Read(u32),
    /// An SPI exchange.
    Spi(u32),
    /// An I2C transfer.
    I2c,
}

/// The accesses a [`Failing`] bus saw, and the one it fails.
#[derive(Default)]
struct Log {
    /// Every access, in order, the failed one included.
    accesses: Vec<Access>,
    /// The index of the access to fail, which never reaches the board.
    fail_at: Option<usize>,
}

/// A simulated board whose bus fails one chosen access, recording them all.
#[derive(Clone)]
struct Failing {
    /// The board.
    sim: SimBus,
    /// What it saw and what it fails.
    log: Arc<Mutex<Log>>,
}

impl Failing {
    /// Records `access`; the failure if it is the chosen one.
    fn access(&self, access: Access) -> Result<(), BusError> {
        let mut log = self.log.lock().unwrap();
        log.accesses.push(access);
        if log.fail_at == Some(log.accesses.len() - 1) {
            return Err(BusError::Disconnected);
        }
        Ok(())
    }
}

impl Bus for Failing {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.access(Access::Read(addr))?;
        self.sim.read_regs(addr, out)
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        self.access(Access::Write(addr, values[0]))?;
        self.sim.write_regs(addr, values)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.access(Access::Spi(word))?;
        self.sim.spi32(target, word)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.access(Access::I2c)?;
        self.sim.i2c(dev, write, read)
    }

    fn sleep(&mut self, duration: Duration) {
        self.sim.sleep(duration);
    }
}

/// The RX IQ calibration of a receiver tuned alone, so the transmitter it wakes is
/// powered down again at the end, on a fresh looped-back board whose bus fails access
/// `fail_at` of the calibration's own (counted from its first); its result, the accesses
/// it made and the board's trace from its start.
fn calibrate_failing(
    fail_at: Option<usize>,
) -> (Result<CalibrationReport, Error>, Vec<Access>, Vec<Op>) {
    let loopback = Loopback {
        converter_clock_hz: 40_000_000,
        tx_lo_leakage: [0.0, 0.0],
        tx_imbalance: Imbalance::default(),
        rx_imbalance: Imbalance {
            gain: 0.03,
            phase: 2_f64.to_radians(),
        },
        gain: 1.0,
        external: false,
    };
    let bus = Failing {
        sim: SimBus::new(SimBoard::new(BoardRevision::Rev3).with_loopback(loopback)),
        log: Arc::default(),
    };
    let mut device = Device::with_bus(bus.clone()).expect("powers up");
    device.set_rx_sample_rate(10_000_000).expect("rate");
    device.set_rx_frequency(1_000_000_000).expect("tunes");
    let start = bus.log.lock().unwrap().accesses.len();
    bus.log.lock().unwrap().fail_at = fail_at.map(|at| start + at);
    let trace_start = bus.sim.board().trace().len();
    let result = device.calibrate(Calibration {
        rx_iq_imbalance: true,
        ..Calibration::default()
    });
    let accesses = bus.log.lock().unwrap().accesses[start..].to_vec();
    let trace = bus.sim.board().trace()[trace_start..].to_vec();
    (result, accesses, trace)
}

#[test]
fn a_failed_restore_still_powers_down_the_woken_transmitter_and_is_reported() {
    let (ok, accesses, _) = calibrate_failing(None);
    ok.expect("calibrates");
    // The restore's first write: the constant generator off on the TX port.
    let generator_off = accesses
        .iter()
        .position(|access| *access == Access::Write(57, 0x0700_0000))
        .expect("the restore turns the generator off");
    let (failed, after, _) = calibrate_failing(Some(generator_off));
    assert!(
        matches!(failed, Err(Error::Bus { reg: 57, .. })),
        "the restore's failure is the result: {failed:?}"
    );
    let tail = &after[generator_off + 1..];
    assert!(
        !tail.is_empty() && accesses.ends_with(tail),
        "the calibration still ends as it does whole, with the transmitter's power-down"
    );
    assert!(
        tail.iter().all(|access| matches!(access, Access::Spi(_))),
        "the rest of the restore (NCOs, filters) is skipped: {tail:?}"
    );
}

#[test]
fn a_failed_power_down_after_a_calibration_is_ignored() {
    let (ok, accesses, _) = calibrate_failing(None);
    let report = ok.expect("calibrates");
    let last = accesses.len() - 1;
    assert!(
        matches!(accesses[last], Access::Spi(_)),
        "the calibration ends with the transmitter's power-down write: {:?}",
        accesses[last]
    );
    let (ignored, again, _) = calibrate_failing(Some(last));
    assert_eq!(
        ignored.expect("libusdr ignores how the power-down goes"),
        report
    );
    assert_eq!(again.len(), accesses.len());
}

#[test]
fn a_failure_while_measuring_ends_the_calibration_without_restoring() {
    let (ok, accesses, _) = calibrate_failing(None);
    ok.expect("calibrates");
    // The first estimator access: its accumulation setting on the RX port's bank 6.
    let measure = accesses
        .iter()
        .position(|access| matches!(access, Access::Write(56, value) if value >> 24 == 6))
        .expect("the calibration measures");
    let (failed, after, trace) = calibrate_failing(Some(measure));
    assert!(
        matches!(failed, Err(Error::Bus { reg: 56, .. })),
        "{failed:?}"
    );
    assert_eq!(after.len(), measure + 1, "nothing follows the failure");
    assert!(
        !trace.iter().any(|op| matches!(
            op,
            Op::RegWrite {
                addr: 57,
                value: 0x0700_0000
            }
        )),
        "no restore"
    );
}
