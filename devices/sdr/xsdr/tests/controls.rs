//! Paths, loopbacks, NCOs and corrections on the simulated XSDR: the board state libusdr's
//! sequences leave, and the refusals libusdr cannot run. Operation parity with libusdr
//! itself is `oracle/tests/controls_parity.rs`.

mod common;

use common::sim_bus::SimBus;
use xsdr::{Device, Direction, Error, Mac, RxRoute, TxRoute};
use xsdr_sim::{SimBoard, Variant};

/// GPO bank `IGPO_TXSW`.
const GPO_TXSW: u8 = 2;
/// GPO bank `IGPO_RXSW`.
const GPO_RXSW: u8 = 3;
/// An XSDR with a TX MMCM in its gateware.
const TX_MMCM: u32 = 0x0030_0020;
/// `RFE_0x010D`, whose bits 8:7 select the LNA.
const RFE_INPUT: u16 = 0x010d;
/// `TRF_0x0101`, whose bit 0 loops the pad back.
const TRF_PAD: u16 = 0x0101;
/// `RXNCO_0x0442`, the RX NCO word's high half.
const RX_NCO_HIGH: u16 = 0x0442;

/// An opened, powered XSDR with a TX MMCM, and the bus to inspect it through.
fn xsdr() -> (Device, SimBus) {
    let sim = SimBus::new(SimBoard::new(Variant::Xsdr).with_hwid(TX_MMCM));
    let mut device = Device::open(sim.clone()).expect("an XSDR opens");
    device.power_up().expect("powers up");
    (device, sim)
}

#[test]
fn an_rf_loopback_feeds_the_lna_from_the_tx_band_on_the_loopback_switch() {
    let (mut device, sim) = xsdr();
    device.set_rx_path(RxRoute::LoopbackW).expect("selected");
    {
        let board = sim.board();
        assert_eq!(board.lms7_reg(RFE_INPUT) >> 7 & 0b11, 3, "LNAW");
        assert_eq!(
            board.gpo(GPO_RXSW),
            Some(1),
            "the wide band's loopback switch"
        );
        assert_eq!(board.lms7_reg(TRF_PAD) & 1, 1, "the pad looped back");
    }
    device.set_rx_path(RxRoute::LnaH).expect("selected");
    let board = sim.board();
    assert_eq!(board.lms7_reg(RFE_INPUT) >> 7 & 0b11, 1, "LNAH");
    assert_eq!(board.gpo(GPO_RXSW), Some(1), "the high band's switch");
    assert_eq!(board.lms7_reg(TRF_PAD) & 1, 0, "the pad out of loopback");
    assert_eq!(board.gpo(GPO_TXSW), Some(1), "band 1's switch, restored");
}

#[test]
fn a_tx_path_by_name_picks_the_band_named_so() {
    let (mut device, sim) = xsdr();
    device.set_tx_path(TxRoute::H).expect("selected");
    assert_eq!(sim.board().gpo(GPO_TXSW), Some(0), "band 2 (H)'s switch");
    device.set_tx_path(TxRoute::W).expect("selected");
    assert_eq!(sim.board().gpo(GPO_TXSW), Some(1), "band 1 (W)'s switch");
}

#[test]
fn an_nco_before_any_sample_rate_is_refused_before_any_access() {
    // libusdr divides by the converter divider, still 0, and crashes.
    let (mut device, sim) = xsdr();
    let before = sim.board().trace().len();
    assert!(matches!(
        device.set_baseband_frequency(Direction::Rx, 1_000),
        Err(Error::NoSampleRate)
    ));
    assert_eq!(sim.board().trace().len(), before);
}

#[test]
fn an_nco_beyond_half_the_converter_clock_names_its_frequency() {
    let (mut device, sim) = xsdr();
    device.set_sample_rate(1_000_000, 1_000_000).expect("rate");
    assert!(matches!(
        device.set_baseband_frequency(Direction::Rx, -20_000_000),
        Err(Error::NcoOutOfRange(-20_000_000))
    ));
    device
        .set_baseband_frequency(Direction::Rx, 8_000_000)
        .expect("a quarter of the 32 MHz ADC clock");
    assert_eq!(
        sim.board().lms7_reg(RX_NCO_HIGH),
        0x4000,
        "2^30 in the high half"
    );
}

#[test]
fn a_dc_offset_on_no_channel_is_refused_before_any_access() {
    let (mut device, sim) = xsdr();
    let before = sim.board().trace().len();
    assert!(matches!(
        device.set_dc_offset(Direction::Tx, Mac::None, 1, 1),
        Err(Error::NoChannel)
    ));
    assert_eq!(sim.board().trace().len(), before);
}
