//! The simulated board: the LMS64C command handler, the gateware's register file and the
//! LMS7002M behind it.
//!
//! Register facts cite the pinned libusdr (`device/u3_limesdr/limesdr_ctrl.c`). The
//! firmware's command semantics are not in the pinned source (the FT601 transport that
//! sends the frames is not vendored), so each one below is an assumption, stated where it
//! is encoded. So are the stream blocks' (`ipblks/streams/stream_limesdr.c` is how libusdr
//! reads and writes them; the gateware that makes and takes them is not in the source).

use std::collections::BTreeMap;

use lms7002m_sim::Lms7002m;

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
/// How long the detector's gate lasts, in microseconds, rounded up: [`FDC_GATE_CYCLES`] of
/// [`FDC_GATE_HZ`], about 168 ms.
pub const DETECTOR_GATE_US: u64 = (FDC_GATE_CYCLES * 1_000_000).div_ceil(FDC_GATE_HZ);

/// Stream control (`LIMESDR_REG_000A`): bit 0, `RX_EN`, "controls both receiver and
/// transmitter".
const STREAM_CONTROL: u16 = 0x000a;
/// The bit of [`STREAM_CONTROL`] that lets RX blocks flow.
const RX_EN: u16 = 1 << 0;
/// The stream's clears (`LIMESDR_REG_0009`): bit 0, `SMPL_NR_CLR`, "rising edge clears"
/// the sample counter.
const STREAM_CLEARS: u16 = 0x0009;
/// The bit of [`STREAM_CLEARS`] whose rising edge clears the sample counter.
const SMPL_NR_CLR: u16 = 1 << 0;
/// A stream burst: a 16-byte header, then samples (`stream_limesdr.c`'s `burst_size` and
/// `LIMESDR_STREAM_HDR`).
const BURST_BYTES: usize = 4096;
/// A burst's header.
const HEADER_BYTES: usize = 16;
/// Samples in one RX burst: `ci16`, 4 bytes each. Assumption: the gateware streams 16-bit
/// samples, the only width libusdr's stream sync asks for.
const BURST_SAMPLES: u64 = ((BURST_BYTES - HEADER_BYTES) / 4) as u64;

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
    /// Its metal-mask revision.
    lms7_mask: u8,
    /// A cap code at which its VCO comparators are forced to read "fail".
    lms7_failing_cap: Option<u8>,
    /// The reference oscillator's frequency, in hertz.
    reference_hz: u32,
    /// How long a frequency measurement takes, in virtual microseconds.
    detector_delay_us: u64,
    /// When the measurement in progress completes, if one is.
    detector_due_us: Option<u64>,
    /// Virtual time, advanced only by [`SimBoard::sleep_us`].
    now_us: u64,
    /// The RX stream's block, once the transport set it up.
    rx_block_bytes: Option<u32>,
    /// The TX stream's largest block, once the transport set it up.
    tx_block_bytes: Option<u32>,
    /// The sample counter: the next RX burst's first sample time.
    rx_time: u64,
    /// RX bursts delivered since the counter was last cleared.
    rx_bursts: u64,
    /// A gap in the RX samples: after this many bursts since the counter was cleared, the
    /// counter skips this many samples.
    rx_gap: Option<(u64, u64)>,
    /// Every operation performed, in order.
    trace: Vec<Op>,
}

impl Default for SimBoard {
    fn default() -> Self {
        Self::new()
    }
}

