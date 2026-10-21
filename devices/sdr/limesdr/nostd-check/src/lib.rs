//! Compile witness for the exact private limesdr portable code, not another driver.
//! A bare-metal check proves core (+ `bnb`'s alloc) compilation, not linking or hardware
//! operation.
#![no_std]
#![forbid(unsafe_code)]

#[path = "../../src/portable/mod.rs"]
mod portable;

use hal::{Sequence, SequenceAction, SequenceFailure};
use portable::framing::{
    BURST_BYTES, RxDeframer, TX_SAMPLE_MULTIPLE, frame_tx, rx_bursts, tx_frame_bytes, tx_plan,
};
use portable::lms64c::{FRAME_LEN, Frame, STATUS_COMPLETED};
use portable::lsop::{BadReply, LsOp};
use portable::open::{Action, Failure, Open};
use portable::rate::SetSampleRate;
use portable::stream::PrepareStreaming;
use portable::sync::StreamSync;

/// LMS7002M registers the fake board stores: the chip's 0x0000 to 0x07FF.
const LMS_REGS: usize = 0x0800;

/// A healthy LimeSDR-mini: the detector done on a 40 MHz count, every other gateware
/// register 0; an LMS7002M storing its writes, reading `version` at 0x002F and VCO
/// comparators that read in range for cap codes 120 to 136, low below, high above.
struct Board {
    /// The LMS7002M's version word.
    version: u16,
    /// The LMS7002M's registers.
    lms: [u16; LMS_REGS],
}

impl Board {
    /// The board with an LMS7002M reading `version`.
    const fn new(version: u16) -> Self {
        Self {
            version,
            lms: [0; LMS_REGS],
        }
    }

    /// A gateware register.
    const fn fpga(addr: u16) -> u16 {
        match addr {
            0x0065 => 1 << 2,
            0x0072 => 0x6664,
            0x0073 => 0x0066,
            _ => 0,
        }
    }

    /// An LMS7002M register.
    fn lms7(&self, addr: u16) -> u16 {
        let stored = |addr: u16| self.lms[usize::from(addr) % LMS_REGS];
        let comparators = |cap: u16| match cap {
            0..120 => 0b00 << 12,
            120..=136 => 0b10 << 12,
            _ => 0b11 << 12,
        };
        match addr {
            0x002f => self.version,
            0x008c => comparators(stored(0x008b) >> 1 & 0xff),
            0x0123 => comparators(stored(0x0121) >> 3 & 0xff),
            _ => stored(addr),
        }
    }

    /// The reply to `request`: the frame echoed, completed, LMS7002M writes stored, a
    /// read's addresses answered with address-then-value records.
    fn reply(&mut self, request: Frame) -> Frame {
        let mut bytes = request.to_bytes();
        bytes[1] = STATUS_COMPLETED;
        let blocks = usize::from(request.block_count());
        match request.command() {
            0x21 => {
                for record in bytes[8..8 + 4 * blocks].chunks_exact(4) {
                    let addr = u16::from_be_bytes([record[0], record[1]]) & 0x7fff;
                    self.lms[usize::from(addr) % LMS_REGS] =
                        u16::from_be_bytes([record[2], record[3]]);
                }
            }
            command @ (0x22 | 0x56) => {
                let mut records = [0; FRAME_LEN - 8];
                let addrs = &bytes[8..8 + 2 * blocks];
                for (record, addr) in records.chunks_exact_mut(4).zip(addrs.chunks_exact(2)) {
                    let addr = u16::from_be_bytes([addr[0], addr[1]]);
                    let value = if command == 0x22 {
                        self.lms7(addr)
                    } else {
                        Self::fpga(addr)
                    };
                    record[..2].copy_from_slice(&addr.to_be_bytes());
                    record[2..].copy_from_slice(&value.to_be_bytes());
                }
                bytes[8..].copy_from_slice(&records);
            }
            _ => {}
        }
        Frame::from_bytes(bytes)
    }

    /// What `op` returns.
    fn answer(&mut self, op: LsOp) -> Result<u32, u8> {
        op.reply(&self.reply(op.request()))
            .map_err(|bad| match bad {
                BadReply::Refused(status) | BadReply::Mismatched(status) => status,
            })
    }

    /// Runs the real open to its end: its exchanges and the finished machine.
    fn open(&mut self) -> Result<(usize, Open), &'static str> {
        let mut open = Open::new();
        let mut exchanges = 0;
        while let Some(action) = open.next() {
            let result = match action {
                Action::Op(op) => {
                    exchanges += 1;
                    self.answer(op)
                }
                Action::Sleep(_) => Ok(0),
            };
            open.complete(result).map_err(reason)?;
        }
        Ok((exchanges, open))
    }

    /// Runs a portable sequence to its end: its exchanges.
    fn drive(&mut self, sequence: &mut impl Sequence) -> Result<usize, &'static str> {
        let mut exchanges = 0;
        while let Some(action) = sequence.next() {
            let result = match action {
                SequenceAction::Fpga(access) => {
                    exchanges += 1;
                    self.answer(LsOp::from_access(access))
                }
                SequenceAction::Sleep(_) => Ok(0),
                SequenceAction::Access(_) => return Err("I2C"),
            };
            sequence
                .complete(result)
                .map_err(|failure| failure_chip(&failure))?;
        }
        Ok(exchanges)
    }
}

