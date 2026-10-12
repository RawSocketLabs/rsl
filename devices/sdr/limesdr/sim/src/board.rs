//! The simulated board: the LMS64C command handler, the gateware's register file and the
//! LMS7002M behind it.
//!
//! Register facts cite the pinned libusdr (`device/u3_limesdr/limesdr_ctrl.c`). The
//! firmware's command semantics are not in the pinned source (the FT601 transport that
//! sends the frames is not vendored), so each one below is an assumption, stated where it
//! is encoded.

use std::collections::BTreeMap;

use rsl_lms7002m_sim::Lms7002m;

use crate::lms64c::{
    CMD_BRDSPI_RD, CMD_BRDSPI_WR, CMD_LMS7002_RD, CMD_LMS7002_RST, CMD_LMS7002_WR, DATA_LEN,
    FRAME_LEN, Frame, RST_PULSE, Status,
};
use crate::trace::Op;

/// Frequency-detector control (`LIMESDR_REG_0061`): `limesdr_detect_refclk` writes 0, then
/// 4 (bit 2) to start a measurement.
const FDC_CONTROL: u16 = 0x0061;
/// The start bit of [`FDC_CONTROL`].
const FDC_START: u16 = 1 << 2;
/// Frequency-detector status (`LIMESDR_REG_0065`): the C polls bit 2 for completion.
const FDC_STATUS: u16 = 0x0065;
/// The completion bit of [`FDC_STATUS`].
const FDC_DONE: u16 = 1 << 2;
/// The measured count, low half (`LIMESDR_REG_0072`). The C reads it with a 32-bit
/// register read; assumption: the high half is the next register, `LIMESDR_REG_0073`
/// (the enum lists both; `LimeSuite`'s `DetectRefClk` reads the pair).
const FDC_COUNT_LO: u16 = 0x0072;
/// The measured count, high half (`LIMESDR_REG_0073`).
const FDC_COUNT_HI: u16 = 0x0073;
/// The detector's gate: it counts reference cycles over this many cycles of the 100 MHz
/// bridge clock. Both numbers are the C's (`fx3Cnt`, `fx3Clk` in `limesdr_detect_refclk`,
/// which inverts this measurement).
const FDC_GATE_CYCLES: u64 = 16_777_210;
/// The bridge clock the detector's gate runs on, in hertz.
const FDC_GATE_HZ: u64 = 100_000_000;

/// The LMS7002M's metal-mask revision on this board. `limesdr_init` rejects any version
/// word but 0x3841 (`-EIO`), so a board libusdr opens has mask 1.
const LMS7_MASK: u8 = 1;
/// The board's reference oscillator, in hertz. Assumption: the LimeSDR-mini's 40 MHz
/// reference (a board fact, not in the source; 40 MHz is one of the frequencies
/// `limesdr_detect_refclk` accepts).
const REFERENCE_HZ: u32 = 40_000_000;

/// A healthy LimeSDR-mini whose firmware, gateware and LMS7002M answer as the pinned
/// libusdr expects.
#[derive(Debug)]
pub struct SimBoard {
    /// Gateware registers, by address; unwritten registers read zero.
    fpga: BTreeMap<u16, u16>,
    /// The RF transceiver.
    lms: Lms7002m,
    /// The reference oscillator's frequency, in hertz.
    reference_hz: u32,
    /// Virtual time, advanced only by [`SimBoard::sleep_us`].
    now_us: u64,
    /// Every operation performed, in order.
    trace: Vec<Op>,
}

impl Default for SimBoard {
    fn default() -> Self {
        Self::new()
    }
}

