//! The FPGA's RX stream engine, as far as a driver can observe it: the registers that
//! start and size it, and the blocks it delivers.
//!
//! The engine runs once the DMA is enabled (register 5, value 1), the front end is started
//! (command register 0x10100, reset command with run state 2) and the sync unit releases
//! the streams (see `sync.rs`; the board asks it). Each block it then
//! delivers holds the configured number of bursts; each burst the configured samples,
//! packed in the configured format, then zeros up to the burst's whole 8-byte words.
//! Register meanings: libusdr's `ipblks/streams/sfe_rx_4.c`, `dma_rx_32.c` and
//! `stream_sfetrx4_dma32.c`.
//!
//! Formats: the burst format's sample format (bits 1:0) selects 16-bit (3) or 12-bit (2)
//! lanes; its lanes (bits 4:2) are 1, channel 0's I and Q on lanes 0 and 1, 2, lane 0
//! (I) alone, 3, lane 1 (Q) alone, 5, channel 1's I and Q on lanes 2 and 3, or 0, all
//! four, channel 0 then channel 1 for each sample. Others (lanes 2 or 3 alone, 2 and 0)
//! are not modelled and panic.
//! - Channel 1: assumed, as libusdr's lane selection and its RX chain's second NCO
//!   suggest (no gateware source is vendored), to be a second complex channel on lanes 2
//!   and 3, sample-aligned with channel 0, lanes packed in order within each sample: the
//!   layout libusdr's one-to-two converters split (`xdsp/conv.c`).
//! - 16-bit: each lane value is a little-endian `i16`; a complex sample is I then Q, 4
//!   bytes, a real one 2 bytes.
//! - 12-bit: assumed, from the C that reads it rather than gateware source (none is
//!   vendored), that lane values are 12 bits each, packed back to back two per three
//!   little-endian bytes (the first in bits 11:0, the second in 23:12), across samples and
//!   words; a complex sample is I then Q, 3 bytes, two real samples 3 bytes, an odd last
//!   one half a byte short. Evidence: `sfe_rx4_configure` sizes a burst as `lanes × 12
//!   bits` per sample over 64-bit words (`burst_fe_calculate`), with no per-sample
//!   alignment, and libusdr's `ci12` and `i12` host formats unpack exactly that layout
//!   (`xdsp/conv.c`'s 12-bit kernels, `usdr::dsp`).
//! - Real lanes: assumed that a one-lane format carries that lane's values alone, in
//!   order, as `IFMT_CH_xxx0` and `IFMT_CH_xx1x` name them.
//!
//! Assumed, not modelled:
//! - Timing and overruns. A block is ready whenever the engine runs, without advancing
//!   virtual time, and none is ever lost.
//! - The signal. The samples are a counting test pattern in the lanes' own width: sample
//!   `n` reads `I = n`, `Q = -n` on channel 0 and `I = n + 0x400`, `Q = -(n + 0x400)` on
//!   channel 1, wrapping at 16 or 12 bits, whichever lanes a burst carries.
//! - The DMA's own words-per-burst register (0x10020). Block sizes come from the front
//!   end's burst format instead; both drivers write the two consistently.
//! - The host-ready strobe (register 5, value 4). Blocks flow without it; libusdr writes
//!   it before every receive until the first succeeds, and so does usdr.
//! - The DMA status, the second out-of-band word, whose fields libusdr only logs
//!   (`_sfetrx4_stream_recv`). Only its transferred-buffer count, bits 13:8, moves: the
//!   blocks delivered so far, the one just delivered included, modulo 64. The rest reads 0.

/// RX DMA control: 1 runs the DMA; 0 and 2 (stop and front-end reset) halt it; 4 (the host
/// is ready) leaves it as it is.
const REG_RX_DMA_CONTROL: u32 = 5;
/// RX DMA configuration: bursts per block, less one.
const REG_RX_DMA_BURSTS: u32 = 0x1_0040;
/// The RX front end's command register: command in bits 31:28, value below.
const REG_RX_FRONT_END: u32 = 0x1_0100;

