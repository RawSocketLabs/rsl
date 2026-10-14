//! How libusdr tells the LMS7 family members apart: HWID bits 23:16 (`xsdr_init`).

use xsdr_oracle::Oracle;
use xsdr_sim::{I2cAddress, Op, SimBoard, Variant};

/// `EFAULT`: `_xsdr_init_revx`'s "LMS8001 not detected".
const EFAULT: i32 = 14;
/// `EINVAL`: `xsdr_init`'s refusal of an unsupported `hwcfg_devid`.
const EINVAL: i32 = 22;
/// The FPGA-side PMIC every variant has.
const FPGA_PMIC: I2cAddress = I2cAddress { bus: 0, addr: 0x60 };
/// The XTRX's LMS-side PMIC.
const LMS_PMIC: I2cAddress = I2cAddress { bus: 1, addr: 0x60 };
/// The XTRX's DAC80501.
const DAC: I2cAddress = I2cAddress { bus: 0, addr: 0x4b };
/// `IGPO_LMS8_CTRL` selecting the LMS8001's SPI.
const LMS8_SELECTED: Op = Op::RegWrite {
    addr: 0,
    value: 15 << 24 | 0x81,
};

/// The I2C devices the trace addressed, in first-use order.
fn i2c_devices(trace: &[Op]) -> Vec<I2cAddress> {
    let mut devices = Vec::new();
    for op in trace {
        if let Op::I2c { addr, .. } = op {
            if !devices.contains(addr) {
                devices.push(*addr);
            }
        }
    }
    devices
}

/// `/dm/revision` is `HWID << 32 | USR_ACCESS2`; the sim's `USR_ACCESS2` reads 0.
fn reported_hwid(oracle: &mut Oracle) -> u64 {
    oracle
        .get_uint(c"/dm/revision")
        .expect("revision is readable")
        >> 32
}

#[test]
fn xsdr_takes_the_new_revision_path_and_probes_for_a_front_end() {
    let mut oracle = Oracle::open(SimBoard::new(Variant::Xsdr)).expect("libusdr opens an XSDR");
    assert_eq!(reported_hwid(&mut oracle), 0x0030_0000);
    let board = oracle.close();

    let devices = i2c_devices(board.trace());
    assert_eq!(
        devices.first(),
        Some(&FPGA_PMIC),
        "XSDR supplies come from the FPGA-side PMIC alone"
    );
    assert!(
        !devices.contains(&LMS_PMIC) && !devices.contains(&DAC),
        "XSDR has no XTRX supply PMIC or trim DAC"
    );
    assert!(
        devices.iter().any(|addr| addr.bus == 1),
        "new-revision boards probe the front-end connector (I2C bus 1)"
    );
}

#[test]
fn xtrx_takes_the_old_revision_path_with_two_pmics_and_a_trim_dac() {
    let mut oracle = Oracle::open(SimBoard::new(Variant::Xtrx)).expect("libusdr opens an XTRX");
    assert_eq!(reported_hwid(&mut oracle), 0x002e_0000);
    let board = oracle.close();

    assert_eq!(
        i2c_devices(board.trace()),
        [LMS_PMIC, FPGA_PMIC, DAC],
        "XTRX brings up the LMS-side PMIC first, then the FPGA-side one, then the DAC, \
         and probes no front end"
    );
}

/// sSDR variants are classified as such, then refused for want of the LMS8001 the sim
/// does not model.
fn assert_ssdr_stops_at_the_lms8001(variant: Variant) {
    let err = Oracle::open(SimBoard::new(variant))
        .err()
        .expect("no LMS8001 answers, so libusdr refuses the board");
    assert_eq!(err.errno, -EFAULT);
    assert!(
        err.board.trace().contains(&LMS8_SELECTED),
        "the sSDR path looks for its LMS8001"
    );
}

#[test]
fn ssdr_is_identified_but_refused_without_its_lms8001() {
    assert_ssdr_stops_at_the_lms8001(Variant::Ssdr);
}

#[test]
fn ssdr_pro_is_identified_but_refused_without_its_lms8001() {
    assert_ssdr_stops_at_the_lms8001(Variant::SsdrPro);
}

#[test]
fn an_unknown_device_id_is_refused_before_anything_is_driven() {
    // 0x32 is the commented-out `SSDR2_DEV`.
    let board = SimBoard::new(Variant::Xsdr).with_hwid(0x0032_0000);
    let err = Oracle::open(board)
        .err()
        .expect("libusdr refuses an unknown device ID");
    assert_eq!(err.errno, -EINVAL);
    assert!(
        err.board
            .trace()
            .iter()
            .all(|op| matches!(op, Op::RegRead { addr: 0x13, .. })),
        "only the HWID may be read before the refusal"
    );
}
