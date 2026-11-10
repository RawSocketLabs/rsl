//! The `RTL-OBS` rows in `PROVENANCE.md` hold over every committed capture trace: the
//! evidence, checked against the constants that cite it. Rows 01 to 07; RTL-OBS-08's bulk
//! stream is not in the traces, which hold control transfers only.

use rtlsdr::r820t2::registers::I2C_ADDRESS;
use rtlsdr::rtl2832u::{
    GATE, GATE_CLOSED, GATE_OPEN, I2C_BLOCK, Location, PAGED_VALUE, POST_WRITE_READ, REQUEST_TYPE,
    VENDOR_REQUEST, WRITE,
};
use rtlsdr_sim::{Control, Direction, Trace, Transfer};

/// The third byte the gate register is written with, bit 2's meaning unknown (RTL-OBS-07).
const GATE_OTHER: u8 = 0x14;

/// The traces taken from the V5's captures.
fn traces() -> [Trace; 3] {
    [
        include_str!("fixtures/gplobs-v5-0001.trace"),
        include_str!("fixtures/gplobs-v5-0002.trace"),
        include_str!("fixtures/gplobs-v5-0003.trace"),
    ]
    .map(|text| Trace::parse(text).expect("the fixture parses"))
}

/// Every transfer of every trace, with its trace's capture id and line.
fn transfers() -> Vec<(String, usize, Transfer)> {
    traces()
        .iter()
        .flat_map(|trace| {
            trace
                .transfers()
                .iter()
                .map(|(line, transfer)| (trace.capture().to_owned(), *line, transfer.clone()))
        })
        .collect()
}

/// A transfer's setup, whether it is a write, and a completed write's data.
fn parts(transfer: &Transfer) -> (Control, bool, Option<&[u8]>) {
    match transfer {
        Transfer::Out { control, data } => (*control, true, Some(data)),
        Transfer::In { control, .. } => (*control, false, None),
        Transfer::Stall { control, direction } => (*control, *direction == Direction::Out, None),
    }
}

/// The block a setup addresses (`wIndex` high byte).
fn block(control: Control) -> u16 {
    (control.index & !WRITE) >> 8
}

#[test]
fn every_vendor_transfer_uses_the_recorded_request_and_write_bit() {
    for trace in traces() {
        assert!(!trace.is_synthetic(), "a capture trace");
    }
    for (capture, line, transfer) in transfers() {
        let (control, is_write, _) = parts(&transfer);
        assert_eq!(
            control.request_type & 0x7f,
            REQUEST_TYPE,
            "RTL-OBS-01, {capture}:{line}"
        );
        assert_eq!(
            control.request, VENDOR_REQUEST,
            "RTL-OBS-01, {capture}:{line}"
        );
        assert_eq!(
            control.index & WRITE != 0,
            is_write,
            "RTL-OBS-02, {capture}:{line}"
        );
    }
}

#[test]
fn paged_block_transfers_carry_the_paged_value_and_a_page_below_0x10() {
    for (capture, line, transfer) in transfers() {
        let (control, _, _) = parts(&transfer);
        if block(control) == 0 {
            assert_eq!(
                control.value & 0xff,
                PAGED_VALUE,
                "RTL-OBS-03, {capture}:{line}"
            );
            assert!(
                control.index & 0x0f == control.index & !WRITE,
                "RTL-OBS-03, {capture}:{line}"
            );
        }
    }
}

#[test]
fn every_paged_write_is_followed_by_the_post_write_read() {
    let mut writes = 0;
    for trace in traces() {
        let transfers = trace.transfers();
        for (at, (line, transfer)) in transfers.iter().enumerate() {
            let (control, is_write, _) = parts(transfer);
            if !is_write || block(control) != 0 {
                continue;
            }
            writes += 1;
            let Some((
                _,
                Transfer::In {
                    control: next,
                    len,
                    data,
                },
            )) = transfers.get(at + 1)
            else {
                panic!("RTL-OBS-04: no read after the paged write at line {line}");
            };
            assert_eq!(
                (next.value, next.index, *len, data.as_slice()),
                (
                    POST_WRITE_READ.value(),
                    POST_WRITE_READ.index(),
                    1,
                    &[0x09][..]
                ),
                "RTL-OBS-04, line {line}"
            );
        }
    }
    assert_eq!(writes, 198, "RTL-OBS-04 counts 198 paged writes");
}

#[test]
fn addressed_blocks_are_only_written() {
    for (capture, line, transfer) in transfers() {
        let (control, is_write, _) = parts(&transfer);
        if matches!(block(control), 1 | 2) {
            assert!(
                is_write,
                "RTL-OBS-05: a read of block 1 or 2 at {capture}:{line}"
            );
        }
    }
}

#[test]
fn every_tuner_read_follows_a_write_of_register_0_and_only_absent_addresses_stall() {
    let tuner = Location::i2c(I2C_ADDRESS).expect("a 7-bit address");
    let mut reads = 0;
    for trace in traces() {
        let transfers = trace.transfers();
        for (at, (line, transfer)) in transfers.iter().enumerate() {
            let (control, _, _) = parts(transfer);
            if block(control) != u16::from(I2C_BLOCK) {
                continue;
            }
            match transfer {
                Transfer::In { .. } if control.value == tuner.value() => {
                    reads += 1;
                    let previous = at.checked_sub(1).map(|at| &transfers[at].1);
                    assert!(
                        matches!(previous, Some(Transfer::Out { control, data })
                            if control.value == tuner.value() && data == &[0x00]),
                        "RTL-OBS-06: a tuner read without register 0 written first, line {line}"
                    );
                }
                // Absent addresses, and the tool's teardown (RTL-OBS-07), stall.
                Transfer::Stall { .. } => {}
                _ => assert_eq!(
                    control.value,
                    tuner.value(),
                    "RTL-OBS-06: only absent addresses stall, line {line}"
                ),
            }
        }
    }
    assert_eq!(reads, 28, "every tuner read in the three captures");
}

#[test]
fn the_tuner_is_reached_only_while_the_gate_is_open() {
    let tuner = Location::i2c(I2C_ADDRESS).expect("a 7-bit address");
    let mut checked = 0;
    for trace in traces() {
        let mut gate = None;
        for (line, transfer) in trace.transfers() {
            let (control, _, data) = parts(transfer);
            let gate_write = (control.value, control.index) == (GATE.value(), GATE.write_index());
            if let (true, Some(data)) = (gate_write, data) {
                assert!(
                    matches!(data[0], GATE_OPEN | GATE_CLOSED | GATE_OTHER),
                    "RTL-OBS-07: an unrecorded gate byte at line {line}"
                );
                gate = Some(data[0]);
            }
            let completed = !matches!(transfer, Transfer::Stall { .. });
            if completed && block(control) == u16::from(I2C_BLOCK) && control.value == tuner.value()
            {
                checked += 1;
                assert!(
                    gate.is_some_and(|byte| byte & 0x08 != 0),
                    "RTL-OBS-07: tuner traffic completed with the gate closed at line {line}"
                );
            }
        }
    }
    assert_eq!(
        checked, 200,
        "RTL-OBS-07 counts 200 completed tuner transfers"
    );
}
