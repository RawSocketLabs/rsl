//! Opening, powering up and closing a LimeSDR-mini on the simulated board: the board state
//! libusdr's `limesdr_init` leaves, and the refusals before it. Operation parity with
//! libusdr itself is `oracle/tests/open_parity.rs`.

mod common;

use std::time::Duration;

use common::sim_port::SimPort;
use limesdr::control::{BusError, ControlPort};
use limesdr::lms64c::FRAME_LEN;
use limesdr::{Access, Device, Error, ExchangeError};
use limesdr_sim::{Op, SimBoard};

/// Stream control, gateware register 0x000A.
const STREAM_CONTROL: u16 = 0x000a;
/// The LMS7002M's AFE register, which its reset programme powers down.
const LMS_AFE: u16 = 0x0082;

/// Whether `op` touched the LMS7002M.
fn is_lms7(op: &Op) -> bool {
    matches!(
        op,
        Op::Lms7Reset | Op::Lms7Read { .. } | Op::Lms7Write { .. }
    )
}

#[test]
fn opening_programmes_the_lms7002m_and_leaves_streaming_stopped() {
    let sim = SimPort::new(SimBoard::new().with_fpga_reg(STREAM_CONTROL, 0xffff));
    let device = Device::open(sim.clone()).expect("limesdr opens the LimeSDR-mini");
    assert_eq!(device.reference_hz(), 40_000_000);

    let board = sim.board();
    assert_eq!(
        board.fpga_reg(STREAM_CONTROL),
        0xfffc,
        "RX_EN and TX_EN cleared, the rest kept"
    );
    assert_eq!(board.lms7_reg(LMS_AFE), 0x003e, "the AFE powered down");
    let first_lms7 = board.trace().iter().position(is_lms7);
    assert_eq!(
        first_lms7.map(|at| &board.trace()[at]),
        Some(&Op::Lms7Reset),
        "the reset precedes every LMS7002M access"
    );
}

#[test]
fn power_up_and_close_touch_nothing() {
    let sim = SimPort::new(SimBoard::new());
    let mut device = Device::open(sim.clone()).expect("limesdr opens the LimeSDR-mini");
    let opened = sim.board().trace().len();
    device
        .power_up()
        .expect("power-up does nothing, successfully");
    device.close().expect("close does nothing, successfully");
    assert_eq!(sim.board().trace().len(), opened);
}

#[test]
fn a_40_mhz_reference_a_few_ppm_slow_opens() {
    // libusdr reads past its reference table for this measurement.
    let sim = SimPort::new(SimBoard::new().with_reference_hz(39_999_960));
    let device = Device::open(sim).expect("a slightly slow reference is 40 MHz");
    assert_eq!(device.reference_hz(), 40_000_000);
}

#[test]
fn a_detector_that_never_finishes_is_refused_after_ten_polls_50_ms_apart() {
    let sim = SimPort::new(SimBoard::new().with_detector_delay_us(u64::MAX));
    assert!(matches!(
        Device::open(sim.clone()),
        Err(Error::DetectorStalled)
    ));
    let board = sim.board();
    assert_eq!(board.now_us(), 10 * 50_000, "ten waits");
    assert!(
        !board.trace().iter().any(is_lms7),
        "refused before the LMS7002M"
    );
}

#[test]
fn a_detector_taking_its_real_gate_time_is_waited_for() {
    let sim = SimPort::new(SimBoard::new().with_detector_delay_us(limesdr_sim::DETECTOR_GATE_US));
    let device = Device::open(sim.clone()).expect("the measurement completes within the polls");
    assert_eq!(device.reference_hz(), 40_000_000);
    assert_eq!(sim.board().now_us(), 4 * 50_000, "four waits cover 168 ms");
}

#[test]
fn an_lms7002m_other_than_mask_1_is_refused_before_anything_is_written_to_it() {
    let sim = SimPort::new(SimBoard::new().with_lms7_mask(0));
    assert!(matches!(
        Device::open(sim.clone()),
        Err(Error::ChipId {
            chip: "LMS7002M",
            expected: 0x3841,
            found: 0x3840,
            ..
        })
    ));
    assert!(
        !sim.board()
            .trace()
            .iter()
            .any(|op| matches!(op, Op::Lms7Write { .. })),
        "nothing written to the chip"
    );
}

/// A control channel that fails every exchange with `error`, or answers every request
/// with its own frame and `status`.
struct Failing(Result<u8, fn() -> BusError>);

impl ControlPort for Failing {
    fn exchange(&mut self, request: &[u8; FRAME_LEN]) -> Result<[u8; FRAME_LEN], BusError> {
        let status = self.0.map_err(|error| error())?;
        let mut reply = *request;
        reply[1] = status;
        Ok(reply)
    }

    fn sleep(&mut self, _: Duration) {}
}

#[test]
fn a_failed_exchange_names_the_register_it_struck() {
    let refused = Device::open(Failing(Ok(5))).unwrap_err();
    assert!(
        matches!(
            refused,
            Error::Bus {
                chip: "FPGA",
                access: Access::Write,
                reg: 0x0061,
                source: ExchangeError::Refused(5),
                ..
            }
        ),
        "the first write, refused by the firmware: {refused:?}"
    );
    let lost = Device::open(Failing(Err(|| BusError::Disconnected))).unwrap_err();
    assert!(
        matches!(
            lost,
            Error::Bus {
                source: ExchangeError::Bus(BusError::Disconnected),
                ..
            }
        ),
        "{lost:?}"
    );
}
