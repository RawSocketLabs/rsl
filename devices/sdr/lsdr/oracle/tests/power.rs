//! ADC supply, reset and clock ordering through open, `/dm/power/en` and close
//! (`usdr_device_m2_d09_4_ad45_2_initialize`, `_lsdr_adc_activate`,
//! `usdr_device_m2_d09_4_ad45_2_destroy`).

use lsdr_oracle::Oracle;
use lsdr_sim::{I2cAddress, Op, SimBoard, Variant};

/// The TPS6594.
const PMIC: I2cAddress = I2cAddress::new(0, 0x48);
/// `LDO4_CTRL`.
const LDO4_CTRL: u8 = 0x20;
/// The Si549.
const SI549: I2cAddress = I2cAddress::new(0, 0x67);
/// Si549 `ODC`: bit 0 enables the output clock.
const SI549_ODC: u8 = 17;
/// GPO bank `IGPO_BANK_CFG_ADC`: bits 1:0 hold the ADCs in reset.
const GPO_CFG_ADC: u8 = 2;
/// `ADS42LBx9` `REG_PWR_CONTROL`.
const ADC_PWR_CONTROL: u8 = 0x08;

/// Whether `op` writes LDO4's control register with the enable bit as `on`.
fn is_ldo4_write(op: &Op, on: bool) -> bool {
    matches!(op, Op::I2c { addr: PMIC, write, .. }
        if write.len() == 2 && write[0] == LDO4_CTRL && (write[1] & 1 != 0) == on)
}

#[test]
fn open_probes_the_adcs_only_while_their_supply_is_on_and_leaves_it_off() {
    let oracle = Oracle::open(SimBoard::new(Variant::DualAdcClockGen), c"")
        .expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let board = oracle.close();
    let open = &board.trace()[..opened];

    let on = open
        .iter()
        .position(|op| is_ldo4_write(op, true))
        .expect("open switches LDO4 on for the probe");
    let off = open
        .iter()
        .rposition(|op| is_ldo4_write(op, true) || is_ldo4_write(op, false))
        .expect("and switches it again");
    assert!(is_ldo4_write(&open[off], false), "open ends with LDO4 off");
    let probes: Vec<usize> = open
        .iter()
        .enumerate()
        .filter(|(_, op)| matches!(op, Op::Spi { bus: 0 | 1, .. }))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(probes.len(), 2, "one presence read per ADC");
    assert!(
        probes.iter().all(|&at| on < at && at < off),
        "the ADCs are probed only while LDO4 supplies them"
    );
}

#[test]
fn power_up_supplies_the_adcs_then_pulses_and_releases_their_reset() {
    let mut oracle =
        Oracle::open(SimBoard::new(Variant::SingleAdc), c"").expect("libusdr opens the board");
    let opened = oracle.trace_len();
    oracle.set_power(true).expect("libusdr powers the ADCs up");
    let powered = oracle.trace_len();
    let board = oracle.close();
    let power_up = &board.trace()[opened..powered];

    let supply = power_up
        .iter()
        .position(|op| is_ldo4_write(op, true))
        .expect("power-up switches LDO4 on");
    let reset = power_up
        .iter()
        .position(|op| cfg_adc_write(op).is_some_and(|data| data & 0b11 == 0b11))
        .expect("power-up holds both ADCs in reset");
    let release = power_up
        .iter()
        .rposition(|op| cfg_adc_write(op).is_some())
        .expect("power-up drives the ADC reset lines");
    assert!(supply < reset, "reset is asserted on a supplied ADC");
    assert!(reset < release, "and released afterwards");
    assert_eq!(
        cfg_adc_write(&power_up[release]).map(|data| data & 0b11),
        Some(0),
        "both ADCs end out of reset"
    );
}

/// The data of a write to GPO bank `IGPO_BANK_CFG_ADC`, if `op` is one.
fn cfg_adc_write(op: &Op) -> Option<u32> {
    match op {
        Op::RegWrite { addr: 0, value } if value >> 24 == u32::from(GPO_CFG_ADC) => {
            Some(value & 0xff)
        }
        _ => None,
    }
}

#[test]
fn close_gates_the_clock_cuts_the_supply_and_powers_the_adcs_down() {
    let mut oracle = Oracle::open(SimBoard::new(Variant::DualAdcClockGen), c"")
        .expect("libusdr opens the board");
    oracle.set_power(true).expect("libusdr powers the ADCs up");
    let powered = oracle.trace_len();
    let board = oracle.close();
    let gates_the_clock = Op::I2c {
        addr: SI549,
        write: vec![SI549_ODC, 0],
        read: vec![],
    };
    assert!(
        board.trace()[powered..].contains(&gates_the_clock),
        "close turns the Si549 output off"
    );
    assert!(!board.adc_supply_enabled(), "LDO4 off");
    for adc in 0..2 {
        assert_eq!(
            board.adc_reg(adc, ADC_PWR_CONTROL),
            Some(0xe0),
            "ADC {adc}: both channels and the chip powered down"
        );
    }
}
