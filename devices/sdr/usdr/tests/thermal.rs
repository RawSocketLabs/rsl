//! The thermal policy on the simulated board.

mod common {
    pub(crate) mod sim_bus;
}

use std::time::Duration;

use common::sim_bus::SimBus;
use rsl_usdr::{Device, Error, HARD_STOP_CELSIUS, ThermalLimits, ThermalPolicy};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};

/// GPO bank enabling the RF booster: set only once the board powers up.
const GPO_BOOSTER: u8 = 7;
/// I2C address of the PMIC: touched only once the board powers up.
const PMIC_ADDR: u16 = 0x60;

/// A revision-3 board at `celsius`.
fn board_at(celsius: f32) -> SimBus {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "test temperatures are small"
    )]
    let millicelsius = (celsius * 1000.0) as i32;
    SimBus::new(SimBoard::new(BoardRevision::Rev3).with_temperature(millicelsius))
}

/// Whether anything beyond identification reached the board.
fn powered_anything(sim: &SimBus) -> bool {
    let board = sim.board();
    board.gpo(GPO_BOOSTER).is_some()
        || board
            .trace()
            .iter()
            .any(|op| matches!(op, Op::I2c { addr, .. } if addr.addr == PMIC_ADDR))
}

#[test]
fn reads_the_board_temperature() {
    let sim = board_at(41.5);
    let mut device = Device::with_bus(sim.clone()).expect("a cool board opens");
    assert!((device.temperature().expect("sensor reads") - 41.5).abs() < 0.01);
}

#[test]
fn reads_sub_zero_temperatures_as_negative() {
    // The FFI wrapper read libusdr's signed value as u64, turning these into "too hot".
    let sim = board_at(-10.25);
    let mut device = Device::with_bus(sim.clone()).expect("a cold board opens");
    assert!((device.temperature().expect("sensor reads") + 10.25).abs() < 0.01);
}

#[test]
fn within_spec_refuses_a_hot_board_before_powering_anything() {
    let sim = board_at(82.0);
    let err = Device::with_bus(sim.clone()).expect_err("82 °C is over the 80 °C start limit");
    assert!(
        matches!(err, Error::Overheated { limit, .. } if (limit - 80.0).abs() < f32::EPSILON),
        "{err:?}"
    );
    assert!(
        !powered_anything(&sim),
        "no rail or RF power before the check"
    );
}

#[test]
fn beyond_spec_starts_where_within_spec_refuses() {
    let sim = board_at(82.0);
    Device::builder(sim.clone())
        .thermal(ThermalPolicy::BeyondSpec)
        .open()
        .expect("82 °C is under 95 °C");
}

#[test]
fn hard_stop_only_still_refuses_at_the_hard_stop() {
    let sim = board_at(HARD_STOP_CELSIUS + 1.0);
    let err = Device::builder(sim.clone())
        .thermal(ThermalPolicy::HardStopOnly)
        .open()
        .expect_err("past the hard stop");
    assert!(matches!(err, Error::Overheated { .. }), "{err:?}");
    assert!(!powered_anything(&sim));
}

#[test]
fn custom_limits_apply() {
    let limits = ThermalLimits::new(60.0, 65.0, 50.0).expect("ordered limits");
    let sim = board_at(62.0);
    let err = Device::builder(sim)
        .thermal(ThermalPolicy::Custom(limits))
        .open()
        .expect_err("62 °C is over 60 °C");
    assert!(matches!(err, Error::Overheated { .. }), "{err:?}");
}

#[test]
fn waits_for_a_hot_board_to_cool_below_the_resume_limit() {
    // 90 °C cooling at 0.5 °C/s: below 70 °C after 40 s.
    let sim = SimBus::new(
        SimBoard::new(BoardRevision::Rev3)
            .with_temperature(90_000)
            .with_cooling(500),
    );
    let mut readings = Vec::new();
    Device::builder(sim.clone())
        .wait_for_temperature(Duration::from_secs(120), |celsius| readings.push(celsius))
        .open()
        .expect("the board cools within the timeout");

    assert!(readings.len() > 2, "polled while cooling: {readings:?}");
    assert!(
        readings.windows(2).all(|pair| pair[1] <= pair[0]),
        "readings fall: {readings:?}"
    );
    assert!(
        *readings.last().unwrap() < 70.0,
        "waited for the resume limit, not just the start limit"
    );
    assert!(sim.board().now_us() >= 40_000_000, "waited in virtual time");
}

#[test]
fn gives_up_when_the_board_does_not_cool_in_time() {
    let sim = board_at(90.0);
    let err = Device::builder(sim.clone())
        .wait_for_temperature(Duration::from_secs(30), |_| {})
        .open()
        .expect_err("no cooling");
    assert!(matches!(err, Error::Overheated { .. }), "{err:?}");
    assert!(!powered_anything(&sim));
    assert!(sim.board().now_us() >= 30_000_000);
}

#[test]
fn an_open_device_can_wait_to_cool() {
    let sim = board_at(40.0);
    let mut device = Device::with_bus(sim.clone()).expect("opens");
    sim.board().set_temperature(75_000);
    device
        .wait_for_temperature(Duration::from_secs(10), |_| {})
        .expect_err("75 °C does not cool");
    sim.board().set_temperature(65_000);
    device
        .wait_for_temperature(Duration::from_secs(10), |_| {})
        .expect("65 °C is below the 70 °C resume limit");
}

#[test]
fn hard_stop_only_refuses_at_exactly_the_hard_stop() {
    let sim = board_at(HARD_STOP_CELSIUS);
    let err = Device::builder(sim.clone())
        .thermal(ThermalPolicy::HardStopOnly)
        .open()
        .expect_err("at the hard stop");
    assert!(matches!(err, Error::Overheated { .. }), "{err:?}");
}

#[test]
fn opens_between_the_resume_and_start_limits() {
    // 75 °C: above WithinSpec's 70 °C resume limit but below its 80 °C start limit.
    Device::with_bus(board_at(75.0)).expect("hysteresis applies only after a refusal");
}

#[test]
fn the_wait_never_overshoots_its_timeout() {
    let sim = board_at(90.0);
    let _ = Device::builder(sim.clone())
        .wait_for_temperature(Duration::from_secs(31), |_| {})
        .open();
    assert_eq!(sim.board().now_us(), 31_000_000);
}

#[test]
fn rev2_boards_are_gated_on_their_sensor_too() {
    let hot = SimBus::new(SimBoard::new(BoardRevision::Rev2).with_temperature(82_000));
    let err = Device::with_bus(hot.clone()).expect_err("82 °C is over the start limit");
    assert!(matches!(err, Error::Overheated { .. }), "{err:?}");

    let blind = SimBus::new(SimBoard::new(BoardRevision::Rev2).without_temperature_sensor());
    let err = Device::with_bus(blind.clone()).expect_err("no sensor, no thermal decision");
    assert!(
        matches!(err, Error::ChipId { chip: "TMP114", .. }),
        "{err:?}"
    );
    assert!(!powered_anything(&blind), "fails closed before any power");
}
