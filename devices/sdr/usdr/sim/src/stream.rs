//! The FPGA's RX stream engine, as far as a driver can observe it: the registers that
//! start and size it, and the blocks it delivers.
//!
//! The engine runs once the DMA is enabled (register 5, value 1), the front end is started
//! (command register 0x10100, reset command with run state 2) and the sync unit free-runs
//! (register 14, mode 7). Each block it then delivers holds the configured number of
//! bursts; each burst the configured samples, interleaved little-endian 16-bit I and Q, then
//! zeros up to the burst's whole words. Register meanings: libusdr's
//! `ipblks/streams/sfe_rx_4.c`, `dma_rx_32.c` and `stream_sfetrx4_dma32.c`.
//!
//! Assumed, not modelled:
//! - Timing and overruns. A block is ready whenever the engine runs, without advancing
//!   virtual time, and none is ever lost.
//! - The signal. The samples are a counting test pattern, sample `n` reading `I = n`,
//!   `Q = -n` (wrapping).
//! - Formats. The burst format's sample format and lanes (bits 1:0 and 4:2) are ignored:
//!   both drivers configure only `ci16` on channel 0 (16-bit, lanes 1 and 0), which is what
//!   the engine produces.
//! - The DMA's own words-per-burst register (0x10020). Block sizes come from the front
//!   end's burst format instead; both drivers write the two consistently.
//! - The host-ready strobe (register 5, value 4). Blocks flow without it; libusdr writes
//!   it before every receive until the first succeeds, and so does rsl-usdr.

/// RX DMA control: 1 runs the DMA; 0 and 2 (stop and front-end reset) halt it; 4 (the host
/// is ready) leaves it as it is.
pub(crate) const REG_RX_DMA_CONTROL: u32 = 5;
/// The stream sync unit: bit 31 set and mode 7 in bits 19:16 free-run.
pub(crate) const REG_SYNC: u32 = 14;
/// RX DMA configuration: bursts per block, less one.
pub(crate) const REG_RX_DMA_BURSTS: u32 = 0x1_0040;
/// The RX front end's command register: command in bits 31:28, value below.
pub(crate) const REG_RX_FRONT_END: u32 = 0x1_0100;

/// Front-end command: samples per burst, less one.
const CMD_BURST_SAMPLES: u32 = 0;
/// Front-end command: burst format; words per burst, less one, in bits 17:5.
const CMD_BURST_FORMAT: u32 = 1;
/// Front-end command: reset and run state (bits 2:0: 2 start now, 4 stop now), plus block
/// resets (bits 8, 13, 14, 15: DSP, DDR, sample assembler, burster).
const CMD_CONTROL: u32 = 3;
/// The front end's block-reset bits (`SFE_CMD_RST_*_OFF` in `sfe_rx_4.c`).
const RESET_BITS: u32 = 1 << 8 | 1 << 13 | 1 << 14 | 1 << 15;
/// Most bursts in a block (`MAX_BURSTS_RX_IN_BUFF`).
const MAX_BURSTS: usize = 32;
/// Sync value that free-runs the streams.
const SYNC_FREE_RUN: u32 = 1 << 31 | 7 << 16;
/// Bytes per front-end word.
const WORD_BYTES: usize = 8;

/// The RX stream engine's observable state.
#[derive(Debug, Default)]
pub(crate) struct RxEngine {
    /// The DMA runs.
    dma_running: bool,

    /// The front end runs.
    front_end_running: bool,

    /// The sync unit free-runs.
    free_running: bool,

    /// Samples per burst.
    samples_per_burst: usize,

    /// Words per burst, padding included.
    words_per_burst: usize,

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
            REG_SYNC => self.free_running = value == SYNC_FREE_RUN,
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
                    CMD_BURST_FORMAT => self.words_per_burst = count((data >> 5) & 0x1fff) + 1,
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

    /// The next block and its out-of-band words (no packets lost; the completed-burst
    /// mask; a zero status), or `None` while the engine is stopped.
    pub(crate) fn next_block(&mut self) -> Option<(Vec<u8>, [u64; 2])> {
        if !(self.dma_running && self.front_end_running && self.free_running) {
            return None;
        }
        let burst_bytes = self.words_per_burst * WORD_BYTES;
        let mut block = Vec::with_capacity(burst_bytes * self.bursts);
        for _ in 0..self.bursts {
            let start = block.len();
            for _ in 0..self.samples_per_burst {
                let i = self.next_sample;
                block.extend_from_slice(&i.to_le_bytes());
                block.extend_from_slice(&i.wrapping_neg().to_le_bytes());
                self.next_sample = self.next_sample.wrapping_add(1);
            }
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

    /// An engine configured as libusdr does for `samples` per burst, `words` per burst and
    /// `bursts` per block, then started and free-running.
    fn running(samples: u32, words: u32, bursts: u32) -> RxEngine {
        let mut engine = RxEngine::default();
        engine.on_write(REG_RX_FRONT_END, 0x3000_e100);
        engine.on_write(REG_RX_FRONT_END, samples - 1);
        engine.on_write(REG_RX_FRONT_END, 0x3000_0000);
        engine.on_write(REG_RX_FRONT_END, 0x1000_0000 | (words - 1) << 5);
        engine.on_write(REG_RX_DMA_BURSTS, bursts - 1);
        engine.on_write(REG_SYNC, 0x8000_0000);
        engine.on_write(REG_RX_DMA_CONTROL, 1);
        engine.on_write(REG_RX_FRONT_END, 0x3000_0002);
        engine
    }

    #[test]
    fn nothing_flows_until_the_sync_unit_free_runs() {
        let mut engine = running(2, 1, 1);
        assert_eq!(engine.next_block(), None);
        engine.on_write(REG_SYNC, SYNC_FREE_RUN);
        assert!(engine.next_block().is_some());
    }

    #[test]
    fn each_halting_write_stops_the_flow() {
        for (addr, value) in [
            (REG_RX_DMA_CONTROL, 0),
            (REG_RX_DMA_CONTROL, 2),
            (REG_RX_FRONT_END, 0x3000_0004),
            (REG_RX_FRONT_END, 0x3000_e100),
            (REG_SYNC, 0x8000_0000),
        ] {
            let mut engine = running(2, 1, 1);
            engine.on_write(REG_SYNC, SYNC_FREE_RUN);
            engine.on_write(addr, value);
            assert_eq!(engine.next_block(), None, "{addr:#x} <- {value:#x}");
        }
    }

    #[test]
    fn the_host_ready_write_does_not_stop_the_dma() {
        let mut engine = running(2, 1, 1);
        engine.on_write(REG_SYNC, SYNC_FREE_RUN);
        engine.on_write(REG_RX_DMA_CONTROL, 4);
        assert!(engine.next_block().is_some());
    }

    #[test]
    fn blocks_hold_padded_bursts_of_the_counting_pattern() {
        // Two bursts of one sample, each padded to one 8-byte word.
        let mut engine = running(1, 1, 2);
        engine.on_write(REG_SYNC, SYNC_FREE_RUN);
        let (block, oob) = engine.next_block().expect("running");
        assert_eq!(
            block,
            [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0xff, 0xff, 0, 0, 0, 0]
        );
        assert_eq!(oob, [0xc000_0000 << 32, 0]);
    }
}
