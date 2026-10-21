//! The sample rate, the NCO, the temperature and the RX stream's create on the simulated
//! board: the board state libusdr's sequences leave (`device/m2_lsdr/m2_da09_4_ad45_2.c`).
//! Operation parity with libusdr itself is `oracle/tests/rate_parity.rs` and
//! `stream_parity.rs`.

mod common;

use common::sim_bus::SimBus;
use lsdr::{Device, Error, OpenOptions, RxChannels};
use lsdr_sim::{Op, SimBoard, Variant};

/// GPO bank `IGPO_BANK_ADC_NCO_0`; banks 11 to 13 follow.
const GPO_NCO: u8 = 10;
/// The LMK04832's `PLL2_N_LOW`.
const LMK_PLL2_N_LOW: u16 = 0x168;
/// `ADS42LBx9` `REG_CP_GEN`, the test-pattern generator.
const ADC_PATTERN: u8 = 0x0f;

/// Opens `board` with `options`.
fn open(board: SimBoard, options: OpenOptions) -> (SimBus, Device) {
    let sim = SimBus::new(board);
    let device = Device::open(sim.clone(), options).expect("lsdr opens the board");
    (sim, device)
}

#[test]
fn a_rate_enables_the_sample_clock_and_lays_out_a_found_clock_generator() {
    let options = OpenOptions::new().probe_clock_generator(true);
    let (sim, mut device) = open(SimBoard::new(Variant::DualAdcClockGen), options);
    assert!(!sim.board().adc_clock_enabled());
    device.set_sample_rate(50_000_000).expect("50 MS/s");
    let board = sim.board();
    assert!(board.adc_clock_enabled(), "the Si549 drives the ADCs");
    assert_eq!(
        board.lmk_reg(LMK_PLL2_N_LOW),
        Some(7),
        "PLL2's N for 2450 MHz"
    );
    assert_eq!(board.adc_reg(0, ADC_PATTERN), Some(0), "conversion data");
}

#[test]
fn a_rate_out_of_the_oscillators_range_touches_nothing() {
    let (sim, mut device) = open(SimBoard::new(Variant::SingleAdc), OpenOptions::new());
    let before = sim.board().trace().len();
    assert!(matches!(
        device.set_sample_rate(400_000_001),
        Err(Error::UnsupportedSampleRate { chip: "Si549", .. })
    ));
    assert_eq!(sim.board().trace().len(), before);
}

#[test]
fn the_nco_needs_a_rate_then_writes_its_increment() {
    let (sim, mut device) = open(SimBoard::new(Variant::SingleAdc), OpenOptions::new());
    assert!(matches!(
        device.set_rx_frequency(10_000_000),
        Err(Error::NoSampleRate)
    ));
    device.set_sample_rate(50_000_000).expect("50 MS/s");
    device.set_rx_frequency(10_000_000).expect("10 MHz");
    let board = sim.board();
    let word = (0..4).fold(0, |word, byte| {
        let data = board.gpo(GPO_NCO + byte).expect("written");
        word | data << (8 * u32::from(byte))
    });
    assert_eq!(word, 0xca3d_70a4, "(10 MHz - 62.5 MHz) x 2^32 / 250 MHz");
}

#[test]
fn the_temperature_is_the_tmp108s_word_in_degrees() {
    let board = SimBoard::new(Variant::SingleAdc).with_temperature(-0x0180);
    let (_, mut device) = open(board, OpenOptions::new());
    assert_eq!(device.temperature().expect("a reading"), -1.5);
}

#[test]
fn the_rx_stream_is_configured_then_refused_for_want_of_a_sync_unit() {
    let (sim, mut device) = open(SimBoard::new(Variant::SingleAdc), OpenOptions::new());
    assert!(matches!(
        device.create_rx_stream(RxChannels::A, 4096),
        Err(Error::NoSyncUnit)
    ));
    let board = sim.board();
    assert!(board.adc_supply_enabled(), "the ADCs were powered up");
    assert!(
        matches!(
            board.trace().last(),
            Some(Op::RegWrite { addr: 0x1_0040, .. })
        ),
        "the DMA's block size was the last write"
    );
}
