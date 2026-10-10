//! rsl-usdr decodes the TMP114 the way libusdr does.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use rsl_usdr::Device;
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, SimBoard};
use sim_bus::SimBus;

/// libusdr's `/dm/sensor/temp`: a signed value in 1/256 °C, returned through a `u64`.
fn libusdr_celsius(millicelsius: i32) -> f32 {
    let mut oracle =
        Oracle::open(SimBoard::new(BoardRevision::Rev3).with_temperature(millicelsius))
            .expect("opens");
    let raw = oracle.get_uint(c"/dm/sensor/temp").expect("reads");
    let _ = oracle.close();
    #[expect(
        clippy::cast_possible_wrap,
        reason = "libusdr stores a signed value in the u64"
    )]
    let signed = raw as i64;
    #[expect(clippy::cast_precision_loss, reason = "sensor values are small")]
    let celsius = signed as f32 / 256.0;
    celsius
}

#[test]
fn temperatures_match_libusdr_across_the_sensor_range() {
    for millicelsius in [
        -40_000, -10_250, 0, 25_000, 79_500, 85_000, 109_992, 125_000,
    ] {
        let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3).with_temperature(millicelsius));
        let rust = Device::builder(sim)
            .thermal(rsl_usdr::ThermalPolicy::HardStopOnly)
            .open()
            .map(|mut device| device.temperature().expect("sensor reads"));
        let reference = libusdr_celsius(millicelsius);
        match rust {
            Ok(rust) => assert!(
                (rust - reference).abs() < 1e-3,
                "{millicelsius} m°C: rsl-usdr {rust}, libusdr {reference}"
            ),
            Err(rsl_usdr::Error::Overheated { .. }) => assert!(
                reference >= rsl_usdr::HARD_STOP_CELSIUS,
                "{millicelsius} m°C refused below the hard stop"
            ),
            Err(err) => panic!("{millicelsius} m°C: unexpected {err:?}"),
        }
    }
}
