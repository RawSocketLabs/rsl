//! The FPGA's TX stream core, original (`CORE_SFETX_DMA32_R0`) or extended
//! (`CORE_EXFETX_DMA32_R0_2`, which the board selects: the uSDR's hardware ID bit 26),
//! and the 32 DMA buffers the `PCIe` driver keeps for it, as far as a driver can observe
//! them. The uSDR and the LMS7 family pass the core the same registers
//! (`device/generic_usdr/generic_regs.h`).
//!
//! The host writes a packet into a free buffer, then announces it. The original core
//! takes two words at registers 12 and 13: samples less one in bits 29:15 of the first, no
//! timestamp in bit 30, the timestamp's bits 46:32 in bits 14:0 and its low 32 bits in the
//! second (`sfe_tx4_push_ring_buffer`). The extended core takes four at registers 28 to
//! 31: bytes less one, then bursts less one in bits 31:24 and samples less one below, then
//! the timestamp's high and low halves, all ones for "now"; the host writes the first
//! three only when they change, and the write to 31 announces the packet
//! (`sfe_extx4_push_ring_buffer`). Either core's commands arrive at register 14 with bit
//! 31 clear (`sfe_tx4_ctl`, `sfe_tx4_upd`): bits 1:0 stop (0), update flags (1) or start
//! 16-bit (3), and a start's bits 4:3 give the channel count as a power of two. Its four
//! status words read at registers 28 to 31. Register meanings: libusdr's
//! `ipblks/streams/sfe_tx_4.c` and `stream_sfetrx4_dma32.c`.
//!
//! The buffers model the `PCIe` driver (`usdr_pcie_uram.c`, read in the upstream tree at
//! the pinned commit, outside the vendored subset): set up, all 32 are free with no
//! completion record ("no OOB data for the first N buffs"); each one the core plays frees
//! it again with a record of the core's first three status words and the kernel's
//! nanosecond time (`usdr_stream_wait_or_alloc`). The sim's kernel time is its virtual
//! clock in nanoseconds, low 32 bits.
//!
//! Assumed, not modelled:
//! - Timing and underruns. An announced packet plays at once while the core runs and the
//!   sync unit releases it, whatever its timestamp; announced while not, it waits until
//!   then. Played packets are recorded whole, timestamps, mute and swap flags
//!   uninterpreted: the original core's packet is its samples times 4 bytes per channel
//!   of the last start, the extended core's the bytes it was told.
//! - The extended core's front end (`CSR_TFE4_BASE`: lane expansion, sample width, mute,
//!   lane shuffle). Its commands are not interpreted; a packet's bytes are recorded as
//!   the host wrote them. A packet of more than one burst is not modelled and panics.
//! - The status words. They read as the test sets them ([`TxEngine::set_status`]),
//!   zero by default; the completion record carries the same three words, an assumption
//!   (the driver copies its interrupt event registers, whose source is in the gateware).
//! - Resetting the buffer pointers (bit 7 of a command) and the start-12-bit command.
//!   libusdr sends the reset with its stop before each start; the sim keeps announced
//!   packets across it, and panics on the 12-bit start, which libusdr never sends this core.

use std::collections::VecDeque;

use crate::sync::{REG_SYNC, SYNC_SELECT};

/// The first word announcing a packet to the original core.
const REG_TX_PUSH: u32 = 12;
/// The first of the extended core's four announcing registers.
const REG_EXTENDED_PUSH: u32 = 28;
/// The extended core's last announcing register: a write announces the packet.
const REG_EXTENDED_ANNOUNCE: u32 = 31;
/// The second: the timestamp's low 32 bits.
const REG_TX_PUSH_TIME: u32 = 13;
/// The first of the core's four status words.
pub const REG_TX_STATUS: u32 = 28;
/// The `PCIe` driver's TX buffers.
const BUFFERS: usize = 32;
/// Wire bytes per `ci16` sample.
const SAMPLE_BYTES: usize = 4;

