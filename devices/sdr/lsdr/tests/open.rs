//! Opening, powering and closing an `LSDr` on the simulated board: the board state libusdr's
//! sequences leave (`device/m2_lsdr/m2_da09_4_ad45_2.c`). Operation parity with libusdr
//! itself is `oracle/tests/session_parity.rs`.

mod common;

use common::sim_bus::SimBus;
use rsl_lsdr::{Device, Error, Fitted, OpenOptions, PgaGain};
use rsl_lsdr_sim::{Op, SimBoard, Variant};

/// GPO bank `IGPO_BANK_CFG_ADC`: bits 1:0 hold the ADCs in reset.
const GPO_CFG_ADC: u8 = 2;
/// GPO bank `IGPO_BANK_PGA_LE`.
const GPO_PGA_LE: u8 = 3;
/// GPO bank `IGPO_BANK_PGA_GAIN`.
const GPO_PGA_GAIN: u8 = 4;
/// The TPS6594's `BUCK1_VOUT_1` .. `BUCK4_VOUT_1`.
const BUCK_VOUT: [u8; 4] = [0x0e, 0x10, 0x12, 0x14];
/// `ADS42LBx9` `REG_PWR_CONTROL`.
const ADC_PWR_CONTROL: u8 = 0x08;

/// A board of `variant`, opened with `options`, and the bus to inspect it through.
fn open(variant: Variant, options: OpenOptions) -> (Device, SimBus) {
    let sim = SimBus::new(SimBoard::new(variant));
    let device = Device::open(sim.clone(), options).expect("an LSDr opens");
    (device, sim)
}

#[test]
fn open_sets_the_rails_finds_the_adc_and_leaves_it_unsupplied_and_unclocked() {
    let (device, sim) = open(Variant::SingleAdc, OpenOptions::new());
    assert_eq!(
        device.fitted(),
        Fitted {
            adcs: [true, false],
            clock_generator: false,
        }
    );
    let board = sim.board();
    // libusdr's `tps6594_buck_vout`: 920 mV -> 0x0F + 320 / 5, 2200 mV -> 0xAB + 540 / 20,
    // 1200 mV -> 0x73 + 100 / 10.
    let rails = BUCK_VOUT.map(|reg| board.pmic_reg(reg));
    assert_eq!(rails, [0x4f, 0x4f, 0xc6, 0x7d]);
    assert!(!board.adc_supply_enabled(), "LDO4 is off after the probes");
    assert!(!board.adc_clock_enabled(), "the Si549 output stays gated");
    assert_eq!(
        board.gpo(GPO_CFG_ADC),
        None,
        "no ADC reset line moves at open"
    );
}

#[test]
fn the_clock_generator_is_tested_only_when_asked_for() {
    let (device, sim) = open(Variant::DualAdcClockGen, OpenOptions::new());
    assert_eq!(device.fitted().adcs, [true, true]);
    assert!(!device.fitted().clock_generator);
    assert!(
        !sim.board()
            .trace()
            .iter()
            .any(|op| matches!(op, Op::Spi { bus: 2, .. })),
        "without lmk=1 the LMK04832 is not touched"
    );

    let (device, _) = open(
        Variant::DualAdcClockGen,
        OpenOptions::new().probe_clock_generator(true),
    );
    assert!(device.fitted().clock_generator, "the sim's LMK04832 passes");

    let (device, _) = open(
        Variant::SingleAdc,
        OpenOptions::new().probe_clock_generator(true),
    );
    assert!(
        !device.fitted().clock_generator,
        "an empty socket opens, without a clock generator"
    );
}

#[test]
fn a_dead_link_is_refused_having_read_only_the_status_word() {
    let sim = SimBus::new(SimBoard::new(Variant::SingleAdc).dead());
    assert!(matches!(
        Device::open(sim.clone(), OpenOptions::new()),
        Err(Error::DeadLink)
    ));
    assert_eq!(sim.board().trace().len(), 1);
}

#[test]
fn power_supplies_and_releases_the_adcs_and_power_down_holds_them_unsupplied() {
    let (mut device, sim) = open(Variant::SingleAdc, OpenOptions::new());
    device.set_power(true).expect("the ADCs power up");
    {
        let board = sim.board();
        assert!(board.adc_supply_enabled());
        assert_eq!(board.gpo(GPO_CFG_ADC), Some(0), "both ADCs out of reset");
        assert!(!board.adc_clock_enabled(), "power does not start the clock");
    }
    device.set_power(false).expect("the ADCs power down");
    let board = sim.board();
    assert!(!board.adc_supply_enabled());
    assert_eq!(
        board.gpo(GPO_CFG_ADC),
        Some(0b11),
        "both ADCs held in reset"
    );
}

#[test]
fn a_pga_gain_is_written_as_libusdrs_code_and_latched() {
    let (mut device, sim) = open(Variant::SingleAdc, OpenOptions::new());
    let gain = PgaGain::new(10).expect("within 0..=31");
    device.set_rx_pga_gain(gain).expect("the gain is set");
    let board = sim.board();
    assert_eq!(board.gpo(GPO_PGA_GAIN), Some(63 - 2 * 10));
    assert_eq!(
        board.gpo(GPO_PGA_LE),
        Some(0),
        "the latch is released again"
    );
}

#[test]
fn close_gates_the_clock_cuts_the_supply_and_powers_both_sockets_down() {
    let (mut device, sim) = open(Variant::DualAdcClockGen, OpenOptions::new());
    device.set_power(true).expect("the ADCs power up");
    device.close().expect("the board closes");
    let board = sim.board();
    assert!(!board.adc_clock_enabled());
    assert!(!board.adc_supply_enabled());
    for adc in 0..2 {
        assert_eq!(
            board.adc_reg(adc, ADC_PWR_CONTROL),
            Some(0xe0),
            "ADC {adc}: both channels and the chip powered down"
        );
    }
}