impl SimBoard {
    /// A LimeSDR-mini: LMS7002M metal mask 1 and a 40 MHz reference.
    ///
    /// Gateware registers are plain storage reading 0 until written, except the frequency
    /// detector's (see [`SimBoard::with_reference_hz`]). Assumption: the values a real
    /// gateware reports unprompted (identity at 0x0000-0x0003, stream status) are not in
    /// the source, and init reads none of them; [`SimBoard::with_fpga_reg`] sets them.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fpga: BTreeMap::new(),
            lms: Lms7002m::new(LMS7_MASK),
            reference_hz: REFERENCE_HZ,
            now_us: 0,
            trace: Vec::new(),
        }
    }

    /// The same board with an LMS7002M of metal-mask revision `mask`, which libusdr refuses
    /// unless it is 1.
    ///
    /// # Panics
    ///
    /// When `mask` does not fit the 6-bit field.
    #[must_use]
    pub fn with_lms7_mask(mut self, mask: u8) -> Self {
        self.lms = Lms7002m::new(mask);
        self
    }

    /// The same board with a `hz` reference oscillator.
    ///
    /// Starting the frequency detector (writing the [`FDC_START`] bit) completes the
    /// measurement at once: the count of reference cycles in the detector's gate appears
    /// in registers 0x0072/0x0073 and the done bit in 0x0065. Assumption: real gateware
    /// takes the gate time (about 168 ms); completing at once keeps the trace free of
    /// polls, and libusdr's poll loop proceeds after ten polls regardless.
    #[must_use]
    pub fn with_reference_hz(mut self, hz: u32) -> Self {
        self.reference_hz = hz;
        self
    }

    /// The same board with gateware register `addr` reading `value` until written.
    #[must_use]
    pub fn with_fpga_reg(mut self, addr: u16, value: u16) -> Self {
        self.fpga.insert(addr, value);
        self
    }

    /// Handles one LMS64C request frame and returns the reply frame.
    ///
    /// Modelled: `CMD_BRDSPI_WR`/`RD` (gateware registers), `CMD_LMS7002_WR`/`RD`
    /// (LMS7002M registers, peripheral 0 only: the board has one) and `CMD_LMS7002_RST`
    /// with `LMS_RST_PULSE`. Every other command replies `STATUS_UNKNOWN_CMD`, other reset
    /// levels or peripherals `STATUS_ERROR_CMD`, and a block count whose records or reply
    /// overflow the payload `STATUS_MANY_BLOCKS_CMD`; a refused request changes nothing.
    /// Assumption (`LimeSuite`'s convention; consistent with `lms64c_parse_packet`): a reply
    /// echoes the request's command, block count and peripheral, and a read's reply holds
    /// one address-then-value record per address requested.
    pub fn lms64c(&mut self, request: &[u8; FRAME_LEN]) -> [u8; FRAME_LEN] {
        let request = Frame::from_bytes(request);
        self.trace.push(Op::Request(request.clone()));
        let mut data = [0; DATA_LEN];
        let status = self.execute(&request, &mut data);
        let reply = Frame {
            data,
            ..request.reply(status)
        };
        self.trace.push(Op::Reply(reply.clone()));
        reply.to_bytes()
    }

    /// Performs `request`, writing any read results into `reply`.
    fn execute(&mut self, request: &Frame, reply: &mut [u8; DATA_LEN]) -> Status {
        let is_lms = matches!(
            request.cmd,
            CMD_LMS7002_RST | CMD_LMS7002_WR | CMD_LMS7002_RD
        );
        if is_lms && request.periph != 0 {
            return Status::Error;
        }
        match request.cmd {
            CMD_BRDSPI_WR | CMD_LMS7002_WR => {
                let Some(records) = request.records(4) else {
                    return Status::ManyBlocks;
                };
                for record in records {
                    let [addr_hi, addr_lo, value_hi, value_lo] =
                        record.try_into().expect("invariant: 4-byte record");
                    let addr = u16::from_be_bytes([addr_hi, addr_lo]);
                    let value = u16::from_be_bytes([value_hi, value_lo]);
                    if request.cmd == CMD_BRDSPI_WR {
                        self.write_fpga(addr, value);
                    } else {
                        self.lms.write(addr, value);
                        self.trace.push(Op::Lms7Write { addr, value });
                    }
                }
                Status::Completed
            }
            CMD_BRDSPI_RD | CMD_LMS7002_RD => {
                let Some(addrs) = request.records(2) else {
                    return Status::ManyBlocks;
                };
                for (addr, out) in addrs.zip(reply.chunks_exact_mut(4)) {
                    let addr = u16::from_be_bytes(addr.try_into().expect("invariant: 2 bytes"));
                    let value = if request.cmd == CMD_BRDSPI_RD {
                        self.read_fpga(addr)
                    } else {
                        let value = self.lms.reg(addr);
                        self.trace.push(Op::Lms7Read { addr, value });
                        value
                    };
                    out.copy_from_slice(&(u32::from(addr) << 16 | u32::from(value)).to_be_bytes());
                }
                Status::Completed
            }
            // One data byte, the pin level; only the pulse libusdr sends is modelled.
            CMD_LMS7002_RST if request.block_count == 1 && request.data[0] == RST_PULSE => {
                self.lms.reset();
                self.trace.push(Op::Lms7Reset);
                Status::Completed
            }
            CMD_LMS7002_RST => Status::Error,
            _ => Status::Unknown,
        }
    }

    /// Writes a gateware register, running the frequency detector on [`FDC_CONTROL`].
    fn write_fpga(&mut self, addr: u16, value: u16) {
        self.trace.push(Op::FpgaWrite { addr, value });
        if addr == FDC_CONTROL {
            // Measuring completes at once, so rewriting the start bit measures again with
            // the same result: an edge and a level trigger are indistinguishable here.
            let status = self.fpga_reg(FDC_STATUS);
            if value & FDC_START != 0 {
                let [hi, lo] = self.reference_count();
                self.fpga.insert(FDC_COUNT_LO, lo);
                self.fpga.insert(FDC_COUNT_HI, hi);
                self.fpga.insert(FDC_STATUS, status | FDC_DONE);
            } else {
                self.fpga.insert(FDC_STATUS, status & !FDC_DONE);
            }
        }
        self.fpga.insert(addr, value);
    }

    /// Reads a gateware register.
    fn read_fpga(&mut self, addr: u16) -> u16 {
        let value = self.fpga_reg(addr);
        self.trace.push(Op::FpgaRead { addr, value });
        value
    }

    /// Reference cycles counted over the detector's gate, high half first. A 40 MHz
    /// reference counts 6 710 884, which libusdr's estimate turns back into exactly 40 MHz.
    fn reference_count(&self) -> [u16; 2] {
        let count = u64::from(self.reference_hz) * FDC_GATE_CYCLES / FDC_GATE_HZ;
        let count = u32::try_from(count).expect("invariant: a u32 frequency counts below 2^30");
        let [hi_hi, hi_lo, lo_hi, lo_lo] = count.to_be_bytes();
        [
            u16::from_be_bytes([hi_hi, hi_lo]),
            u16::from_be_bytes([lo_hi, lo_lo]),
        ]
    }

    /// Advances virtual time.
    pub fn sleep_us(&mut self, us: u64) {
        self.now_us += us;
        self.trace.push(Op::Sleep { us });
    }

    /// Virtual microseconds elapsed since the board was created.
    #[must_use]
    pub fn now_us(&self) -> u64 {
        self.now_us
    }

    /// Every operation performed so far, in order.
    #[must_use]
    pub fn trace(&self) -> &[Op] {
        &self.trace
    }

    /// A gateware register, untraced.
    #[must_use]
    pub fn fpga_reg(&self, addr: u16) -> u16 {
        self.fpga.get(&addr).copied().unwrap_or(0)
    }

    /// An LMS7002M register, untraced.
    #[must_use]
    pub fn lms7_reg(&self, addr: u16) -> u16 {
        self.lms.reg(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request frame with `records` laid out from the payload's start.
    fn request(cmd: u8, periph: u8, records: &[&[u8]]) -> [u8; FRAME_LEN] {
        let mut frame = [0; FRAME_LEN];
        frame[0] = cmd;
        frame[2] = u8::try_from(records.len()).expect("test: few records");
        frame[3] = periph;
        let mut at = FRAME_LEN - DATA_LEN;
        for record in records {
            frame[at..at + record.len()].copy_from_slice(record);
            at += record.len();
        }
        frame
    }

    /// The status byte of a reply.
    fn status(reply: &[u8; FRAME_LEN]) -> u8 {
        reply[1]
    }

    /// Reads gateware register `addr` through LMS64C.
    fn read_fpga(board: &mut SimBoard, addr: u16) -> u16 {
        let reply = board.lms64c(&request(CMD_BRDSPI_RD, 0, &[&addr.to_be_bytes()]));
        assert_eq!(status(&reply), Status::Completed as u8);
        u16::from_be_bytes([reply[10], reply[11]])
    }

    /// Writes gateware register `addr` through LMS64C.
    fn write_fpga(board: &mut SimBoard, addr: u16, value: u16) {
        let [a0, a1] = addr.to_be_bytes();
        let [v0, v1] = value.to_be_bytes();
        let reply = board.lms64c(&request(CMD_BRDSPI_WR, 0, &[&[a0, a1, v0, v1]]));
        assert_eq!(status(&reply), Status::Completed as u8);
    }

    #[test]
    fn a_read_reply_echoes_each_address_before_its_value() {
        let mut board = SimBoard::new()
            .with_fpga_reg(0x0002, 0xbeef)
            .with_fpga_reg(0x0003, 0x1234);
        let reply = board.lms64c(&request(CMD_BRDSPI_RD, 0, &[&[0, 2], &[0, 3]]));
        assert_eq!(
            &reply[..4],
            &[CMD_BRDSPI_RD, 1, 2, 0],
            "cmd, status, blocks, periph"
        );
        assert_eq!(&reply[8..16], &[0, 2, 0xbe, 0xef, 0, 3, 0x12, 0x34]);
    }

    #[test]
    fn the_frequency_detector_measures_the_reference_libusdr_expects() {
        let mut board = SimBoard::new();
        write_fpga(&mut board, FDC_CONTROL, 0);
        assert_eq!(read_fpga(&mut board, FDC_STATUS) & FDC_DONE, 0);
        write_fpga(&mut board, FDC_CONTROL, FDC_START);
        assert_ne!(
            read_fpga(&mut board, FDC_STATUS) & FDC_DONE,
            0,
            "done once started"
        );
        let count = u64::from(read_fpga(&mut board, FDC_COUNT_LO))
            | u64::from(read_fpga(&mut board, FDC_COUNT_HI)) << 16;
        // libusdr's own estimate (`limesdr_detect_refclk`): count * fx3Clk / fx3Cnt.
        assert_eq!(count * 100_000_000 / 16_777_210, 40_000_000);
        write_fpga(&mut board, FDC_CONTROL, 0);
        assert_eq!(
            read_fpga(&mut board, FDC_STATUS) & FDC_DONE,
            0,
            "stopping clears done"
        );
    }

    #[test]
    fn lms7_writes_reach_the_chip_and_reads_return_them() {
        let mut board = SimBoard::new();
        let reply = board.lms64c(&request(CMD_LMS7002_WR, 0, &[&[0x80, 0x92, 0xab, 0xcd]]));
        assert_eq!(status(&reply), Status::Completed as u8);
        assert_eq!(board.lms7_reg(0x0092), 0xabcd, "bit 15 is the write flag");
        let reply = board.lms64c(&request(CMD_LMS7002_RD, 0, &[&[0x00, 0x92]]));
        assert_eq!(
            &reply[8..12],
            &[0x00, 0x92, 0xab, 0xcd],
            "the written value reads back over LMS64C"
        );
        let reply = board.lms64c(&request(CMD_LMS7002_RD, 0, &[&[0x00, 0x2f]]));
        assert_eq!(
            &reply[8..12],
            &[0x00, 0x2f, 0x38, 0x41],
            "the version libusdr wants"
        );
    }

    #[test]
    fn a_reset_pulse_returns_the_lms7002m_to_power_on() {
        let mut board = SimBoard::new();
        board.lms64c(&request(CMD_LMS7002_WR, 0, &[&[0x80, 0x20, 0xff, 0xff]]));
        let reply = board.lms64c(&request(CMD_LMS7002_RST, 0, &[&[RST_PULSE]]));
        assert_eq!(status(&reply), Status::Completed as u8);
        assert_eq!(board.lms7_reg(0x0020), 0);
    }

    #[test]
    fn unmodelled_requests_are_refused_without_effect() {
        let mut board = SimBoard::new();
        for (frame, expected) in [
            (request(0x00, 0, &[&[0]]), Status::Unknown),
            (request(CMD_LMS7002_RST, 0, &[&[1]]), Status::Error),
            (
                request(CMD_LMS7002_WR, 1, &[&[0x80, 0x20, 0, 1]]),
                Status::Error,
            ),
        ] {
            assert_eq!(
                status(&board.lms64c(&frame)),
                expected as u8,
                "{frame:02x?}"
            );
        }
        let mut overflowing = request(CMD_BRDSPI_WR, 0, &[]);
        overflowing[2] = 15;
        assert_eq!(
            status(&board.lms64c(&overflowing)),
            Status::ManyBlocks as u8
        );
        assert!(
            board
                .trace()
                .iter()
                .all(|op| matches!(op, Op::Request(_) | Op::Reply(_))),
            "a refused request decodes to no register operation"
        );
        assert_eq!(board.lms7_reg(0x0020), 0);
    }

    #[test]
    fn each_exchange_traces_request_operations_then_reply() {
        let mut board = SimBoard::new();
        write_fpga(&mut board, 0x000a, 3);
        board.sleep_us(7);
        let lines: Vec<String> = board.trace().iter().map(ToString::to_string).collect();
        assert_eq!(
            lines,
            [
                "req cmd=0x55 st=0 blk=1 id=0 d=000a0003",
                "fpga-wr 0x000a 0x0003",
                "rep cmd=0x55 st=1 blk=1 id=0 d=-",
                "sleep 7",
            ]
        );
        assert_eq!(board.now_us(), 7);
    }
}