/// A free TX buffer the transport hands out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxGrant {
    /// One of the buffers the driver starts with: no completion record.
    Fresh,

    /// A buffer a played packet freed, with its completion record: the TX core's first
    /// three status words and the kernel time.
    Freed([u32; 4]),
}

impl TxGrant {
    /// The completion record, if the buffer has one.
    #[must_use]
    pub const fn record(self) -> Option<[u32; 4]> {
        match self {
            Self::Fresh => None,
            Self::Freed(words) => Some(words),
        }
    }
}

/// A packet the TX core played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxPacket {
    /// Its timestamp, or `None` to play on arrival.
    pub time: Option<u64>,

    /// Its wire bytes: `ci16` samples, I then Q, little-endian.
    pub bytes: Vec<u8>,
}

/// The TX core's and its buffers' observable state.
#[derive(Debug, Default)]
pub struct TxEngine {
    /// Bytes in each buffer.
    block_bytes: usize,

    /// Buffers the transport may hand out, oldest first, each with the completion record
    /// it carries, if any.
    free: VecDeque<TxGrant>,

    /// Buffers the host filled and has not announced yet, in order.
    filled: VecDeque<Vec<u8>>,

    /// Packets announced and not yet played.
    queued: VecDeque<TxPacket>,

    /// Packets played, in order.
    played: Vec<TxPacket>,

    /// The first announcing word, until the second arrives.
    push: Option<u32>,

    /// The core was started and not stopped since.
    running: bool,

    /// The channel count the last start announced, as a power of two (bits 4:3); 0, one
    /// channel, before any.
    channels_log2: u32,

    /// The core is the extended one, announced through registers 28 to 31.
    extended: bool,

    /// The extended core's first three announcing registers as last written.
    extended_words: [u32; 3],

    /// The four status words.
    status: [u32; 4],
}

impl TxEngine {
    /// The extended core (`CORE_EXFETX_DMA32_R0_2`), stopped and with no buffers.
    #[must_use]
    pub fn extended() -> Self {
        Self {
            extended: true,
            ..Self::default()
        }
    }

    /// Sets the four status words the core reads, which played buffers' completion
    /// records also carry; zero by default.
    pub const fn set_status(&mut self, words: [u32; 4]) {
        self.status = words;
    }

    /// Sets the buffers up, `block_bytes` each: all free, none with a record (the
    /// driver's `DMA_CONF`).
    pub fn open(&mut self, block_bytes: usize) {
        self.block_bytes = block_bytes;
        self.free = std::iter::repeat_n(TxGrant::Fresh, BUFFERS).collect();
        self.filled.clear();
        self.queued.clear();
        self.push = None;
    }

    /// Bytes in each buffer.
    #[must_use]
    pub const fn block_bytes(&self) -> usize {
        self.block_bytes
    }

    /// Releases the buffers (`DMA_UNCONF`).
    pub fn close(&mut self) {
        self.free.clear();
        self.filled.clear();
    }

    /// Hands out the next free buffer, or `None` while every buffer waits to be played (or
    /// none are set up).
    pub fn grant(&mut self) -> Option<TxGrant> {
        self.free.pop_front()
    }

    /// The status word a read of `addr` returns, if it is one of the four.
    #[must_use]
    pub fn status_word(&self, addr: u32) -> Option<u32> {
        let index = usize::try_from(addr.checked_sub(REG_TX_STATUS)?).ok()?;
        self.status.get(index).copied()
    }

    /// Takes the bytes the host wrote into the buffer it was last granted.
    pub fn load(&mut self, bytes: &[u8]) {
        self.filled.push_back(bytes.to_vec());
    }

    /// Whether the core runs: the TX activity the sync unit can wait for.
    #[must_use]
    pub const fn running(&self) -> bool {
        self.running
    }

