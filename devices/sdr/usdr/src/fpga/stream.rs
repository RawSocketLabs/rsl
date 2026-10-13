//! The RX stream engine's arithmetic: how a stream's packets split into bursts and fill the
//! front end's FIFO, and the burst-format word that says so. The engine's registers are
//! written by the portable machines in `portable/stream.rs`, from what [`BurstPlan`] plans.
//!
//! Source: `ipblks/streams/sfe_rx_4.c` and `stream_sfetrx4_dma32.c`; `SRF4_FIFOBSZ` from
//! `device/m2_lm6_1/m2_lm6_1.c`.

use bnb::{BitEnum, bitfield, u2, u3, u10, u13};

use crate::portable::stream::RxFormat;
use crate::stream::{RxChannels, WireFormat};

/// The front end's FIFO RAM, in bytes (`SRF4_FIFOBSZ`).
const FIFO_BYTES: u32 = 0x1_0000;
/// Bytes per front-end word (`cfg_word_bytes` for `CORE_SFERX_DMA32_R0`).
const WORD_BYTES: u32 = 8;
/// Most bursts in a DMA block (`MAX_BURSTS_RX_IN_BUFF`).
const MAX_BURSTS: u32 = 32;
/// Most words in a burst, and most samples (`1 << IPBLK_PARAM_BWORDS`).
const MAX_BURST_WORDS: u32 = 1 << 13;
/// Most bursts the FIFO count field holds (`SFE_CMD_BF_BTOTAL_MASK`).
const MAX_FIFO_BURSTS: u32 = (1 << 10) - 1;

/// How a stream's packets split into bursts and fit the front end's FIFO
/// (`burst_fe_calculate` for one complex channel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BurstPlan {
    /// The format the front end packs.
    wire: WireFormat,

    /// Bytes in one burst, padded to whole words.
    bytes_per_burst: u32,

    /// Bursts in one DMA block.
    bursts: u32,

    /// Samples in one burst.
    samples_per_burst: u32,

    /// Bursts the FIFO holds.
    fifo_bursts: u32,
}

impl BurstPlan {
    /// The plan for packets of `samples` in `wire` format, or `None` where libusdr returns
    /// `-EINVAL` (or would divide by zero, for packets too small to fill a word per burst).
    ///
    /// libusdr's search: the fewest bursts that divide the packet evenly, keep each burst
    /// within 8192 samples and words, leave room for two bursts in the FIFO and split the
    /// words evenly; failing an even split, the one with the least padding. Its FIFO count
    /// comes from the last burst count it tried, as here. Samples pack back to back across
    /// words: one or two lanes of 16 or 12 bits each (`bps = chans_raw * ch_bits`).
    pub(crate) fn for_packet(samples: u32, wire: WireFormat) -> Option<Self> {
        // libusdr's 32-bit product would wrap here; no such packet fits a burst anyway.
        let words = wire
            .sample_bits()
            .checked_mul(samples)?
            .div_ceil(WORD_BYTES * 8);
        if words == 0 {
            return None;
        }
        let mut fifo_bursts = FIFO_BYTES / (words * WORD_BYTES);
        let mut best: Option<(u32, u32)> = None;
        let mut even = None;
        for bursts in 1..=MAX_BURSTS {
            if samples % bursts != 0 {
                continue;
            }
            let burst_words = words / bursts;
            if burst_words == 0 {
                return None;
            }
            fifo_bursts = FIFO_BYTES / (burst_words * WORD_BYTES);
            if fifo_bursts <= 1
                || samples / bursts > MAX_BURST_WORDS
                || burst_words > MAX_BURST_WORDS
            {
                continue;
            }
            if words % bursts == 0 {
                even = Some(bursts);
                break;
            }
            let padding = words.div_ceil(bursts) * bursts - words;
            if best.is_none_or(|(_, least)| padding < least) {
                best = Some((bursts, padding));
            }
        }
        let bursts = even.or(best.map(|(bursts, _)| bursts))?;
        Some(Self {
            wire,
            bytes_per_burst: words.div_ceil(bursts) * WORD_BYTES,
            bursts,
            samples_per_burst: samples / bursts,
            fifo_bursts: fifo_bursts.min(MAX_FIFO_BURSTS),
        })
    }

