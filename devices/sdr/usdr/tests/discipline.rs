//! The oscillator discipline loop composed with the board, as the module docs describe:
//! the clock-measurement word in, the oscillator DAC out, on a simulated board with the
//! `PCIe` front end's DAC. The oracle's `gpsdo_parity` test compares the same composition
//! with libusdr's tool.

mod common {
    pub(crate) mod sim_bus;
}

use std::num::NonZeroU32;
use std::time::Duration;

use common::sim_bus::SimBus;
use rsl_usdr::Device;
use rsl_usdr::discipline::{OscillatorDiscipline, Outcome, PpsMeasurement};
use rsl_usdr_sim::{BoardRevision, FrontEnd, Op, SimBoard};

/// The frequency the counter should see: the tool's default sample rate.
const EXPECTED: u32 = 4_000_000;
/// GPI word 24's register.
const MEASUREMENT_REGISTER: u32 = 22;

/// A powered board with the `PCIe` front end, its DAC included.
fn device() -> (SimBus, Device) {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3).with_front_end(FrontEnd::Pciefe));
    let device = Device::with_bus(sim.clone()).expect("powers up");
    (sim, device)
}

/// Loads `word` as the next measurement, then reads it through the device.
fn read(sim: &SimBus, device: &mut Device, word: u32) -> PpsMeasurement {
    sim.board().set_clock_measurement(word);
    PpsMeasurement::from_word(device.clock_measurement().expect("reads"))
}

/// Each correction's code reaches the DAC, and a poll without a new pulse reads the
/// measurement and does nothing else.
#[test]
fn corrections_reach_the_oscillator_dac_and_idle_polls_only_read() {
    let (sim, mut device) = device();
    let expected = NonZeroU32::new(EXPECTED).expect("nonzero");
    let first = read(&sim, &mut device, EXPECTED);
    let mut discipline = OscillatorDiscipline::new(expected, first, Duration::ZERO);

    let mut writes = 0;
    for poll in 1..=40_u32 {
        let pulse = poll / 10;
        let word = pulse << 28 | (EXPECTED + 25);
        let start = sim.board().trace().len();
        let measurement = read(&sim, &mut device, word);
        let now = Duration::from_millis(100 * u64::from(poll));
        let step = discipline.measure(measurement, now);
        if let Some(code) = step.dac_write() {
            device.set_oscillator_dac(code).expect("writes the DAC");
            discipline.dac_written(code);
            writes += 1;
            assert_eq!(sim.board().front_end_dac_code(), Some(code));
        } else {
            let ops = sim.board().trace()[start..].to_vec();
            assert_eq!(
                ops,
                [Op::RegRead {
                    addr: MEASUREMENT_REGISTER,
                    value: word,
                }],
                "poll {poll}: {:?}",
                step.outcome
            );
        }
        if poll % 10 != 0 {
            assert_eq!(step.outcome, Outcome::NoPulse, "poll {poll}");
        }
    }
    assert_eq!(writes, 4, "one correction per pulse");
    assert!(
        discipline.dac_code() < 32767,
        "a count above the expected frequency lowers the code"
    );
}