    /// Follows a register write.
    ///
    /// # Panics
    ///
    /// On an announcement with no filled buffer, or one shorter than it says, and on the
    /// unmodelled 12-bit start: harness failures.
    pub fn on_write(&mut self, addr: u32, value: u32) {
        match addr {
            REG_EXTENDED_PUSH..REG_EXTENDED_ANNOUNCE if self.extended => {
                let index = usize::try_from(addr - REG_EXTENDED_PUSH).expect("0 to 2");
                self.extended_words[index] = value;
            }
            REG_EXTENDED_ANNOUNCE if self.extended => {
                let [bytes, bursts_samples, high] = self.extended_words;
                assert_eq!(
                    bursts_samples >> 24,
                    0,
                    "a TX packet of several bursts is not modelled"
                );
                let bytes = usize::try_from(bytes & 0x00ff_ffff).expect("24 bits") + 1;
                let stamp = u64::from(high) << 32 | u64::from(value);
                let time = (stamp != u64::MAX).then_some(stamp);
                self.announce(bytes, time);
            }
            REG_TX_PUSH => self.push = Some(value),
            REG_TX_PUSH_TIME => {
                let first = self
                    .push
                    .take()
                    .expect("the second announcing word follows the first");
                let samples = usize::try_from((first >> 15) & 0x7fff).expect("15 bits") + 1;
                let time = (first & 1 << 30 == 0)
                    .then(|| u64::from(first & 0x7fff) << 32 | u64::from(value));
                self.announce((samples * SAMPLE_BYTES) << self.channels_log2, time);
            }
            REG_SYNC if value & SYNC_SELECT == 0 => match value & 0b11 {
                0 => self.running = false,
                1 => {}
                3 => {
                    self.running = true;
                    self.channels_log2 = (value >> 3) & 0b11;
                }
                _ => panic!("the TX core's 12-bit start is not modelled"),
            },
            _ => {}
        }
    }

    /// Queues the next filled buffer's first `bytes` as a packet to play at `time`.
    ///
    /// # Panics
    ///
    /// With no filled buffer, or one shorter than `bytes`: harness failures.
    fn announce(&mut self, bytes: usize, time: Option<u64>) {
        let mut filled = self
            .filled
            .pop_front()
            .expect("a TX announcement names a buffer the host filled");
        assert!(
            filled.len() >= bytes,
            "{bytes} bytes announced, {} filled",
            filled.len()
        );
        filled.truncate(bytes);
        self.queued.push_back(TxPacket {
            time,
            bytes: filled,
        });
    }

    /// Plays every queued packet if the core runs and `released`, freeing its buffer with
    /// a completion record taken at `kernel_time`.
    pub fn play(&mut self, released: bool, kernel_time: u32) {
        if !(self.running && released) {
            return;
        }
        let [stat, stat_m, stat_ts, _] = self.status;
        while let Some(packet) = self.queued.pop_front() {
            self.played.push(packet);
            self.free
                .push_back(TxGrant::Freed([stat, stat_m, stat_ts, kernel_time]));
        }
    }