    /// Samples in one burst.
    pub(crate) const fn samples_per_burst(&self) -> u32 {
        self.samples_per_burst
    }

    /// Bytes in one burst, padding included: a burst whose samples do not fill whole words
    /// ends in a stub.
    pub(crate) const fn bytes_per_burst(&self) -> u32 {
        self.bytes_per_burst
    }

    /// Bytes in one DMA block: what the transport delivers per packet.
    pub(crate) const fn block_bytes(&self) -> u32 {
        self.bytes_per_burst * self.bursts
    }

    /// What the front end and DMA take for this plan on the lanes of `channels`
    /// (`_configure_simple_fe_generic` for one channel, `dma_rx32_configure`).
    pub(crate) fn rx_format(&self, channels: RxChannels) -> RxFormat {
        let words = self.bytes_per_burst / WORD_BYTES;
        let format = BurstFormat::new()
            .with_fifo_bursts(u10::new(
                u16::try_from(self.fifo_bursts).expect("invariant: at most 1023"),
            ))
            .with_words(u13::new(
                u16::try_from(words - 1).expect("invariant: at most 8191"),
            ))
            .with_lanes(Lanes::for_channel(self.wire, channels))
            .with_format(self.wire.into());
        RxFormat {
            samples_per_burst: self.samples_per_burst,
            words_per_burst: words,
            bursts: self.bursts,
            burst_format: format.to_raw(),
        }
    }
}

/// The sample format the front end packs (`IFMT_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum SampleFormat {
    /// DSP output.
    Dsp,

    /// 8 bits.
    Bits8,

    /// 12 bits.
    Bits12,

    /// 16 bits.
    Bits16,
}

impl From<WireFormat> for SampleFormat {
    /// `sfe_rx4_configure`'s `fe_format` for the wire format's bits.
    fn from(wire: WireFormat) -> Self {
        match wire {
            WireFormat::Ci16 | WireFormat::I16 => Self::Bits16,
            WireFormat::Ci12 | WireFormat::I12 => Self::Bits12,
        }
    }
}

/// Which raw lanes a burst carries (`IFMT_CH_*`); lane 0 is I and lane 1 Q of channel 0.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
enum Lanes {
    /// All four.
    Lanes3210,

    /// Lanes 1 and 0: channel 0's I and Q.
    Lanes10,

    /// Lane 0.
    Lane0,

    /// Lane 1.
    Lane1,

    /// Lanes 2 and 0.
    Lanes20,

    /// Lanes 3 and 2.
    Lanes32,

    /// Lane 2.
    Lane2,

    /// Lane 3.
    Lane3,
}

impl Lanes {
    /// The lanes `sfe_rx4_configure` selects for one channel of `wire`: complex channel 0
    /// is lanes 1 and 0; real channel 0 lane 0, real channel 1 lane 1. Complex channel 1
    /// is refused before any bus access.
    fn for_channel(wire: WireFormat, channels: RxChannels) -> Self {
        match (wire.is_complex(), channels) {
            (true, _) => Self::Lanes10,
            (false, RxChannels::Channel0) => Self::Lane0,
            (false, RxChannels::Channel1) => Self::Lane1,
        }
    }
}

