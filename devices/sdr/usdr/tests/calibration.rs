//! The calibration's refusals, on the simulated board: where libusdr divides by zero
//! (no sample rate) or would send its test tone in place of a stream's samples, rsl-usdr
//! refuses before any bus access, so the oracle has nothing to compare.

mod common {
    pub(crate) mod sim_bus;
}

use common::sim_bus::SimBus;
use rsl_usdr::{Calibration, Device, Error, HostFormat, RxConfig, StreamsConfig, WireFormat};
use rsl_usdr_sim::{BoardRevision, SimBoard};

/// Every calibration.
const ALL: Calibration = Calibration {
    tx_lo_leakage: true,
    tx_iq_imbalance: true,
    rx_iq_imbalance: true,
    feedback: rsl_usdr::Feedback::Internal,
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