/// Front-end command: samples per burst, less one.
const CMD_BURST_SAMPLES: u32 = 0;
/// Front-end command: burst format; words per burst, less one, in bits 17:5, lanes in
/// bits 4:2, sample format in bits 1:0.
const CMD_BURST_FORMAT: u32 = 1;
/// Front-end command: reset and run state (bits 2:0: 2 start now, 4 stop now), plus block
/// resets (bits 8, 13, 14, 15: DSP, DDR, sample assembler, burster).
const CMD_CONTROL: u32 = 3;
/// The front end's block-reset bits (`SFE_CMD_RST_*_OFF` in `sfe_rx_4.c`).
const RESET_BITS: u32 = 1 << 8 | 1 << 13 | 1 << 14 | 1 << 15;
/// Most bursts in a block (`MAX_BURSTS_RX_IN_BUFF`).
const MAX_BURSTS: usize = 32;
/// Bytes per front-end word.
const WORD_BYTES: usize = 8;

/// Which of the receiver's lanes a burst carries (`IFMT_CH_*`, burst-format bits 4:2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Lanes {
    /// Lanes 0 and 1, I then Q (`IFMT_CH_xx10`, 1).
    #[default]
    IAndQ,

    /// Lane 0, I alone (`IFMT_CH_xxx0`, 2).
    I,

    /// Lane 1, Q alone (`IFMT_CH_xx1x`, 3).
    Q,

    /// Lanes 2 and 3, channel 1's I then Q (`IFMT_CH_32xx`, 5).
    Channel1,

    /// All four lanes: channel 0's I and Q, then channel 1's (`IFMT_CH_3210`, 0).
    Both,
}

/// How far channel 1's test pattern runs ahead of channel 0's.
const CHANNEL_1_OFFSET: u16 = 0x400;

/// How the front end packs a burst (`IFMT_*` and `IFMT_CH_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Packing {
    /// 12-bit lane values (`IFMT_12BIT`, 2) rather than 16-bit (`IFMT_16BIT`, 3).
    twelve_bit: bool,

    /// The lanes carried.
    lanes: Lanes,
}

impl Packing {
    /// The packing a burst-format value selects.
    ///
    /// # Panics
    ///
    /// For formats or lanes the sim does not model.
    fn from_burst_format(value: u32) -> Self {
        let lanes = match (value >> 2) & 0b111 {
            1 => Lanes::IAndQ,
            2 => Lanes::I,
            3 => Lanes::Q,
            5 => Lanes::Channel1,
            0 => Lanes::Both,
            lanes => panic!("RX lanes {lanes} are not modelled; only 0, 1, 2, 3 and 5 are"),
        };
        let twelve_bit = match value & 0b11 {
            3 => false,
            2 => true,
            format => panic!("RX sample format {format} is not modelled; only 12 and 16 bits"),
        };
        Self { twelve_bit, lanes }
    }

    /// Appends sample `n`'s lane values of the counting pattern to `values`.
    fn push_values(self, values: &mut Vec<u16>, n: u16) {
        let q = n.wrapping_neg();
        let n1 = n.wrapping_add(CHANNEL_1_OFFSET);
        let q1 = n1.wrapping_neg();
        match self.lanes {
            Lanes::IAndQ => values.extend([n, q]),
            Lanes::I => values.push(n),
            Lanes::Q => values.push(q),
            Lanes::Channel1 => values.extend([n1, q1]),
            Lanes::Both => values.extend([n, q, n1, q1]),
        }
    }

    /// Appends a burst's lane values, packed.
    fn pack(self, values: &[u16], block: &mut Vec<u8>) {
        if !self.twelve_bit {
            block.extend(values.iter().flat_map(|value| value.to_le_bytes()));
            return;
        }
        for pair in values.chunks(2) {
            let second = pair.get(1).map_or(0, |value| u32::from(value & 0xfff));
            let packed = u32::from(pair[0] & 0xfff) | second << 12;
            // A lone last value takes 12 of the 16 bits written.
            let bytes = if pair.len() == 2 { 3 } else { 2 };
            block.extend_from_slice(&packed.to_le_bytes()[..bytes]);
        }
    }
}

/// The RX stream engine's observable state.
#[derive(Debug, Default)]
pub struct RxEngine {
    /// The DMA runs.
    dma_running: bool,

    /// The front end runs.
    front_end_running: bool,

    /// Samples per burst.
    samples_per_burst: usize,

    /// Words per burst, padding included.
    words_per_burst: usize,

    /// How samples are packed.
    packing: Packing,

    /// Bursts per block.
    bursts: usize,

    /// The next sample of the test pattern.
    next_sample: u16,

    /// Blocks delivered so far.
    blocks: u64,
}

