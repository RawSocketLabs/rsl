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
//! lanes; its lanes (bits 4:2) must be 1, channel 0's I and Q on lanes 0 and 1, the only
//! ones the uSDR's single receiver drives. Others are not modelled and panic.
//! - 16-bit: each sample is I then Q, little-endian `i16`s, 4 bytes.
//! - 12-bit: assumed, from the C that reads it rather than gateware source (none is
//!   vendored), that each sample is 24 bits, I in bits 11:0 and Q in bits 23:12, stored as
//!   three little-endian bytes, samples back to back across words. Evidence:
//!   `sfe_rx4_configure` sizes a burst as `2 lanes × 12 bits` per sample over 64-bit words
//!   (`burst_fe_calculate`), with no per-sample alignment, and libusdr's `ci12` host
//!   formats unpack exactly that layout (`xdsp/conv.c`'s 12-bit kernels, `rsl_usdr::dsp`).
//!
//! Assumed, not modelled:
//! - Timing and overruns. A block is ready whenever the engine runs, without advancing
//!   virtual time, and none is ever lost.
//! - The signal. The samples are a counting test pattern in the lanes' own width: sample
//!   `n` reads `I = n`, `Q = -n`, wrapping at 16 or 12 bits.
//! - The DMA's own words-per-burst register (0x10020). Block sizes come from the front
//!   end's burst format instead; both drivers write the two consistently.
//! - The host-ready strobe (register 5, value 4). Blocks flow without it; libusdr writes
//!   it before every receive until the first succeeds, and so does rsl-usdr.

/// RX DMA control: 1 runs the DMA; 0 and 2 (stop and front-end reset) halt it; 4 (the host
/// is ready) leaves it as it is.
pub(crate) const REG_RX_DMA_CONTROL: u32 = 5;
/// RX DMA configuration: bursts per block, less one.
pub(crate) const REG_RX_DMA_BURSTS: u32 = 0x1_0040;
/// The RX front end's command register: command in bits 31:28, value below.
pub(crate) const REG_RX_FRONT_END: u32 = 0x1_0100;

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
/// Burst-format lanes: channel 0's I and Q on lanes 0 and 1 (`IFMT_CH_xx10`).
const LANES_CHANNEL_0: u32 = 1;

/// How the front end packs a sample (`IFMT_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Packing {
    /// Two 16-bit lanes, 4 bytes (`IFMT_16BIT`, 3).
    #[default]
    Bits16,

    /// Two 12-bit lanes, 3 bytes (`IFMT_12BIT`, 2).
    Bits12,
}

impl Packing {
    /// The packing a burst-format value selects.
    ///
    /// # Panics
    ///
    /// For formats or lanes the sim does not model.
    fn from_burst_format(value: u32) -> Self {
        let lanes = (value >> 2) & 0b111;
        assert_eq!(
            lanes, LANES_CHANNEL_0,
            "RX lanes {lanes} are not modelled; only channel 0's I and Q (1) are"
        );
        match value & 0b11 {
            3 => Self::Bits16,
            2 => Self::Bits12,
            format => panic!("RX sample format {format} is not modelled; only 12 and 16 bits"),
        }
    }

    /// Appends sample `n` of the counting pattern.
    fn push(self, block: &mut Vec<u8>, n: u16) {
        let q = n.wrapping_neg();
        match self {
            Self::Bits16 => {
                block.extend_from_slice(&n.to_le_bytes());
                block.extend_from_slice(&q.to_le_bytes());
            }
            Self::Bits12 => {
                let packed = u32::from(n & 0xfff) | u32::from(q & 0xfff) << 12;
                block.extend_from_slice(&packed.to_le_bytes()[..3]);
            }
        }
    }
}

/// The RX stream engine's observable state.
#[derive(Debug, Default)]
pub(crate) struct RxEngine {
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
}

impl RxEngine {
    /// Follows a register write.
    pub(crate) fn on_write(&mut self, addr: u32, value: u32) {
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
    pub(crate) const fn running(&self) -> bool {
        self.dma_running && self.front_end_running
    }

    /// The next block and its out-of-band words (no packets lost; the completed-burst
    /// mask; a zero status), or `None` while the engine is stopped or the sync unit holds
    /// it (`released` false).
    pub(crate) fn next_block(&mut self, released: bool) -> Option<(Vec<u8>, [u64; 2])> {
        if !(self.running() && released) {
            return None;
        }
        let burst_bytes = self.words_per_burst * WORD_BYTES;
        let mut block = Vec::with_capacity(burst_bytes * self.bursts);
        for _ in 0..self.bursts {
            let start = block.len();
            for _ in 0..self.samples_per_burst {
                self.packing.push(&mut block, self.next_sample);
                self.next_sample = self.next_sample.wrapping_add(1);
            }
            assert!(
                block.len() - start <= burst_bytes,
                "{} samples overflow a {burst_bytes}-byte burst",
                self.samples_per_burst
            );
            block.resize(start + burst_bytes, 0);
        }
        // One bit per burst, from the top.
        let mask = ((1_u64 << self.bursts) - 1) << (32 - self.bursts);
        Some((block, [mask << 32, 0]))
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
        assert_eq!(oob, [0xc000_0000 << 32, 0]);
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
    #[should_panic(expected = "not modelled")]
    fn an_unmodelled_sample_format_is_refused() {
        // `IFMT_8BIT`.
        running(2, 1, 1 << 2 | 1, 1);
    }
}
