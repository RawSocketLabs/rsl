//! How libusdr recognises an `LSDr` and finds out what is fitted: the transport's device ID
//! picks the factory, then open probes each chip (`usdr_device_m2_d09_4_ad45_2_initialize`).

use lsdr_oracle::Oracle;
use lsdr_sim::{Op, SimBoard, Variant};

/// `EIO`: open's "Device is dead!".
const EIO: i32 = 5;
/// `SPI_ADS42LBX9_1`: the second ADC's bus.
const SPI_ADC1: u32 = 1;
/// `SPI_LMK04832`.
const SPI_LMK: u32 = 2;
/// LMK04832 `RESET_CFG` (R330).
const LMK_RESET_CFG: u16 = 0x14a;

/// The SPI words the trace sent on `bus`.
fn spi_on(trace: &[Op], bus: u32) -> Vec<u32> {
    trace
        .iter()
        .filter_map(|op| match op {
            Op::Spi { bus: b, out, .. } if *b == bus => Some(*out),
            _ => None,
        })
        .collect()
}

#[test]
fn the_registered_device_id_selects_the_receive_only_lsdr_factory() {
    let mut oracle =
        Oracle::open(SimBoard::new(Variant::SingleAdc), c"").expect("libusdr opens an LSDr");
    assert_eq!(oracle.device_name().as_deref(), Ok("lsdr"));
    assert_eq!(oracle.get_uint(c"/ll/sdr/max_hw_rx_chans"), Ok(1));
    assert_eq!(
        oracle.get_uint(c"/ll/sdr/max_hw_tx_chans"),
        Ok(0),
        "the LSDr has no TX path"
    );
    let _ = oracle.close();
}

#[test]
fn both_adc_buses_are_probed_but_the_clock_generator_only_on_request() {
    let oracle = Oracle::open(SimBoard::new(Variant::DualAdcClockGen), c"")
        .expect("libusdr opens a dual-ADC LSDr");
    let board = oracle.close();
    assert!(
        !spi_on(board.trace(), SPI_ADC1).is_empty(),
        "open looks for a second ADC"
    );
    assert!(
        spi_on(board.trace(), SPI_LMK).is_empty(),
        "without lmk=1, open leaves the LMK04832 alone"
    );
}

#[test]
fn lmk_1_routes_the_clock_generators_readback_before_reading_its_ids() {
    let oracle = Oracle::open(SimBoard::new(Variant::DualAdcClockGen), c"lmk=1")
        .expect("libusdr opens a dual-ADC LSDr with its LMK04832");
    let board = oracle.close();
    let words = spi_on(board.trace(), SPI_LMK);
    let routed = words
        .iter()
        .position(|&out| out >> 8 == u32::from(LMK_RESET_CFG))
        .expect("open routes the LMK04832 readback");
    let first_read = words
        .iter()
        .position(|&out| out & 0x80_0000 != 0)
        .expect("open reads the LMK04832 IDs");
    assert!(
        routed < first_read,
        "no ID read before the readback is routed"
    );
    assert_eq!(
        board.lmk_reg(LMK_RESET_CFG).map(|cfg| cfg >> 3 & 7),
        Some(6),
        "RESET_MUX stays on SPI readback"
    );
}

#[test]
fn an_absent_clock_generator_is_not_an_error() {
    let oracle = Oracle::open(SimBoard::new(Variant::SingleAdc), c"lmk=1")
        .expect("lmk04832_reset_and_test's -ENODEV is tolerated");
    let board = oracle.close();
    assert!(!spi_on(board.trace(), SPI_LMK).is_empty(), "the probe ran");
}

#[test]
fn an_empty_front_end_connector_yields_no_front_end() {
    let mut oracle =
        Oracle::open(SimBoard::new(Variant::SingleAdc), c"").expect("libusdr opens an LSDr");
    assert!(
        oracle.get_uint(c"/ll/fe/type").is_err(),
        "device_fe_probe registers /ll/fe/type only for a detected front end"
    );
    let _ = oracle.close();
}

#[test]
fn a_dead_link_is_refused_before_anything_is_driven() {
    let err = Oracle::open(SimBoard::new(Variant::SingleAdc).dead(), c"")
        .err()
        .expect("libusdr refuses a board whose register 0 reads all ones");
    assert_eq!(err.errno, -EIO);
    assert_eq!(
        err.board.trace(),
        [Op::RegRead {
            addr: 0,
            value: u32::MAX
        }],
        "only the liveness read may happen"
    );
}
