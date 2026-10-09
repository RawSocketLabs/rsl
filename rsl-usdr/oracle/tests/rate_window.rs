//! The harness for sample-rate parity: libusdr's trace of one RX rate change, isolated.
//!
//! rsl-usdr has no sample-rate API yet; these checks pin where libusdr's rate call starts
//! and ends, so the parity test that follows the port compares exactly that window.

use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, I2cAddress, Op, SimBoard};

/// The LP8758 PMIC.
const PMIC: I2cAddress = I2cAddress { bus: 0, addr: 0x60 };
/// The Si5332 clock generator.
const CLOCK: I2cAddress = I2cAddress { bus: 0, addr: 0x6a };

/// libusdr's operations for one `set_rx_rate(rate)` on a freshly opened board.
fn libusdr_rate_window(revision: BoardRevision, rate: u32) -> Vec<Op> {
    let mut oracle = Oracle::open(SimBoard::new(revision)).expect("libusdr opens the board");
    let start = oracle.trace_len();
    oracle.set_rx_rate(rate).expect("libusdr accepts the rate");
    let end = oracle.trace_len();
    oracle.close().trace()[start..end].to_vec()
}

/// Whether `op` writes `value` to Si5332 register `reg`.
fn clock_write(op: &Op, reg: u8, value: u8) -> bool {
    matches!(op, Op::I2c { addr, write, .. } if *addr == CLOCK && write[..] == [reg, value])
}

#[test]
fn rate_window_isolates_one_rx_rate_change() {
    for (revision, rate) in [
        (BoardRevision::Rev2, 1_000_000),
        (BoardRevision::Rev3, 1_000_000),
        (BoardRevision::Rev3, 20_000_000),
        (BoardRevision::Rev3, 62_000_000),
    ] {
        let case = format!("{revision:?} at {rate} S/s");
        let window = libusdr_rate_window(revision, rate);

        // `usdr_set_samplerate_ex` starts with the 1.925 V VIO boost at 62 MS/s and above
        // (LP8758 BUCK3_VOUT 0x10 = 0xB7), otherwise with RX power-up's first SPI word
        // (TOP_ENREG with the RX modulator clock, `lms6002d_rfe_enable`).
        let first = window.first().expect("the rate call does something");
        if rate >= 62_000_000 {
            assert!(
                matches!(first, Op::I2c { addr, write, .. } if *addr == PMIC && write[..] == [0x10, 0xb7]),
                "{case}: starts with the VIO boost, got {first:x?}"
            );
        } else {
            assert!(
                matches!(first, Op::Spi { out: 0x8904, .. }),
                "{case}: starts with RX power-up, got {first:x?}"
            );
        }

        // `si5332_set_layout` brackets its writes with USYS_CTRL READY, then ACTIVE.
        let ready = window.iter().position(|op| clock_write(op, 0x06, 0x01));
        let active = window.iter().rposition(|op| clock_write(op, 0x06, 0x02));
        assert!(
            matches!((ready, active), (Some(r), Some(a)) if r < a),
            "{case}: the Si5332 is reprogrammed in one READY..ACTIVE cycle"
        );

        // It ends with `lms6002d_set_bandwidth`'s three RX LPF writes (0x54-0x56).
        let tail: Vec<u32> = window[window.len() - 3..]
            .iter()
            .map(|op| match op {
                Op::Spi { out, .. } => out >> 8,
                other => panic!("{case}: the window ends in LPF writes, got {other:x?}"),
            })
            .collect();
        assert_eq!(
            tail,
            [0xd4, 0xd5, 0xd6],
            "{case}: ends with the RX LPF writes"
        );
    }
}
