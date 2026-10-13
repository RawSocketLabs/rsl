//! Sample rate, LO tuning, gains and bandwidths on the simulated XSDR: the board state
//! libusdr's sequences leave (`device/m2_lm7_1/xsdr_ctrl.c`, `lms7002m_ctrl.c`), and the
//! refusals before them. Operation parity with libusdr itself is
//! `oracle/tests/rate_parity.rs`.

mod common;

use common::sim_bus::SimBus;
use rsl_xsdr::{Device, Direction, Error, Gain};
use rsl_xsdr_sim::{Op, SimBoard, Variant};

/// GPO bank `IGPO_LMS_PWR`.
const GPO_LMS_PWR: u8 = 0;
/// GPO bank `IGPO_RXSW`.
const GPO_RXSW: u8 = 3;
/// An XSDR with a TX MMCM in its gateware.
const TX_MMCM: u32 = 0x0030_0020;
/// `RFE_0x010D`, whose bits 8:7 select the LNA.
const RFE_INPUT: u16 = 0x010d;
/// `LML_0x002A`, whose bits 11:10 select the RX FIFO's source.
const LML_MUX: u16 = 0x002a;

/// An opened XSDR reporting `hwid`, and the bus to inspect it through.
fn xsdr(hwid: u32) -> (Device, SimBus) {
    let sim = SimBus::new(SimBoard::new(Variant::Xsdr).with_hwid(hwid));
    (Device::open(sim.clone()).expect("an XSDR opens"), sim)
}

#[test]
fn a_rate_powers_the_rfic_up_and_leaves_rx_streamed_up_on_the_lfsr_pattern() {
    let (mut device, sim) = xsdr(TX_MMCM);
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("the rate is set");
    let board = sim.board();
    assert_eq!(
        board.gpo(GPO_LMS_PWR),
        Some(0x0f),
        "LDOs, reset release and both enable pins"
    );
    assert_eq!(
        board.lms7_reg(LML_MUX) >> 10 & 0b11,
        2,
        "libusdr leaves the RX FIFO on the LFSR pattern when RX was not running before"
    );
}

#[test]
fn tuning_rx_while_it_runs_moves_the_band_and_its_antenna_switch() {
    let (mut device, sim) = xsdr(TX_MMCM);
    device.set_sample_rate(1_000_000, 1_000_000).expect("rate");
    device.set_rx_frequency(2_400_000_000).expect("tuned");
    {
        let board = sim.board();
        assert_eq!(board.gpo(GPO_RXSW), Some(1), "the high band's switch");
        assert_eq!(board.lms7_reg(RFE_INPUT) >> 7 & 0b11, 1, "LNAH");
    }
    device.set_rx_frequency(1_000_000_000).expect("tuned");
    let board = sim.board();
    assert_eq!(board.gpo(GPO_RXSW), Some(0), "the wide band's switch");
    assert_eq!(board.lms7_reg(RFE_INPUT) >> 7 & 0b11, 3, "LNAW");
}

#[test]
fn the_lo_last_tuned_is_not_tuned_again_but_a_failed_one_is() {
    let (mut device, sim) = xsdr(TX_MMCM);
    device.power_up().expect("powered");
    device.set_rx_frequency(900_000_000).expect("tuned");
    let tuned = sim.board().trace().len();
    device.set_rx_frequency(900_000_000).expect("unchanged");
    assert_eq!(sim.board().trace().len(), tuned, "nothing driven");

    assert!(matches!(
        device.set_rx_frequency(4_100_000_000),
        Err(Error::TuneOutOfRange(4_100_000_000))
    ));
    device.set_rx_frequency(900_000_000).expect("tuned again");
    assert!(
        sim.board().trace().len() > tuned,
        "the failure forgot the last LO"
    );
}

#[test]
fn gateware_without_a_tx_mmcm_fails_the_calibration_after_its_fixed_delay() {
    let (mut device, sim) = xsdr(0x0030_0000);
    assert!(matches!(
        device.set_sample_rate(1_000_000, 1_000_000),
        Err(Error::LinkCalibration)
    ));
    assert!(
        sim.board().trace().contains(&Op::RegWrite {
            addr: 57,
            value: 0x0400_0003
        }),
        "PHY_REG_DLY_VALUE = 3"
    );
}

#[test]
fn invalid_rates_and_unported_gateware_are_refused_before_any_access() {
    let (mut device, sim) = xsdr(TX_MMCM);
    let opened = sim.board().trace().len();
    for (rx, tx) in [(0, 0), (89_999, 0), (0, 195_000_001)] {
        assert!(matches!(
            device.set_sample_rate(rx, tx),
            Err(Error::InvalidRate(_))
        ));
    }
    let (mut rx_mmcm, other) = xsdr(TX_MMCM | 0x10);
    let other_opened = other.board().trace().len();
    assert!(matches!(
        rx_mmcm.set_sample_rate(1_000_000, 1_000_000),
        Err(Error::UnsupportedGateware(0x30))
    ));
    assert_eq!(sim.board().trace().len(), opened);
    assert_eq!(other.board().trace().len(), other_opened);
}

#[test]
fn gains_report_libusdrs_reached_value() {
    let (mut device, _sim) = xsdr(TX_MMCM);
    device.power_up().expect("powered");
    assert_eq!(device.set_gain(Gain::RxLna, 30).ok(), Some(30));
    assert_eq!(
        device.set_gain(Gain::RxPga, 40).ok(),
        Some(31),
        "the PGA tops out at 31 dB"
    );
    assert_eq!(
        device.set_gain(Gain::TxPad, 5).ok(),
        Some(0),
        "a positive pad gain is 0"
    );
    device
        .set_bandwidth(Direction::Rx, 5_000_000)
        .expect("bandwidth");
}