/// The burst format (`FE_CMD_BURST_FORMAT`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BurstFormat {
    /// Bursts the FIFO holds. `BTOTAL`, bits 27:18.
    #[bits(18..=27)]
    fifo_bursts: u10,

    /// Words per burst, less one. `BWORDS`, bits 17:5.
    #[bits(5..=17)]
    words: u13,

    /// The lanes. `CHFMT`, bits 4:2.
    #[bits(2..=4)]
    lanes: Lanes,

    /// The sample format. `IFMT`, bits 1:0.
    #[bits(0..=1)]
    format: SampleFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values: libusdr's arithmetic worked by hand.

    #[test]
    fn a_1024_sample_packet_is_one_4096_byte_burst() {
        let plan = BurstPlan::for_packet(1024, WireFormat::Ci16).expect("valid");
        assert_eq!(
            plan,
            BurstPlan {
                wire: WireFormat::Ci16,
                bytes_per_burst: 4096,
                bursts: 1,
                samples_per_burst: 1024,
                fifo_bursts: 16,
            }
        );
        assert_eq!(plan.block_bytes(), 4096);
    }

    #[test]
    fn a_packet_too_big_for_one_burst_splits_evenly() {
        // 16384 samples are 8192 words: one burst would leave no FIFO room for two.
        let plan = BurstPlan::for_packet(16384, WireFormat::Ci16).expect("valid");
        assert_eq!(
            (plan.bursts, plan.samples_per_burst, plan.bytes_per_burst),
            (2, 8192, 32768)
        );
    }

    #[test]
    fn small_packets_get_a_deep_fifo() {
        // 256 samples are 128 words: 64 bursts fit the FIFO, past the bottom 24 bits.
        let plan = BurstPlan::for_packet(256, WireFormat::Ci16).expect("valid");
        assert_eq!((plan.bursts, plan.fifo_bursts), (1, 64));
    }

    #[test]
    fn a_packet_that_cannot_split_evenly_pads_each_burst() {
        // 16383 = 3 × 43 × 127: three bursts of 5461 samples, each 2730.5 words, so 2731.
        let plan = BurstPlan::for_packet(16383, WireFormat::Ci16).expect("valid");
        assert_eq!(
            (plan.bursts, plan.samples_per_burst, plan.bytes_per_burst),
            (3, 5461, 21848)
        );
    }

    #[test]
    fn twelve_bit_samples_pack_three_bytes_each_across_words() {
        // 1024 samples of 24 bits are 384 words: one burst, 3072 bytes, 21 in the FIFO.
        let plan = BurstPlan::for_packet(1024, WireFormat::Ci12).expect("valid");
        assert_eq!(
            (plan.bursts, plan.bytes_per_burst, plan.fifo_bursts),
            (1, 3072, 21)
        );
        // 1023 samples are 3069 bytes, so the burst ends in a 3-byte stub.
        let plan = BurstPlan::for_packet(1023, WireFormat::Ci12).expect("valid");
        assert_eq!((plan.bursts, plan.bytes_per_burst), (1, 3072));
    }

    #[test]
    fn a_twelve_bit_packet_splits_by_its_own_word_count() {
        // 16384 samples are 6144 words: one burst fits 8192 words but leaves no FIFO room
        // for two (65536 / 49152 bytes), so two bursts of 3072 words.
        let plan = BurstPlan::for_packet(16384, WireFormat::Ci12).expect("valid");
        assert_eq!(
            (plan.bursts, plan.samples_per_burst, plan.bytes_per_burst),
            (2, 8192, 24576)
        );
    }

    #[test]
    fn a_packet_too_large_to_count_is_refused() {
        assert_eq!(BurstPlan::for_packet(1 << 27, WireFormat::Ci16), None);
    }

    #[test]
    fn the_front_end_words_are_libusdrs() {
        let plan = BurstPlan::for_packet(1024, WireFormat::Ci16).expect("valid");
        assert_eq!(
            plan.rx_format(RxChannels::Channel0),
            RxFormat {
                samples_per_burst: 1024,
                words_per_burst: 512,
                bursts: 1,
                // 16 bursts in the FIFO, 511 words, lanes 1 and 0, 16 bits.
                burst_format: 0x0040_3fe7,
            }
        );
        // `IFMT_12BIT` is 2 in bits 1:0.
        let format = BurstFormat::new()
            .with_fifo_bursts(u10::new(16))
            .with_words(u13::new(511))
            .with_lanes(Lanes::Lanes10)
            .with_format(WireFormat::Ci12.into());
        assert_eq!(format.to_raw(), 0x0040_3fe6);
    }
}
