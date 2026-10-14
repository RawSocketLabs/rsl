//! The calibration's refusals, on the simulated board: where libusdr divides by zero
//! (no sample rate) or would send its test tone in place of a stream's samples, usdr
//! refuses before any bus access, so the oracle has nothing to compare. Also the setters
//! that reapply a saved result, which libusdr has no control for.

mod common {
    pub(crate) mod sim_bus;
}

use common::sim_bus::SimBus;
use usdr::{
    Calibration, Device, Error, HostFormat, IqCalibration, RxConfig, StreamsConfig, WireFormat,
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
