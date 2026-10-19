//! lsdr's front-end bring-up at open against libusdr's, operation for operation, with
//! fe1005000 on the connector (`lsdr-sim`'s model): the expander check over the
//! SPI-extension core, the path tuned to 900 MHz (both LOs, the band switches) and the
//! status read for the log; then its debug-command controls. Each scenario compares the
//! whole session, open to close.
//!
//! No divergence is whitelisted.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;

use lsdr::{Device, Fe1005000Setting, FrontEndKind, OpenOptions};
use lsdr_oracle::Oracle;
use lsdr_sim::{Op, SimBoard, Variant};
use sim_bus::SimBus;

/// The expander's output registers, `OCR1` and `OCR2`.
const OCR: [u8; 2] = [0x04, 0x06];

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: lsdr diverges from libusdr at op {at}: libusdr {}, lsdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// libusdr's and lsdr's sessions on `variant` with fe1005000 attached (open with
/// `connection` or `options`, ADC power on, close), compared whole; returns the boards as
/// each left them and what lsdr's probe brought up.
fn assert_sessions_match(
    variant: Variant,
    connection: &CStr,
    options: OpenOptions,
) -> (SimBoard, SimBoard, Option<FrontEndKind>) {
    let board = || SimBoard::new(variant).with_fe1005000();
    let mut oracle = Oracle::open(board(), connection).expect("libusdr opens the board");
    oracle.set_power(true).expect("libusdr powers the ADCs");
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::open(sim.clone(), options).expect("lsdr opens the board");
    let kind = device.front_end();
    device.set_power(true).expect("lsdr powers the ADCs");
    device.close().expect("lsdr closes the board");
    let actual = std::mem::replace(&mut *sim.board(), SimBoard::new(variant));

    assert_same_ops(
        reference.trace(),
        actual.trace(),
        &format!("{variant:?} {connection:?}"),
    );
    (reference, actual, kind)
}

#[test]
fn fe1005000_comes_up_like_libusdr_on_every_fit() {
    for (variant, connection, options) in [
        (Variant::SingleAdc, c"", OpenOptions::new()),
        (Variant::DualAdcClockGen, c"", OpenOptions::new()),
        (
            Variant::DualAdcClockGen,
            c"lmk=1",
            OpenOptions::new().probe_clock_generator(true),
        ),
    ] {
        let (reference, actual, kind) = assert_sessions_match(variant, connection, options);
        assert_eq!(kind, Some(FrontEndKind::Fe1005000), "{variant:?}");
        // Band 1 with the 2250 MHz IF: OCR1 0xF3, OCR2 0x07; LO1 at 3150 MHz (N 157,
        // fraction 2047), LO2 at 2562.5 MHz (N 256, fraction 1023).
        for board in [&reference, &actual] {
            assert_eq!(
                OCR.map(|reg| board.fe1005000_expander_reg(reg)),
                [Some(0xf3), Some(0x07)]
            );
            assert_eq!(board.fe1005000_lo_reg(0, 0), Some(157 << 15 | 2047 << 3));
            assert_eq!(board.fe1005000_lo_reg(1, 0), Some(256 << 15 | 1023 << 3));
            assert_eq!(board.fe1005000_attenuation(), None, "untouched at open");
        }
    }
}

/// One fe1005000 command: the `FE_*` address and data libusdr takes, and lsdr's setting.
type Command = (u8, u32, Fe1005000Setting);

/// fe1005000's commands (`/debug/hw/fe_100_5000_cmd/0/reg`) applied one by one after
/// open, each compared with libusdr's operations and outcome: every band and both IF
/// filters, the edges of the tuning range (refused before any access past them), the
/// attenuator and the preselector, whose two bits libusdr masks.
#[test]
fn fe1005000_commands_match_libusdr() {
    let commands: [Command; 14] = [
        (0, 100_000, Fe1005000Setting::FrequencyKhz(100_000)),
        (0, 2_000_000, Fe1005000Setting::FrequencyKhz(2_000_000)),
        (0, 3_000_000, Fe1005000Setting::FrequencyKhz(3_000_000)),
        (0, 4_000_000, Fe1005000Setting::FrequencyKhz(4_000_000)),
        (0, 4_800_000, Fe1005000Setting::FrequencyKhz(4_800_000)),
        (0, 6_000_000, Fe1005000Setting::FrequencyKhz(6_000_000)),
        (0, 6_000_001, Fe1005000Setting::FrequencyKhz(6_000_001)),
        (0, 19_999, Fe1005000Setting::FrequencyKhz(19_999)),
        (1, 0x55, Fe1005000Setting::Attenuation(0x55)),
        (1, 127, Fe1005000Setting::Attenuation(127)),
        (2, 1, Fe1005000Setting::Preselector(1)),
        (2, 3, Fe1005000Setting::Preselector(3)),
        (2, 6, Fe1005000Setting::Preselector(6)),
        (0, 20_000, Fe1005000Setting::FrequencyKhz(20_000)),
    ];
    let board = || SimBoard::new(Variant::SingleAdc).with_fe1005000();
    let mut oracle = Oracle::open(board(), c"").expect("libusdr opens the board");
    let sim = SimBus::new(board());
    let mut device = Device::open(sim.clone(), OpenOptions::new()).expect("lsdr opens the board");
    let mut ends = (oracle.trace_len(), sim.board().trace().len());
    let mut steps = Vec::new();
    for (command, value, setting) in commands {
        let expected = oracle.fe1005000_command(command, value).is_ok();
        let actual = device.configure_fe1005000(setting);
        assert_eq!(actual.is_ok(), expected, "{setting:?}: {actual:?}");
        let now = (oracle.trace_len(), sim.board().trace().len());
        steps.push((setting, ends, now));
        ends = now;
    }
    device.close().expect("lsdr closes the board");
    let reference = oracle.close();
    let actual = sim.board();
    for (setting, (from_c, from_rust), (to_c, to_rust)) in steps {
        assert_same_ops(
            &reference.trace()[from_c..to_c],
            &actual.trace()[from_rust..to_rust],
            &format!("{setting:?}"),
        );
    }
    assert_same_ops(reference.trace(), actual.trace(), "whole session");
    assert_eq!(actual.fe1005000_attenuation(), Some(127));
}