    /// Packets played so far.
    #[must_use]
    pub fn played(&self) -> &[TxPacket] {
        &self.played
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An engine set up and started, its first buffer granted and filled with `bytes`.
    fn started_with(bytes: &[u8]) -> TxEngine {
        let mut engine = TxEngine::default();
        engine.open(64);
        engine.on_write(REG_SYNC, 3);
        assert_eq!(
            engine.grant(),
            Some(TxGrant::Fresh),
            "the first buffers carry no record"
        );
        engine.load(bytes);
        engine
    }

    #[test]
    fn an_announced_packet_plays_with_its_time_and_samples() {
        let mut engine = started_with(&[1, 0, 2, 0, 3, 0, 4, 0, 9, 9]);
        // Two samples at time 0x1_0000_0005.
        engine.on_write(REG_TX_PUSH, 1 << 15 | 1);
        engine.on_write(REG_TX_PUSH_TIME, 5);
        engine.play(true, 0);
        assert_eq!(
            engine.played(),
            [TxPacket {
                time: Some(0x1_0000_0005),
                bytes: vec![1, 0, 2, 0, 3, 0, 4, 0],
            }]
        );
    }

    #[test]
    fn a_packet_for_now_has_no_time() {
        let mut engine = started_with(&[0; 4]);
        engine.on_write(REG_TX_PUSH, 1 << 30 | 0x7fff);
        engine.on_write(REG_TX_PUSH_TIME, u32::MAX);
        engine.play(true, 0);
        assert_eq!(engine.played()[0].time, None);
    }

    #[test]
    fn packets_wait_for_the_core_and_the_sync_unit() {
        let mut engine = started_with(&[0; 4]);
        engine.on_write(REG_SYNC, 0);
        engine.on_write(REG_TX_PUSH, 1 << 30);
        engine.on_write(REG_TX_PUSH_TIME, 0);
        engine.play(true, 0);
        assert!(engine.played().is_empty(), "stopped");
        engine.on_write(REG_SYNC, 0x103);
        engine.play(false, 0);
        assert!(engine.played().is_empty(), "held");
        engine.play(true, 0);
        assert_eq!(engine.played().len(), 1);
    }

    #[test]
    fn played_buffers_return_with_the_status_and_kernel_time() {
        let mut engine = started_with(&[0; 4]);
        engine.set_status([1, 2, 3, 4]);
        engine.on_write(REG_TX_PUSH, 1 << 30);
        engine.on_write(REG_TX_PUSH_TIME, 0);
        engine.play(true, 77);
        let records: Vec<_> = std::iter::from_fn(|| engine.grant()).collect();
        assert_eq!(records.len(), BUFFERS, "every buffer is free again");
        assert_eq!(records.last(), Some(&TxGrant::Freed([1, 2, 3, 77])));
        assert!(
            records[..BUFFERS - 1]
                .iter()
                .all(|grant| *grant == TxGrant::Fresh)
        );
    }

    #[test]
    fn a_two_channel_start_plays_eight_bytes_a_sample() {
        let mut engine = started_with(&(0..20).collect::<Vec<u8>>());
        // `TX_CMD_START_16BIT` with `MODE_FMT` 1: two channels.
        engine.on_write(REG_SYNC, 1 << 3 | 3);
        engine.on_write(REG_TX_PUSH, 1 << 30 | 1 << 15);
        engine.on_write(REG_TX_PUSH_TIME, 0);
        engine.play(true, 0);
        assert_eq!(engine.played()[0].bytes, (0..16).collect::<Vec<u8>>());
    }

    #[test]
    fn the_extended_core_plays_the_bytes_announced_with_its_last_registers() {
        let mut engine = TxEngine::extended();
        engine.open(64);
        engine.on_write(REG_SYNC, 3);
        for _ in 0..2 {
            assert!(engine.grant().is_some());
            engine.load(&[9; 12]);
        }
        // Six bytes, one sample, at time 0x2_0000_0007.
        for (reg, value) in [(28, 5), (29, 0), (30, 2), (31, 7)] {
            engine.on_write(reg, value);
        }
        // The same size again, now: only the timestamp's halves change.
        engine.on_write(30, u32::MAX);
        engine.on_write(31, u32::MAX);
        engine.play(true, 0);
        assert_eq!(
            engine.played(),
            [
                TxPacket {
                    time: Some(0x2_0000_0007),
                    bytes: vec![9; 6],
                },
                TxPacket {
                    time: None,
                    bytes: vec![9; 6],
                },
            ]
        );
    }

    #[test]
    #[should_panic(expected = "several bursts")]
    fn an_extended_packet_of_several_bursts_is_not_modelled() {
        let mut engine = TxEngine::extended();
        engine.on_write(29, 1 << 24);
        engine.on_write(31, 0);
    }

    #[test]
    fn sync_writes_are_not_tx_commands() {
        let mut engine = started_with(&[0; 4]);
        engine.on_write(REG_SYNC, SYNC_SELECT);
        assert!(engine.running(), "a sync stop leaves the core running");
    }
}
