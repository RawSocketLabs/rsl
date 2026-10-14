//! usdr's thermal gate, the one intentional addition to its power-up trace. Shared by
//! the parity tests that compare whole power-ups (`#[path]`-included).

use usdr_sim::{BoardRevision, I2cAddress, Op};

/// The TMP114.
const TEMP: I2cAddress = I2cAddress { bus: 0, addr: 0x4e };
/// The LP8758 PMIC.
const PMIC: I2cAddress = I2cAddress { bus: 0, addr: 0x60 };

/// usdr's trace without its thermal gate.
///
/// Intentional addition: before the first PMIC operation usdr reads the TMP114
/// temperature register, and on revisions 1 and 2 first checks the TMP114 ID (libusdr
/// checks it only on revision 3). The gate must sit exactly there, so a misplaced or
/// extra temperature read still fails the comparison.
pub(crate) fn without_thermal_gate(trace: &[Op], revision: BoardRevision) -> Vec<Op> {
    let gate_len = if revision == BoardRevision::Rev3 {
        1
    } else {
        2
    };
    let first_pmic_op = trace
        .iter()
        .position(|op| matches!(op, Op::I2c { addr, .. } if *addr == PMIC))
        .expect("power-up reaches the PMIC");
    let gate = &trace[first_pmic_op - gate_len..first_pmic_op];
    assert!(
        matches!(gate.last(), Some(Op::I2c { addr, write, .. }) if *addr == TEMP && write[..] == [0x00]),
        "the gate ends in one temperature read: {gate:x?}"
    );
    assert!(
        gate.iter()
            .all(|op| matches!(op, Op::I2c { addr, .. } if *addr == TEMP)),
        "the gate only talks to the TMP114: {gate:x?}"
    );
    [&trace[..first_pmic_op - gate_len], &trace[first_pmic_op..]].concat()
}

/// Panics at the first operation where `actual` departs from `expected`, or if one is a
/// prefix of the other.
pub(crate) fn assert_same_ops(expected: &[Op], actual: &[Op], context: &str) {
    if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
        panic!(
            "{context}: first difference at op {i}: libusdr {:x?}, usdr {:x?}",
            expected[i], actual[i]
        );
    }
    assert_eq!(
        expected.len(),
        actual.len(),
        "{context}: same prefix, different lengths"
    );
}
