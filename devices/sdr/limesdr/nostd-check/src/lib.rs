//! Compile witness for the exact private rsl-limesdr portable code, not another driver.
//! A bare-metal check proves core (+ `bnb`'s alloc) compilation, not linking or hardware
//! operation.
#![no_std]
#![forbid(unsafe_code)]

#[path = "../../src/portable/mod.rs"]
mod portable;

use portable::lms64c::{FRAME_LEN, Frame, STATUS_COMPLETED};
use portable::lsop::{BadReply, LsOp};
use portable::open::{Action, Failure, Open};
use rsl_lowlevel::SequenceFailure;

/// A healthy LimeSDR-mini's register values: the detector done, a 40 MHz count, LMS7002M
/// version `version`; every other register reads 0.
fn register(addr: u16, version: u16) -> u16 {
    match addr {
        0x0065 => 1 << 2,
        0x0072 => 0x6664,
        0x0073 => 0x0066,
        0x002f => version,
        _ => 0,
    }
}

/// The board's reply to `request`: the frame echoed, completed, a read's addresses
/// answered with address-then-value records.
fn reply(request: Frame, version: u16) -> Frame {
    let mut bytes = request.to_bytes();
    bytes[1] = STATUS_COMPLETED;
    if matches!(request.command(), 0x22 | 0x56) {
        let mut records = [0; FRAME_LEN - 8];
        let addrs = &bytes[8..8 + 2 * usize::from(request.block_count())];
        for (record, addr) in records.chunks_exact_mut(4).zip(addrs.chunks_exact(2)) {
            let addr = u16::from_be_bytes([addr[0], addr[1]]);
            record[..2].copy_from_slice(&addr.to_be_bytes());
            record[2..].copy_from_slice(&register(addr, version).to_be_bytes());
        }
        bytes[8..].copy_from_slice(&records);
    }
    Frame::from_bytes(bytes)
}

/// Runs the real open through the real LMS64C mapping and codec on a board whose LMS7002M
/// reads `version`: the exchanges made and the reference recognised, or what stopped it.
pub fn open(version: u16) -> Result<(usize, u32), &'static str> {
    let mut open = Open::new();
    let mut exchanges = 0;
    while let Some(action) = open.next() {
        let result = match action {
            Action::Op(op) => {
                exchanges += 1;
                answer(op, version)
            }
            Action::Sleep(_) => Ok(0),
        };
        open.complete(result).map_err(reason)?;
    }
    Ok((exchanges, open.reference_hz().ok_or("no reference")?))
}

/// What `op` returns from the healthy board.
fn answer(op: LsOp, version: u16) -> Result<u32, u8> {
    op.reply(&reply(op.request(), version))
        .map_err(|bad| match bad {
            BadReply::Refused(status) | BadReply::Mismatched(status) => status,
        })
}

/// What a failure names.
fn reason(failure: Failure<u8>) -> &'static str {
    match failure {
        Failure::Sequence(
            SequenceFailure::Transport { chip, .. }
            | SequenceFailure::FpgaTransport { chip, .. }
            | SequenceFailure::ChipId { chip, .. }
            | SequenceFailure::ChipMissing { chip }
            | SequenceFailure::InputMissing { chip },
        ) => chip,
        Failure::Reset(_) => "reset",
        Failure::DetectorStalled => "detector",
        Failure::UnknownReference { .. } => "reference",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exact_source_witness_runs_the_pinned_open() {
        // Golden: the open's 20 exchanges, at 40 MHz.
        assert_eq!(open(0x3841), Ok((20, 40_000_000)));
    }

    #[test]
    fn the_exact_source_witness_keeps_the_boards_version_gate() {
        assert_eq!(open(0x3840), Err("LMS7002M"));
    }
}
