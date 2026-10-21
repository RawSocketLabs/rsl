//! Opening, powering up and closing an XSDR and an XTRX on the simulated board: the board
//! state libusdr's sequence leaves (`device/m2_lm7_1/xsdr_ctrl.c`), and the refusals
//! before it. Operation parity with libusdr itself is `oracle/tests/open_parity.rs`.

mod common;

use common::sim_bus::SimBus;
use xsdr::{Device, Error, FrontEndKind, Variant};
use xsdr_sim::{Op, SimBoard, Variant as SimVariant};

/// GPO bank `IGPO_LMS_PWR`.
const GPO_LMS_PWR: u8 = 0;
/// GPO bank `IGPO_CLK_CFG`.
const GPO_CLK_CFG: u8 = 1;
/// GPO bank `IGPO_LDOLMS_EN`.
const GPO_LDOLMS_EN: u8 = 17;
/// GPO bank `IGPO_LED`.
const GPO_LED: u8 = 18;
/// The LP8758's `BUCKn_CTRL1` registers, channels 0 to 3.
const BUCK_CONTROLS: [u8; 4] = [0x02, 0x04, 0x06, 0x08];

/// An XSDR on a fresh board, and the bus to inspect it through.
fn open_xsdr(board: SimBoard) -> (Result<Device, Error>, SimBus) {
    let sim = SimBus::new(board);
    (Device::open(sim.clone()), sim)
}

#[test]
fn opening_an_xsdr_turns_every_converter_on_and_leaves_the_rfic_unpowered() {
    let (device, sim) = open_xsdr(SimBoard::new(SimVariant::Xsdr));
    let device = device.expect("an XSDR opens");
    assert_eq!(device.hwid() >> 16 & 0xff, 0x30);

    let board = sim.board();
    for reg in BUCK_CONTROLS {
        // `lp8758_vout_ctrl(.., en = 1, forcepwm = 1)`: PMIC_CH_ENABLE | 2.
        assert_eq!(
            board.fpga_pmic_reg(reg),
            0x8a,
            "converter {reg:#04x} on, forced PWM"
        );
    }
    // libusdr's `s_get_vout` for its requests: 1480 mV -> 0x9D + 80 / 20, 1280 mV ->
    // 0x18 + 550 / 5, 1850 mV -> 0x9D + 450 / 20.
    assert_eq!(
        board.fpga_pmic_reg(0x0c),
        0xa1,
        "channel 1 at libusdr's 1.48 V code"
    );
    assert_eq!(
        board.fpga_pmic_reg(0x0e),
        0x86,
        "channel 2 at libusdr's 1.28 V code"
    );
    assert_eq!(
        board.fpga_pmic_reg(0x10),
        0xb3,
        "channel 3 at libusdr's 1.85 V code"
    );
    assert_eq!(
        board.gpo(GPO_LMS_PWR),
        Some(0),
        "the LMS7002M stays unpowered in reset"
    );
    assert_eq!(
        board.gpo(GPO_CLK_CFG),
        Some(1),
        "the internal reference is selected"
    );
    assert_eq!(
        board.gpo(GPO_LDOLMS_EN),
        None,
        "its LDOs are not touched at open"
    );
    assert!(
        !board.trace().iter().any(|op| matches!(op, Op::Spi { .. })),
        "nothing talks to the unpowered LMS7002M"
    );
}

#[test]
fn power_up_releases_the_lms7002m_reset_before_any_spi_and_keeps_its_afe_down() {
    let (device, sim) = open_xsdr(SimBoard::new(SimVariant::Xsdr));
    // Kept open: a dropped device closes the board.
    let mut device = device.expect("an XSDR opens");
    device.power_up().expect("its RFIC powers up");

    let board = sim.board();
    let trace = board.trace();
    let running = Op::RegWrite {
        addr: 0,
        value: 0b1001,
    };
    let released = trace.iter().position(|op| *op == running);
    let first_spi = trace.iter().position(|op| matches!(op, Op::Spi { .. }));
    assert!(
        matches!((released, first_spi), (Some(r), Some(s)) if r < s),
        "LDOEN | NRESET is written before the first SPI exchange"
    );
    assert_eq!(
        board.gpo(GPO_LDOLMS_EN),
        Some(1),
        "the external LDOs are on"
    );
    assert_eq!(board.gpo(GPO_LMS_PWR), Some(0b1001), "the LMS7002M runs");
    assert_eq!(
        board.lms7_reg(0x0082) & 0b11_1111,
        0b11_1110,
        "AFE powered down, its global enable off (\"Disable RF\")"
    );
}