/// Runs the real open through the real LMS64C mapping and codec on a board whose LMS7002M
/// reads `version`: the exchanges made and the reference recognised, or what stopped it.
pub fn open(version: u16) -> Result<(usize, u32), &'static str> {
    let (exchanges, open) = Board::new(version).open()?;
    Ok((exchanges, open.reference_hz().ok_or("no reference")?))
}

/// Opens the healthy board, sets `rate`'s RX and TX samples per second (none: leaves the
/// rate to the stream preparation) and prepares streaming, all through the real machines,
/// mapping and codec: the exchanges after the open, or what stopped them.
pub fn prepare_streaming(rate: Option<(u32, u32)>) -> Result<usize, &'static str> {
    let mut board = Board::new(0x3841);
    let (_, open) = board.open()?;
    let mut control = open.control().ok_or("no control state")?;
    let mut exchanges = 0;
    if let Some((rx, tx)) = rate {
        let mut set = SetSampleRate::new(&control, rx, tx);
        exchanges += board.drive(&mut set)?;
        if set.refusal().is_some() {
            return Err("rate refused");
        }
        control = *set.control();
    }
    let mut prepare = PrepareStreaming::new(&control);
    exchanges += board.drive(&mut prepare)?;
    match prepare.refusal() {
        Some(_) => Err("streaming up refused"),
        None if prepare.control().rx_running() == [true, false] => Ok(exchanges),
        None => Err("channel A not running"),
    }
}

/// Opens the healthy board, prepares streaming, then starts and stops the streams through
/// the real sync sequences, mapping and codec: the start's and the stop's exchanges.
pub fn sync_streams() -> Result<(usize, usize), &'static str> {
    let mut board = Board::new(0x3841);
    let (_, open) = board.open()?;
    let control = open.control().ok_or("no control state")?;
    let mut prepare = PrepareStreaming::new(&control);
    board.drive(&mut prepare)?;
    let mut setup = StreamSync::setup(prepare.control());
    let started = board.drive(&mut setup)?;
    if setup.refused() {
        return Err("still streaming");
    }
    Ok((started, board.drive(&mut StreamSync::disable())?))
}

/// Frames two bursts of TX samples timed from 77 into a block and receives a packet of
/// them back through the RX deframer (a TX block's layout is an RX one's; a TX stream
/// of that packet is raised to 4096 samples, five bursts): the bytes
/// framed and the packet's first sample time, or what differed.
pub fn frame_and_receive() -> Result<(usize, u64), &'static str> {
    const BLOCK: usize = 2 * BURST_BYTES as usize;
    const SAMPLES: u32 = 2040;
    let samples = [[1i16, -1]; SAMPLES as usize];
    let mut wire = [0u8; BLOCK];
    let framed = frame_tx(&samples, Some(77), &mut wire);
    if framed != tx_frame_bytes(samples.len()) || tx_plan(SAMPLES) != Some(5) {
        return Err("TX plan");
    }
    if SAMPLES % TX_SAMPLE_MULTIPLE != 0 {
        return Err("TX multiple");
    }
    let mut deframer = RxDeframer::new(SAMPLES, rx_bursts(SAMPLES)).ok_or("RX stream")?;
    if deframer.block_bytes() as usize != BLOCK {
        return Err("RX block");
    }
    let mut held = [0u8; BLOCK];
    let mut host = [[0i16; 2]; SAMPLES as usize];
    let time = deframer
        .receive(&mut held, &mut host, |block| {
            block.copy_from_slice(&wire);
            Ok::<_, ()>(())
        })
        .map_err(|()| "fetch")?;
    if host != samples || deframer.overruns() != 0 {
        return Err("samples");
    }
    Ok((framed, time))
}

/// The chip a sequence failure names.
fn failure_chip(failure: &SequenceFailure<u8>) -> &'static str {
    match *failure {
        SequenceFailure::Transport { chip, .. }
        | SequenceFailure::FpgaTransport { chip, .. }
        | SequenceFailure::ChipId { chip, .. }
        | SequenceFailure::ChipMissing { chip }
        | SequenceFailure::InputMissing { chip } => chip,
    }
}

/// What a failure names.
fn reason(failure: Failure<u8>) -> &'static str {
    match failure {
        Failure::Sequence(failure) => failure_chip(&failure),
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

    #[test]
    fn the_exact_source_witness_prepares_streaming_after_the_default_or_a_set_rate() {
        let default = prepare_streaming(None).expect("the default rate, then streaming up");
        let set = prepare_streaming(Some((1_000_000, 1_000_000))).expect("a set rate");
        assert_eq!(
            default, set,
            "the default rate is 1 MS/s, set the same way as any other"
        );
    }

    #[test]
    fn the_exact_source_witness_starts_and_stops_the_streams() {
        // Start: select, stop (2), check, reset (4), DC (8 LMS7002M writes), mode,
        // channels, the clears' read, start (2), the pulse (2). Stop: select, stop (2).
        assert_eq!(sync_streams(), Ok((1 + 2 + 1 + 4 + 8 + 2 + 1 + 2 + 2, 3)));
    }

    #[test]
    fn the_exact_source_witness_frames_and_receives_samples() {
        assert_eq!(frame_and_receive(), Ok((2 * 4096, 77)));
    }
}