impl SimBoard {
    /// A LimeSDR-mini: LMS7002M metal mask 1 and a 40 MHz reference, which also clocks
    /// the LMS7002M, so its VCO comparators answer the tunings libusdr makes
    /// (`lms7002m-sim`'s model). Assumption: the board feeds its one reference
    /// oscillator to the LMS7002M (`limesdr_init` hands the measured frequency to
    /// `lms7002m_init` as the chip's `fref`).
    ///
    /// Gateware registers are plain storage reading 0 until written, except the frequency
    /// detector's (see [`SimBoard::with_reference_hz`]). Assumption: the values a real
    /// gateware reports unprompted (identity at 0x0000-0x0003, stream status) are not in
    /// the source, and init reads none of them; [`SimBoard::with_fpga_reg`] sets them.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fpga: BTreeMap::new(),
            lms: lms7(LMS7_MASK, REFERENCE_HZ, None),
            lms7_mask: LMS7_MASK,
            lms7_failing_cap: None,
            reference_hz: REFERENCE_HZ,
            detector_delay_us: 0,
            detector_due_us: None,
            now_us: 0,
            rx_block_bytes: None,
            tx_block_bytes: None,
            rx_time: 0,
            rx_bursts: 0,
            rx_gap: None,
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
        self.lms7_mask = mask;
        self.rebuild_lms7();
        self
    }

    /// The same board with the LMS7002M's VCO comparators reading "fail" whenever the cap
    /// code last written is `cap` (`lms7002m-sim`'s fault injection, for the tuning
    /// searches' error paths; not a modelled behaviour).
    #[must_use]
    pub fn with_lms7_comparator_failure(mut self, cap: u8) -> Self {
        self.lms7_failing_cap = Some(cap);
        self.rebuild_lms7();
        self
    }

    /// Replaces the LMS7002M, before any driver touched it, with one built from the
    /// board's choices.
    fn rebuild_lms7(&mut self) {
        self.lms = lms7(self.lms7_mask, self.reference_hz, self.lms7_failing_cap);
    }

    /// The same board with a `hz` reference oscillator, which clocks the LMS7002M too.
    ///
    /// Starting the frequency detector (writing the [`FDC_START`] bit) completes the
    /// measurement at once: the count of reference cycles in the detector's gate appears
    /// in registers 0x0072/0x0073 and the done bit in 0x0065. Assumption: real gateware
    /// takes the gate time (about 168 ms); completing at once keeps the trace free of
    /// polls, and libusdr's poll loop proceeds after ten polls regardless. See
    /// [`SimBoard::with_detector_delay_us`] for a detector that takes time.
    #[must_use]
    pub fn with_reference_hz(mut self, hz: u32) -> Self {
        self.reference_hz = hz;
        self.rebuild_lms7();
        self
    }

    /// The same board with a frequency detector that completes `us` virtual microseconds
    /// after it is started, instead of at once: until then the done bit stays clear and the
    /// count registers keep what they held. A delay no driver waits out (`u64::MAX`, or one
    /// past libusdr's ten 50 ms polls) models a detector that never finishes.
    ///
    /// Assumption: the real gate is [`DETECTOR_GATE_US`]; the count appearing all at once
    /// on completion, rather than counting up, is the sim's choice (a driver reads it only
    /// after the done bit).
    #[must_use]
    pub fn with_detector_delay_us(mut self, us: u64) -> Self {
        self.detector_delay_us = us;
        self
    }

    /// The same board with `samples` RX samples lost after `bursts` bursts since the
    /// sample counter was last cleared: the next burst's time is that much later (fault
    /// injection, for the receive's lost-sample filling; not a modelled overrun).
    #[must_use]
    pub fn with_rx_gap(mut self, bursts: u64, samples: u64) -> Self {
        self.rx_gap = Some((bursts, samples));
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
        if addr == STREAM_CLEARS && value & !self.fpga_reg(addr) & SMPL_NR_CLR != 0 {
            self.rx_time = 0;
            self.rx_bursts = 0;
        }
        if addr == FDC_CONTROL {
            // Rewriting the start bit restarts the measurement, whose result is the same: an
            // edge and a level trigger are indistinguishable here.
            if value & FDC_START != 0 {
                self.detector_due_us = Some(self.now_us.saturating_add(self.detector_delay_us));
                self.settle_detector();
            } else {
                self.detector_due_us = None;
                let status = self.fpga_reg(FDC_STATUS);
                self.fpga.insert(FDC_STATUS, status & !FDC_DONE);
            }
        }
        self.fpga.insert(addr, value);
    }

    /// Completes the frequency measurement in progress once its time has come: the count
    /// in 0x0072/0x0073 and the done bit in 0x0065.
    fn settle_detector(&mut self) {
        if self.detector_due_us.is_none_or(|due| self.now_us < due) {
            return;
        }
        self.detector_due_us = None;
        let [hi, lo] = self.reference_count();
        self.fpga.insert(FDC_COUNT_LO, lo);
        self.fpga.insert(FDC_COUNT_HI, hi);
        let status = self.fpga_reg(FDC_STATUS);
        self.fpga.insert(FDC_STATUS, status | FDC_DONE);
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

    /// Sets a stream up for blocks of `block_bytes`, as the transport's `stream_initialize`
    /// asks (`tx` for stream 1, else stream 0); a later call replaces an earlier one.
    ///
    /// # Panics
    ///
    /// When `block_bytes` is not a whole number of 4 KiB bursts, which `stream_limesdr.c`
    /// never asks for.
    pub fn stream_open(&mut self, tx: bool, block_bytes: u32) {
        assert!(
            block_bytes != 0 && block_bytes as usize % BURST_BYTES == 0,
            "a stream block is whole bursts"
        );
        self.trace.push(Op::StreamOpen { tx, block_bytes });
        if tx {
            self.tx_block_bytes = Some(block_bytes);
        } else {
            self.rx_block_bytes = Some(block_bytes);
        }
    }

    /// The next RX block, or `None` (a timeout) without an RX stream or while stream
    /// control's `RX_EN` is clear. Each 4 KiB burst is a header holding the burst's first
    /// sample time at byte 8 (little-endian; bytes 0 to 7 zero), then 1020 `ci16`
    /// samples; the sample of time `t` is I = `t`, Q = `!t`, both truncated to 16 bits.
    /// Assumptions: the gateware sends blocks as the transport sized them, as long as
    /// `RX_EN` is set, at the rate the host reads them (no overrun is modelled; see
    /// [`SimBoard::with_rx_gap`]), its counter counting every sample since `SMPL_NR_CLR`
    /// last rose; byte 0 to 7's contents are not in the source.
    pub fn rx_block(&mut self) -> Option<Vec<u8>> {
        let block_bytes = self.rx_block_bytes?;
        if self.fpga_reg(STREAM_CONTROL) & RX_EN == 0 {
            return None;
        }
        let mut block = vec![0; block_bytes as usize];
        let first = self.rx_time;
        for burst in block.chunks_exact_mut(BURST_BYTES) {
            if self
                .rx_gap
                .is_some_and(|(after, _)| after == self.rx_bursts)
            {
                self.rx_time += self.rx_gap.map_or(0, |(_, samples)| samples);
            }
            burst[8..HEADER_BYTES].copy_from_slice(&self.rx_time.to_le_bytes());
            for (n, sample) in (0..).zip(burst[HEADER_BYTES..].chunks_exact_mut(4)) {
                let [low, high, ..] = (self.rx_time + n).to_le_bytes();
                let t = u16::from_le_bytes([low, high]);
                sample[..2].copy_from_slice(&t.to_le_bytes());
                sample[2..].copy_from_slice(&(!t).to_le_bytes());
            }
            self.rx_time += BURST_SAMPLES;
            self.rx_bursts += 1;
        }
        self.trace.push(Op::RxBlock {
            len: block.len(),
            time: first,
        });
        Some(block)
    }

    /// The TX stream's block size in bytes, once the transport set it up: the buffer a
    /// transport lends the driver to frame a block in.
    #[must_use]
    pub fn tx_capacity(&self) -> Option<usize> {
        self.tx_block_bytes.map(|bytes| bytes as usize)
    }

    /// Takes a TX block the transport committed with `time` as its out-of-band word; false
    /// without a TX stream or for a block over its size. Assumption: the gateware takes
    /// every block it is given; nothing is played.
    pub fn tx_block(&mut self, bytes: &[u8], time: Option<u64>) -> bool {
        if self
            .tx_block_bytes
            .is_none_or(|max| bytes.len() > max as usize)
        {
            return false;
        }
        self.trace.push(Op::TxBlock {
            bytes: bytes.to_vec(),
            time,
        });
        true
    }

    /// Advances virtual time.
    pub fn sleep_us(&mut self, us: u64) {
        self.now_us += us;
        self.trace.push(Op::Sleep { us });
        self.settle_detector();
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

/// An LMS7002M of metal mask `mask`, clocked from `reference_hz`, its comparators failing
/// at `failing_cap` if given.
fn lms7(mask: u8, reference_hz: u32, failing_cap: Option<u8>) -> Lms7002m {
    let chip = Lms7002m::new(mask).with_reference(reference_hz);
    match failing_cap {
        Some(cap) => chip.with_comparator_failure(cap),
        None => chip,
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
    fn a_timed_detector_completes_only_once_its_delay_has_passed() {
        let mut board = SimBoard::new()
            .with_detector_delay_us(DETECTOR_GATE_US)
            .with_fpga_reg(FDC_COUNT_LO, 0x1234);
        write_fpga(&mut board, FDC_CONTROL, FDC_START);
        assert_eq!(read_fpga(&mut board, FDC_STATUS) & FDC_DONE, 0);
        assert_eq!(
            read_fpga(&mut board, FDC_COUNT_LO),
            0x1234,
            "the count is stale until the measurement completes"
        );
        board.sleep_us(DETECTOR_GATE_US - 1);
        assert_eq!(read_fpga(&mut board, FDC_STATUS) & FDC_DONE, 0);
        board.sleep_us(1);
        assert_ne!(read_fpga(&mut board, FDC_STATUS) & FDC_DONE, 0);
        assert_eq!(
            read_fpga(&mut board, FDC_COUNT_LO),
            0x6664,
            "40 MHz, low half"
        );

        let mut stopped = SimBoard::new().with_detector_delay_us(10);
        write_fpga(&mut stopped, FDC_CONTROL, FDC_START);
        write_fpga(&mut stopped, FDC_CONTROL, 0);
        stopped.sleep_us(10);
        assert_eq!(
            read_fpga(&mut stopped, FDC_STATUS) & FDC_DONE,
            0,
            "stopping abandons the measurement"
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

    /// Writes LMS7002M register `addr` through LMS64C.
    fn write_lms7(board: &mut SimBoard, addr: u16, value: u16) {
        let [a0, a1] = (addr | 0x8000).to_be_bytes();
        let [v0, v1] = value.to_be_bytes();
        let reply = board.lms64c(&request(CMD_LMS7002_WR, 0, &[&[a0, a1, v0, v1]]));
        assert_eq!(status(&reply), Status::Completed as u8);
    }

    /// The CGEN comparators (`CGEN_0x008C` bits 13:12) at cap code `cap`, the PLL aiming
    /// at 60 times the reference (2.4 GHz at 40 MHz, inside the model's VCO span).
    fn cgen_comparators(board: &mut SimBoard, cap: u16) -> u16 {
        write_lms7(board, 0x0088, 59 << 4);
        write_lms7(board, 0x008b, cap << 1);
        let reply = board.lms64c(&request(CMD_LMS7002_RD, 0, &[&[0x00, 0x8c]]));
        u16::from_be_bytes([reply[10], reply[11]]) >> 12
    }

    #[test]
    fn the_lms7002m_vco_comparators_answer_from_the_boards_reference() {
        let mut board = SimBoard::new();
        assert_eq!(
            cgen_comparators(&mut board, 0),
            0b00,
            "the fastest code reads low"
        );
        assert_eq!(
            cgen_comparators(&mut board, 255),
            0b11,
            "the slowest reads high"
        );
        let mut faster = SimBoard::new().with_reference_hz(52_000_000);
        assert_eq!(
            cgen_comparators(&mut faster, 0),
            0b11,
            "60 times 52 MHz is above the span: the board's reference clocks the chip"
        );
        let mut failing = SimBoard::new().with_lms7_comparator_failure(255);
        assert_eq!(cgen_comparators(&mut failing, 255), 0b01, "forced to fail");
        assert_eq!(cgen_comparators(&mut failing, 0), 0b00, "only at that code");
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
    fn rx_blocks_flow_while_rx_en_is_set_their_times_counting_from_the_last_clear() {
        let mut board = SimBoard::new().with_rx_gap(3, 100);
        assert_eq!(board.rx_block(), None, "no stream");
        board.stream_open(false, 2 * 4096);
        assert_eq!(board.rx_block(), None, "not enabled");
        write_fpga(&mut board, STREAM_CONTROL, RX_EN);
        let block = board.rx_block().expect("a block");
        assert_eq!(block.len(), 8192);
        assert_eq!(
            &block[..16],
            &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(&block[16..24], &[0, 0, 0xff, 0xff, 1, 0, 0xfe, 0xff]);
        assert_eq!(&block[4096 + 8..4096 + 16], &1020u64.to_le_bytes());
        let block = board.rx_block().expect("a block");
        assert_eq!(&block[8..16], &2040u64.to_le_bytes());
        assert_eq!(
            &block[4096 + 8..4096 + 16],
            &3160u64.to_le_bytes(),
            "the gap"
        );
        write_fpga(&mut board, STREAM_CLEARS, SMPL_NR_CLR);
        let block = board.rx_block().expect("a block");
        assert_eq!(
            &block[8..16],
            &0u64.to_le_bytes(),
            "the rising edge cleared it"
        );
        write_fpga(&mut board, STREAM_CLEARS, SMPL_NR_CLR);
        assert_eq!(
            &board.rx_block().expect("a block")[8..16],
            &2040u64.to_le_bytes()
        );
        let lines: Vec<String> = board.trace().iter().map(ToString::to_string).collect();
        assert!(lines.contains(&"stream-open rx 8192".to_owned()));
        assert!(lines.contains(&"rx-block 8192 2040".to_owned()));
    }

    #[test]
    fn tx_blocks_are_taken_up_to_the_streams_size() {
        let mut board = SimBoard::new();
        assert!(!board.tx_block(&[0; 16], None), "no stream");
        board.stream_open(true, 4096);
        assert!(board.tx_block(&[1, 2], Some(7)));
        assert!(!board.tx_block(&[0; 4097], None));
        assert_eq!(
            board.trace().last().map(ToString::to_string).as_deref(),
            Some("tx-block 2 7 082f2407b4e8902a")
        );
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