#[test]
fn close_powers_the_rfic_down_and_holds_it_in_reset() {
    let (device, sim) = open_xsdr(SimBoard::new(SimVariant::Xsdr));
    let mut device = device.expect("an XSDR opens");
    device.power_up().expect("its RFIC powers up");
    device.close().expect("it closes");

    let board = sim.board();
    assert_eq!(board.gpo(GPO_LMS_PWR), Some(0), "the LMS7002M is in reset");
    assert_eq!(board.gpo(GPO_LDOLMS_EN), Some(0), "its LDOs are off");
    assert_eq!(board.gpo(GPO_LED), Some(0), "the LED is off");
    assert_eq!(
        board.lms7_reg(0x0082) & 0b10_0001,
        0b10_0000,
        "the whole AFE powered down (PD_AFE), its global enable off"
    );
    assert_eq!(board.lms7_reg(0x0124), 0, "lms7002m_destroy clears EN_DIR");
}

#[test]
fn an_unknown_device_id_is_refused_having_read_only_the_hwid() {
    let board = SimBoard::new(SimVariant::Xsdr).with_hwid(0x0032_0000);
    let (device, sim) = open_xsdr(board);
    assert!(matches!(device, Err(Error::UnknownDevice(0x32))));
    assert!(
        sim.board()
            .trace()
            .iter()
            .all(|op| matches!(op, Op::RegRead { addr: 0x13, .. })),
        "nothing is driven on an unknown board"
    );
}

#[test]
fn an_xtrx_opens_without_a_front_end_and_trims_its_dac() {
    for (board, dac) in [
        (SimBoard::new(SimVariant::Xtrx), Some(0xa8f5)),
        (SimBoard::new(SimVariant::Xtrx).without_trim_dac(), None),
    ] {
        let (device, sim) = open_xsdr(board);
        let mut device = device.expect("an XTRX opens");
        assert_eq!(device.variant(), Variant::Xtrx);
        assert_eq!(device.front_end(), None, "no connector, no probe");
        assert_eq!(sim.board().trim_dac_code(), dac, "mid-range at open");
        assert!(
            sim.board()
                .trace()
                .iter()
                .all(|op| !matches!(op, Op::RegWrite { addr: 3, .. })),
            "the connector's GPIO core is never written"
        );

        // `xsdr_trim_dac_vctcxo`, which libusdr's `dac_vctcxo` cannot reach on the XTRX
        // (it looks up a front end its open never set).
        let before = sim.board().trace().len();
        device
            .set_oscillator_dac(0x1234)
            .expect("the trim DAC answers");
        let board = sim.board();
        let written = &board.trace()[before..];
        let expected = if dac.is_some() {
            [0x08, 0x12, 0x34].to_vec()
        } else {
            // The MCP4725's fast mode: the code's top 12 bits.
            [0x01, 0x23].to_vec()
        };
        assert!(
            matches!(written, [Op::I2c { write, .. }] if *write == expected),
            "{written:?}"
        );
        if dac.is_some() {
            assert_eq!(board.trim_dac_code(), Some(0x1234));
        }
    }
}

