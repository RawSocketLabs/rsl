//! What libusdr checks before it accepts a LimeSDR-mini: the transport's device ID, the
//! reference clock `limesdr_detect_refclk` measures, and the LMS7002M version word.

use rsl_limesdr_oracle::Oracle;
use rsl_limesdr_sim::{Op, SimBoard};

/// `EIO`: `limesdr_init`'s refusal of an LMS7002M version word other than 0x3841.
const EIO: i32 = 5;
/// `EFAULT`: `limesdr_detect_refclk`'s refusal of an unrecognised reference.
const EFAULT: i32 = 14;
/// `ENODEV`: what `usdr_dmd_create_string` reports for any device init failure, since
/// `lowlevel_create` returns the last transport plugin's result (`PCIe`: nothing found).
const ENODEV: i32 = 19;

/// Whether `op` touched the LMS7002M.
fn is_lms7(op: &Op) -> bool {
    matches!(
        op,
        Op::Lms7Reset | Op::Lms7Read { .. } | Op::Lms7Write { .. }
    )
}

#[test]
fn identity_comes_from_the_device_id_not_the_board() {
    let mut oracle = Oracle::open(SimBoard::new()).expect("libusdr opens the LimeSDR-mini");
    let before = oracle.trace_len();
    let revision = oracle
        .get_uint(c"/dm/revision")
        .expect("revision is readable");
    assert_eq!(
        oracle.trace_len(),
        before,
        "/dm/revision is emulated, not read"
    );
    let board = oracle.close();
    assert!(
        !board
            .trace()
            .iter()
            .any(|op| matches!(op, Op::FpgaRead { addr: 0..=3, .. })),
        "open never reads the gateware identity registers"
    );

    let other = SimBoard::new()
        .with_fpga_reg(0x0000, 0x1234)
        .with_fpga_reg(0x0001, 0x5678);
    let mut oracle = Oracle::open(other).expect("libusdr opens it regardless of identity");
    assert_eq!(
        oracle.get_uint(c"/dm/revision"),
        Ok(revision),
        "the reported revision ignores the gateware"
    );
}

#[test]
fn only_an_lms7002m_reading_0x3841_is_accepted() {
    let refused = Oracle::open(SimBoard::new().with_lms7_mask(0))
        .err()
        .expect("libusdr refuses metal mask 0");
    assert_eq!(refused.device_errno, -EIO, "limesdr_init's version check");
    assert_eq!(
        refused.errno, -ENODEV,
        "libusdr's API masks the device's errno"
    );
    let lms7: Vec<&Op> = refused
        .board
        .trace()
        .iter()
        .filter(|op| is_lms7(op))
        .collect();
    assert!(
        matches!(
            lms7.as_slice(),
            [Op::Lms7Reset, Op::Lms7Read { addr: 0x002f, .. }]
        ),
        "refused at the first version read, before lms7002m_create writes anything: {lms7:?}"
    );
}

#[test]
fn the_lms7002m_is_reset_after_the_reference_is_measured_and_before_any_access() {
    let board = Oracle::open(SimBoard::new())
        .expect("libusdr opens the LimeSDR-mini")
        .close();
    let trace = board.trace();
    let first_lms7 = trace
        .iter()
        .position(is_lms7)
        .expect("init talks to the LMS7002M");
    assert_eq!(
        trace[first_lms7],
        Op::Lms7Reset,
        "reset precedes every LMS7002M access"
    );
    let count_read = trace
        .iter()
        .position(|op| matches!(op, Op::FpgaRead { addr: 0x0073, .. }))
        .expect("the measured count is read");
    assert!(count_read < first_lms7, "the reference is measured first");
}

/// libusdr's reference table entries and neighbours, with the outcome on an ideal counter.
/// Characterization of two bugs in `limesdr_detect_refclk`, which a port must reproduce or
/// deliberately fix: its `ABS` macro evaluates `clkTbl[i++]` twice, so the nearest-entry
/// walk skips entries, and it subtracts entries unsigned, so a count below an entry wraps.
/// Of the table only 40 MHz opens (10 MHz is taken for 30.72 MHz and refused), along with
/// counts up to 1 MHz above it. Left out on purpose: 30.72 MHz and counts just below
/// 40 MHz (39 999 999 Hz) walk past the five-entry table and read `clkTbl[5]`, undefined
/// behaviour whose outcome depends on the stack. Exactly 52 MHz would too; the 52 MHz row is
/// deterministic only because the detector's integer count truncates it to 51 999 998 Hz
/// (`rsl-limesdr-sim`'s `reference_count`), a dependency a sim change must keep.
#[test]
fn only_a_reference_measured_from_40_to_41_mhz_is_accepted() {
    for (hz, expected) in [
        (10_000_000, Err(-EFAULT)),
        (38_400_000, Err(-EFAULT)),
        (40_000_000, Ok(())),
        (40_900_000, Ok(())),
        (41_100_000, Err(-EFAULT)),
        (52_000_000, Err(-EFAULT)),
    ] {
        let outcome = Oracle::open(SimBoard::new().with_reference_hz(hz));
        let outcome = match outcome {
            Ok(oracle) => {
                let _ = oracle.close();
                Ok(())
            }
            Err(refused) => {
                assert!(
                    !refused.board.trace().iter().any(is_lms7),
                    "{hz} Hz: a refused reference stops init before the LMS7002M"
                );
                assert_eq!(refused.errno, -ENODEV, "{hz} Hz: masked by lowlevel_create");
                Err(refused.device_errno)
            }
        };
        assert_eq!(outcome, expected, "{hz} Hz reference");
    }
}

#[test]
fn libusdrs_gateware_info_request_reads_registers_0_to_3() {
    let board = SimBoard::new()
        .with_fpga_reg(0x0000, 0x0019)
        .with_fpga_reg(0x0001, 0x0002)
        .with_fpga_reg(0x0002, 0x0003)
        .with_fpga_reg(0x0003, 0x0004);
    let mut oracle = Oracle::open(board).expect("libusdr opens the LimeSDR-mini");
    assert_eq!(oracle.fpga_info(), Ok([0x0019, 0x0002, 0x0003, 0x0004]));
}
