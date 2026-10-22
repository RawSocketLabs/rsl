//! The RF calibration's refusals libusdr cannot run, and what it leaves on the simulated
//! XSDR. Operation parity with libusdr itself is `oracle/tests/rf_calibration_parity.rs`.

mod common;

use common::sim_bus::SimBus;
use xsdr::{Calibrations, Device, Error, Mac, TxRoute};
use xsdr_sim::{Loopback, SimBoard, Variant};

/// An XSDR with a TX MMCM and a DUC/DDC in its gateware.
const DUC_DDC: u32 = 0x0030_0024;
/// `DC_0x05C7`, the RX A I DC DAC.
const RX_A_I_DAC: u16 = 0x05c7;

/// Every calibration.
const ALL: Calibrations = Calibrations {
    rx_lo: true,
    tx_lo: true,
    rx_iq: true,
    tx_iq: true,
    external_loopback: false,
    keep_settings: false,
};

/// An opened, powered XSDR on `board`, and the bus to inspect it through.
fn xsdr(board: SimBoard) -> (Device, SimBus) {
    let sim = SimBus::new(board);
    let mut device = Device::open(sim.clone()).expect("an XSDR opens");
    device.power_up().expect("powers up");
    (device, sim)
}

#[test]
fn a_calibration_before_any_rate_or_of_no_one_channel_is_refused_before_any_access() {
    let (mut device, sim) = xsdr(SimBoard::new(Variant::Xsdr).with_hwid(DUC_DDC));
    let before = sim.board().trace().len();
    // libusdr divides the converter clock by its zero divider here.
    assert!(matches!(
        device.calibrate(Mac::A, ALL),
        Err(Error::NoSampleRate)
    ));
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("rate");
    let after_rate = sim.board().trace().len();
    for channels in [Mac::None, Mac::Both] {
        assert!(matches!(
            device.calibrate(channels, ALL),
            Err(Error::NoChannel)
        ));
    }
    assert_eq!(sim.board().trace().len(), after_rate);
    assert!(after_rate > before, "only the rate touched the bus");
}

#[test]
fn the_rx_lo_calibration_leaves_its_dacs_on_the_receivers_offset() {
    let board = SimBoard::new(Variant::Xsdr)
        .with_hwid(DUC_DDC)
        .with_loopback(Loopback {
            rx_dc_offset: [-20.0, 0.0],
            gain: 0.5,
            ..Loopback::default()
        });
    let (mut device, sim) = xsdr(board);
    device
        .set_sample_rate(40_000_000, 40_000_000)
        .expect("rate");
    device.set_rx_frequency(1_000_000_000).expect("rx");
    device.set_tx_frequency(1_200_000_000).expect("tx");
    device.set_tx_path(TxRoute::Band1).expect("band");
    let rx_lo_only = Calibrations {
        rx_lo: true,
        ..Calibrations::default()
    };
    let found = device.calibrate(Mac::A, rx_lo_only).expect("calibrates");
    let [i, q] = found.rx_lo.expect("the RX LO ran");
    // libusdr's golden search answers its last left probe: within a step of the minimum.
    assert!((19..=21).contains(&i) && q.abs() <= 1, "{i} {q}");
    assert_eq!(found.tx_lo, None);
    let written = sim.board().lms7_reg(RX_A_I_DAC) & 0x7ff;
    assert_eq!(i32::from(written), i, "the best X is left in the DAC");
    let level = device.rx_dc_level().expect("measures");
    assert!(
        level < -100_000,
        "the corrector, back on, removes what is left: {level}"
    );
}