impl RxEngine {
    /// Follows a register write.
    ///
    /// # Panics
    ///
    /// For more bursts per block than the engine has, and for burst formats the module
    /// does not model.
    pub fn on_write(&mut self, addr: u32, value: u32) {
        match addr {
            REG_RX_DMA_CONTROL => match value {
                1 => self.dma_running = true,
                0 | 2 => self.dma_running = false,
                _ => {}
            },
            REG_RX_DMA_BURSTS => {
                let bursts = count(value) + 1;
                assert!(
                    bursts <= MAX_BURSTS,
                    "{bursts} bursts per block exceed the engine's {MAX_BURSTS}"
                );
                self.bursts = bursts;
            }
            REG_RX_FRONT_END => {
                let data = value & 0x0fff_ffff;
                match value >> 28 {
                    CMD_BURST_SAMPLES => self.samples_per_burst = count(data) + 1,
                    CMD_BURST_FORMAT => {
                        self.words_per_burst = count((data >> 5) & 0x1fff) + 1;
                        self.packing = Packing::from_burst_format(data);
                    }
                    CMD_CONTROL if data & RESET_BITS != 0 => self.front_end_running = false,
                    CMD_CONTROL => match data & 0b111 {
                        2 => self.front_end_running = true,
                        4 => self.front_end_running = false,
                        _ => {}
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Whether the DMA and front end both run: the RX activity the sync unit can wait for.
    #[must_use]
    pub const fn running(&self) -> bool {
        self.dma_running && self.front_end_running
    }

    /// The next block and its out-of-band words (no packets lost; the completed-burst
    /// mask; the DMA status with its block count), or `None` while the engine is stopped
    /// or the sync unit holds it (`released` false).
    ///
    /// # Panics
    ///
    /// When the configured samples overflow the configured burst, which neither driver
    /// configures.
    pub fn next_block(&mut self, released: bool) -> Option<(Vec<u8>, [u64; 2])> {
        if !(self.running() && released) {
            return None;
        }
        let burst_bytes = self.words_per_burst * WORD_BYTES;
        let mut block = Vec::with_capacity(burst_bytes * self.bursts);
        let mut values = Vec::new();
        for _ in 0..self.bursts {
            let start = block.len();
            values.clear();
            for _ in 0..self.samples_per_burst {
                self.packing.push_values(&mut values, self.next_sample);
                self.next_sample = self.next_sample.wrapping_add(1);
            }
            self.packing.pack(&values, &mut block);
            assert!(
                block.len() - start <= burst_bytes,
                "{} samples overflow a {burst_bytes}-byte burst",
                self.samples_per_burst
            );
            block.resize(start + burst_bytes, 0);
        }
        // One bit per burst, from the top.
        let mask = ((1_u64 << self.bursts) - 1) << (32 - self.bursts);
        self.blocks += 1;
        let status = (self.blocks % 64) << 8;
        Some((block, [mask << 32, status]))
    }
}

/// A register field as a count.
fn count(value: u32) -> usize {
    usize::try_from(value).expect("invariant: register fields fit in usize")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Burst format bits for 16-bit samples on channel 0.
    const FORMAT_16: u32 = 1 << 2 | 3;
    /// Burst format bits for 12-bit samples on channel 0.
    const FORMAT_12: u32 = 1 << 2 | 2;

    /// An engine configured as libusdr does for `samples` per burst, `words` per burst,
    /// `format` bits and `bursts` per block, then started.
    fn running(samples: u32, words: u32, format: u32, bursts: u32) -> RxEngine {
        let mut engine = RxEngine::default();
        engine.on_write(REG_RX_FRONT_END, 0x3000_e100);
        engine.on_write(REG_RX_FRONT_END, samples - 1);
        engine.on_write(REG_RX_FRONT_END, 0x3000_0000);
        engine.on_write(REG_RX_FRONT_END, 0x1000_0000 | (words - 1) << 5 | format);
        engine.on_write(REG_RX_DMA_BURSTS, bursts - 1);
        engine.on_write(REG_RX_DMA_CONTROL, 1);
        engine.on_write(REG_RX_FRONT_END, 0x3000_0002);
        engine
    }

    #[test]
    fn nothing_flows_while_the_sync_unit_holds_the_stream() {
        let mut engine = running(2, 1, FORMAT_16, 1);
        assert_eq!(engine.next_block(false), None);
        assert!(engine.next_block(true).is_some());
    }

    #[test]
    fn each_halting_write_stops_the_flow() {
        for (addr, value) in [
            (REG_RX_DMA_CONTROL, 0),
            (REG_RX_DMA_CONTROL, 2),
            (REG_RX_FRONT_END, 0x3000_0004),
            (REG_RX_FRONT_END, 0x3000_e100),
        ] {
            let mut engine = running(2, 1, FORMAT_16, 1);
            engine.on_write(addr, value);
            assert_eq!(engine.next_block(true), None, "{addr:#x} <- {value:#x}");
        }
    }

    #[test]
    fn the_host_ready_write_does_not_stop_the_dma() {
        let mut engine = running(2, 1, FORMAT_16, 1);
        engine.on_write(REG_RX_DMA_CONTROL, 4);
        assert!(engine.next_block(true).is_some());
    }

    #[test]
    fn blocks_hold_padded_bursts_of_the_counting_pattern() {
        // Two bursts of one sample, each padded to one 8-byte word.
        let mut engine = running(1, 1, FORMAT_16, 2);
        let (block, oob) = engine.next_block(true).expect("running");
        assert_eq!(
            block,
            [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0xff, 0xff, 0, 0, 0, 0]
        );
        assert_eq!(oob, [0xc000_0000 << 32, 1 << 8]);
    }

    #[test]
    fn the_dma_status_counts_delivered_blocks_modulo_64() {
        let mut engine = running(1, 1, FORMAT_16, 1);
        let counts: Vec<u64> = (0..65)
            .map(|_| engine.next_block(true).expect("running").1[1] >> 8)
            .collect();
        assert_eq!(counts[..2], [1, 2]);
        assert_eq!(counts[62..], [63, 0, 1], "the 6-bit count wraps");
    }

    #[test]
    fn twelve_bit_samples_pack_back_to_back_across_words() {
        // Three samples are 9 bytes, so two words: samples 0, 1 and 2, then a 7-byte stub.
        let mut engine = running(3, 2, FORMAT_12, 1);
        let (block, _) = engine.next_block(true).expect("running");
        assert_eq!(
            block,
            [
                0x00, 0x00, 0x00, // I 0, Q 0
                0x01, 0xf0, 0xff, // I 1, Q -1 (0xfff)
                0x02, 0xe0, 0xff, // I 2, Q -2 (0xffe)
                0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn real_lanes_carry_i_or_q_alone() {
        // Three 16-bit I samples, then three Q samples: 6 bytes in one word each.
        let mut engine = running(3, 1, 2 << 2 | 3, 1);
        let (block, _) = engine.next_block(true).expect("running");
        assert_eq!(block, [0, 0, 1, 0, 2, 0, 0, 0]);
        let mut engine = running(3, 1, 3 << 2 | 3, 1);
        let (block, _) = engine.next_block(true).expect("running");
        assert_eq!(block, [0, 0, 0xff, 0xff, 0xfe, 0xff, 0, 0]);
    }

    #[test]
    fn real_twelve_bit_values_pack_two_per_three_bytes() {
        // Three Q values, 0, -1 and -2: a pair in 3 bytes, the lone last in 12 bits.
        let mut engine = running(3, 1, 3 << 2 | 2, 1);
        let (block, _) = engine.next_block(true).expect("running");
        assert_eq!(block, [0x00, 0xf0, 0xff, 0xfe, 0x0f, 0, 0, 0]);
    }

    #[test]
    fn channel_1_rides_lanes_2_and_3_after_channel_0() {
        // One sample per burst: channel 1 alone is (0x400, -0x400); both channels carry
        // channel 0's (0, 0) first.
        let mut engine = running(1, 1, 5 << 2 | 3, 1);
        let (block, _) = engine.next_block(true).expect("running");
        assert_eq!(block, [0x00, 0x04, 0x00, 0xfc, 0, 0, 0, 0]);
        let mut engine = running(1, 1, 3, 1);
        let (block, _) = engine.next_block(true).expect("running");
        assert_eq!(block, [0, 0, 0, 0, 0x00, 0x04, 0x00, 0xfc]);
    }

    #[test]
    #[should_panic(expected = "not modelled")]
    fn an_unmodelled_sample_format_is_refused() {
        // `IFMT_8BIT`.
        running(2, 1, 1 << 2 | 1, 1);
    }
}