#[test]
fn an_xsdr_without_an_oscillator_dac_refuses_the_trim_untouched() {
    let sim = SimBus::new(SimBoard::new(SimVariant::Xsdr));
    let mut device =
        Device::open_with(sim.clone(), &xsdr::FrontEndSelection::Auto).expect("an XSDR opens");
    let before = sim.board().trace().len();
    // The empty connector's breakout has no DAC.
    assert!(matches!(
        device.set_oscillator_dac(1),
        Err(Error::FrontEnd(_))
    ));
    assert_eq!(sim.board().trace().len(), before, "nothing driven");
}

#[test]
fn members_not_yet_ported_are_identified_and_refused_before_anything_is_driven() {
    for (sim, variant) in [
        (SimVariant::Ssdr, Variant::Ssdr),
        (SimVariant::SsdrPro, Variant::SsdrPro),
    ] {
        let (device, bus) = open_xsdr(SimBoard::new(sim));
        assert!(
            matches!(device, Err(Error::UnsupportedVariant(v)) if v == variant),
            "{variant:?} is named in the refusal"
        );
        assert!(
            bus.board()
                .trace()
                .iter()
                .all(|op| matches!(op, Op::RegRead { .. })),
            "{variant:?}: nothing is driven"
        );
    }
}

#[test]
fn a_pmic_revision_libusdr_rejects_opens_but_refuses_power_up_unpowered() {
    let board = SimBoard::new(SimVariant::Xsdr).with_fpga_pmic_revision(0xe101);
    let (device, sim) = open_xsdr(board);
    let mut device = device.expect("libusdr opens it too, logging the revision");
    let opened = sim.board().trace().len();

    let refusal = device.power_up();
    assert!(matches!(
        refusal,
        Err(Error::ChipId {
            chip: "LP8758",
            expected: 0xe001,
            found: 0xe101,
            ..
        })
    ));
    let board = sim.board();
    let refusal = &board.trace()[opened..];
    assert!(
        matches!(
            refusal,
            [
                Op::RegWrite { addr: 0, value: 0 },
                Op::Sleep { us: 5000 },
                Op::I2c { .. }
            ]
        ),
        "held in reset, then the sensor's ID read for libusdr's log: {refusal:?}"
    );
}

#[test]
fn open_probes_the_empty_connector_and_reports_a_breakout_without_a_dac() {
    let (device, sim) = open_xsdr(SimBoard::new(SimVariant::Xsdr));
    let device = device.expect("an XSDR opens");
    assert_eq!(
        device.front_end(),
        Some(FrontEndKind::Exm2pe { dac: false }),
        "libusdr cannot tell a DAC-less breakout from an empty connector"
    );

    let board = sim.board();
    let trace = board.trace();
    let probe_start = trace
        .iter()
        .position(|op| matches!(op, Op::RegWrite { addr: 3, .. }))
        .expect("open writes the connector's GPIO core");
    assert!(
        trace[probe_start..].iter().all(|op| match op {
            Op::RegWrite { addr, .. } => *addr == 3,
            Op::I2c { addr, .. } => addr.bus == 1,
            Op::Sleep { .. } => true,
            Op::RegRead { .. } | Op::Spi { .. } => false,
        }),
        "the probe stays on the connector: its GPIO core and I2C bus 1"
    );
    assert!(
        matches!(
            trace.last(),
            Some(Op::RegWrite {
                addr: 3,
                value: 0x0030_0000
            })
        ),
        "the breakout's oscillator and GPS are left off"
    );
}

#[test]
fn a_dropped_device_closes_the_board_as_close_does() {
    let closed = {
        let (device, sim) = open_xsdr(SimBoard::new(SimVariant::Xsdr));
        let mut device = device.expect("an XSDR opens");
        device.power_up().expect("its RFIC powers up");
        let opened = sim.board().trace().len();
        device.close().expect("it closes");
        sim.board().trace()[opened..].to_vec()
    };
    let (device, sim) = open_xsdr(SimBoard::new(SimVariant::Xsdr));
    let mut device = device.expect("an XSDR opens");
    device.power_up().expect("its RFIC powers up");
    let opened = sim.board().trace().len();
    drop(device);
    assert_eq!(
        sim.board().trace()[opened..],
        closed,
        "the same close, once"
    );
}
