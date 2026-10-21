//! The LimeSDR-mini's stream framing: libusdr's `ipblks/streams/stream_limesdr.c`, in
//! `ci16` on the wire and on the host, the one format it streams without conversion.
//!
//! The gateware moves samples in 4 KiB bursts, each a 16-byte header then 4080 bytes of
//! samples: 1020 `ci16` samples. A transport block is a whole number of bursts. RX
//! headers carry the burst's first sample time at byte 8 (`limesdr_data_hdr_t`'s
//! `counter`); TX headers also carry, at byte 0, the "ignore the time" flag (bit 4) and at
//! bytes 1 and 2 the burst's sample bytes.

/// One burst on the wire (`burst_size`).
pub(crate) const BURST_BYTES: u32 = 4096;

/// A burst's header (`LIMESDR_STREAM_HDR`).
pub(crate) const HEADER_BYTES: u32 = 16;

/// Bytes of one `ci16` sample, I then Q, each a little-endian `i16`.
const SAMPLE_BYTES: u32 = 4;

/// Samples in one burst (`burst_smpl`, also the stream's `block_samples`).
pub(crate) const BURST_SAMPLES: u32 = (BURST_BYTES - HEADER_BYTES) / SAMPLE_BYTES;

/// Samples a TX send counts in multiples of (`tx_sampl_c`, "rem 16 bytes for 16 bits").
pub(crate) const TX_SAMPLE_MULTIPLE: u32 = 16 / SAMPLE_BYTES;

/// A TX stream's packet when none is asked for (`32 * 1020`).
const TX_DEFAULT_SAMPLES: u32 = 32 * 1020;

/// The smallest TX packet (`pktsyms < 4096` is raised to it).
const TX_MIN_SAMPLES: u32 = 4096;

/// An RX stream's bursts per block for `samples` per packet (`create_limesdr_stream`):
/// the packet's bursts when it is whole bursts, else two fifths of them, "so extra
/// buffering would be 20% max", at least one. Computed without libusdr's 32-bit wrap of
/// `2 * pktsyms`, which only packets over 2^31 samples reach.
pub(crate) fn rx_bursts(samples: u32) -> u32 {
    let bursts = if samples % BURST_SAMPLES == 0 {
        samples / BURST_SAMPLES
    } else {
        u32::try_from(2 * u64::from(samples) / u64::from(BURST_SAMPLES) / 5)
            .expect("invariant: 2 * u32::MAX / 1020 / 5 fits")
    };
    if bursts == 0 { 1 } else { bursts }
}

/// A TX stream's bursts per block for `samples` per packet asked for
/// (`create_limesdr_stream`): 0 asks for the default packet, fewer than 4096 get 4096,
/// then as many bursts as hold it. `None` for a packet not a multiple of
/// [`TX_SAMPLE_MULTIPLE`] (libusdr's `-EINVAL`, "Transmit packet should be round to").
pub(crate) const fn tx_plan(samples: u32) -> Option<u32> {
    let samples = if samples == 0 {
        TX_DEFAULT_SAMPLES
    } else if samples < TX_MIN_SAMPLES {
        TX_MIN_SAMPLES
    } else {
        samples
    };
    if samples % TX_SAMPLE_MULTIPLE != 0 {
        return None;
    }
    Some(samples.div_ceil(BURST_SAMPLES))
}

/// Where a partly consumed block stands (`struct stream_partial_rx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Held {
    /// Samples taken from it (`wire_cons_smpl`).
    samples: u32,
    /// Bytes taken from it, headers included (`wire_cons_bytes`).
    bytes: u32,
}

/// An RX stream's deframing state (`stream_limesdr_t`'s RX fields).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RxDeframer {
    /// Host samples per packet (`host_pkt_symbs`).
    packet: u32,
    /// Bursts per block (`burst_count`).
    bursts: u32,
    /// The last burst's time, `u64::MAX` before the first (`blk_time_prev`).
    last_burst: u64,
    /// Samples lost before the current burst still to be zero-filled (`lag_remaining`).
    lag: u64,
    /// The block the caller holds, partly consumed (`outst`), if any.
    held: Option<Held>,
    /// Whole packets lost (`overruns`).
    overruns: u64,
}

impl RxDeframer {
    /// A stream of `packet` host samples per receive over blocks of `bursts` bursts.
    pub(crate) const fn new(packet: u32, bursts: u32) -> Self {
        Self {
            packet,
            bursts,
            last_burst: u64::MAX,
            lag: 0,
            held: None,
            overruns: 0,
        }
    }

    /// Bytes of one transport block.
    pub(crate) const fn block_bytes(&self) -> u32 {
        self.bursts * BURST_BYTES
    }

    /// Whole packets lost so far, as libusdr counts them: a burst whose time is not the
    /// last one's plus a burst's samples adds the gap's whole packets.
    pub(crate) const fn overruns(&self) -> u64 {
        self.overruns
    }

    /// Fills `host`'s first packet of samples (`_limestr_stream_recv`), taking them from
    /// `block`, which holds the partly consumed block from the last receive, if any, else
    /// is refilled by `fetch` (the transport's `recv_dma_wait`) as often as needed. Gaps
    /// between bursts' times are filled with zero samples, a gap's whole packets dropped.
    /// Returns the time of `host`'s first sample (`fsymtime`).
    ///
    /// `block` must be [`Self::block_bytes`] long and `host` at least a packet; a `fetch`
    /// failure leaves the samples written so far and loses no held block.
    pub(crate) fn receive<E>(
        &mut self,
        block: &mut [u8],
        host: &mut [[i16; 2]],
        mut fetch: impl FnMut(&mut [u8]) -> Result<(), E>,
    ) -> Result<u64, E> {
        let wire_packet = self.bursts * BURST_SAMPLES;
        let mut host_at = 0u32;
        let mut host_rem = self.packet;
        let mut burst_time = u64::MAX;
        let mut first = u64::MAX;
        loop {
            let (mut consumed, mut offset) = if let Some(held) = self.held.take() {
                burst_time = self.last_burst;
                (held.samples, held.bytes)
            } else {
                fetch(block)?;
                (0, 0)
            };
            loop {
                let mut burst_samples = BURST_SAMPLES;
                if offset % (BURST_BYTES) == 0 {
                    burst_time = counter(block, offset);
                    self.lag = if self.last_burst == u64::MAX {
                        0
                    } else {
                        burst_time
                            .wrapping_sub(u64::from(BURST_SAMPLES))
                            .wrapping_sub(self.last_burst)
                    };
                    self.last_burst = burst_time;
                    offset += HEADER_BYTES;
                } else {
                    burst_samples -= consumed % BURST_SAMPLES;
                }

                if self.lag != 0 {
                    let fill = if host_at == 0 {
                        let packet = u64::from(self.packet);
                        self.overruns += self.lag / packet;
                        self.lag %= packet;
                        first = burst_time.wrapping_sub(self.lag);
                        self.lag
                    } else {
                        self.lag.min(u64::from(host_rem))
                    };
                    let fill = u32::try_from(fill).expect("invariant: at most a packet");
                    host[index(host_at)..index(host_at + fill)].fill([0; 2]);
                    host_at += fill;
                    host_rem -= fill;
                    self.lag -= u64::from(fill);
                }

                let wire_rem = (wire_packet - consumed).min(burst_samples);
                let take = host_rem.min(wire_rem);
                if take == 0 {
                    break;
                }
                if host_at == 0 {
                    first = burst_time.wrapping_add(u64::from(consumed % BURST_SAMPLES));
                }
                let wire = &block[index(offset)..index(offset + take * SAMPLE_BYTES)];
                let out = &mut host[index(host_at)..index(host_at + take)];
                for (sample, bytes) in out.iter_mut().zip(wire.chunks_exact(4)) {
                    *sample = [
                        i16::from_le_bytes([bytes[0], bytes[1]]),
                        i16::from_le_bytes([bytes[2], bytes[3]]),
                    ];
                }
                offset += take * SAMPLE_BYTES;
                host_at += take;
                host_rem -= take;
                consumed += take;
                if host_rem == 0 || consumed == wire_packet {
                    break;
                }
            }
            if consumed != wire_packet {
                self.held = Some(Held {
                    samples: consumed,
                    bytes: offset,
                });
            }
            if host_rem == 0 {
                return Ok(first);
            }
        }
    }
}

/// The time in the header of the burst at `offset` (`*(uint64_t*)(dma_buf + 8 + off)`,
/// little-endian as libusdr's hosts read it).
fn counter(block: &[u8], offset: u32) -> u64 {
    let at = index(offset) + 8;
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&block[at..at + 8]);
    u64::from_le_bytes(bytes)
}

/// A 32-bit offset as a slice index.
const fn index(offset: u32) -> usize {
    offset as usize
}

/// Frames `samples` into `out` as `_limestr_stream_send` does: a burst per
/// [`BURST_SAMPLES`] samples, each header flagging `time` absent (bit 4 of byte 0),
/// holding the burst's sample bytes (bytes 1 and 2, little-endian) and its first
/// sample's time (bytes 8 to 15: `time` plus the samples before it, libusdr's `-1` plus
/// them without one). Returns the bytes written, which `out` must hold.
pub(crate) fn frame_tx(samples: &[[i16; 2]], time: Option<u64>, out: &mut [u8]) -> usize {
    let base = time.unwrap_or(u64::MAX);
    let mut at = 0;
    for (burst, chunk) in samples.chunks(index(BURST_SAMPLES)).enumerate() {
        let bytes = chunk.len() * index(SAMPLE_BYTES);
        let header = &mut out[at..at + index(HEADER_BYTES)];
        header.fill(0);
        header[0] = u8::from(time.is_none()) << 4;
        let [low, high, ..] = bytes.to_le_bytes();
        header[1] = low;
        header[2] = high;
        let before = u64::try_from(burst).expect("invariant: usize fits in u64");
        let first = base.wrapping_add(before * u64::from(BURST_SAMPLES));
        header[8..].copy_from_slice(&first.to_le_bytes());
        at += index(HEADER_BYTES);
        for (wire, sample) in out[at..at + bytes].chunks_exact_mut(4).zip(chunk) {
            wire[..2].copy_from_slice(&sample[0].to_le_bytes());
            wire[2..].copy_from_slice(&sample[1].to_le_bytes());
        }
        at += bytes;
    }
    at
}

/// The bytes [`frame_tx`] writes for `samples` samples.
pub(crate) const fn tx_frame_bytes(samples: usize) -> usize {
    samples.div_ceil(index(BURST_SAMPLES)) * index(HEADER_BYTES) + samples * index(SAMPLE_BYTES)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    use super::*;

    /// A sample time's low 16 bits.
    fn low(t: u64) -> u16 {
        let [low, high, ..] = t.to_le_bytes();
        u16::from_le_bytes([low, high])
    }

    /// A block of bursts whose times start at `times[i]`, each sample I the sample's time
    /// and Q its complement (low 16 bits).
    fn block(times: &[u64]) -> Vec<u8> {
        let mut block = std::vec![0; times.len() * 4096];
        for (burst, &time) in times.iter().enumerate() {
            let at = burst * 4096;
            block[at + 8..at + 16].copy_from_slice(&time.to_le_bytes());
            for (n, s) in (0..1020).zip((at + 16..).step_by(4)) {
                let t = low(time + n);
                block[s..s + 2].copy_from_slice(&t.to_le_bytes());
                block[s + 2..s + 4].copy_from_slice(&(!t).to_le_bytes());
            }
        }
        block
    }

    /// The sample of time `t`, as [`block`] writes it.
    fn sample(t: u64) -> [i16; 2] {
        let t = low(t);
        [
            i16::from_le_bytes(t.to_le_bytes()),
            i16::from_le_bytes((!t).to_le_bytes()),
        ]
    }

    #[test]
    fn packet_sizes_follow_libusdr() {
        assert_eq!(rx_bursts(1020), 1);
        assert_eq!(rx_bursts(3060), 3);
        assert_eq!(rx_bursts(4096), 1, "2 * 4096 / 1020 / 5");
        assert_eq!(rx_bursts(100_000), 39);
        assert_eq!(rx_bursts(1), 1, "at least one");
        assert_eq!(tx_plan(0), Some(32), "32 * 1020");
        assert_eq!(tx_plan(100), Some(5), "4096 samples");
        assert_eq!(tx_plan(8192), Some(9));
        assert_eq!(tx_plan(8194), None, "not a multiple of 4");
    }

    #[test]
    fn a_packet_spanning_blocks_takes_the_held_rest_first() {
        // Blocks of one burst, packets of 1500 samples: the first takes a whole burst and
        // 480 of the next, the second the next's other 540 and 960 of a third.
        let blocks = [block(&[0]), block(&[1020]), block(&[2040])];
        let mut fetched = blocks.iter();
        let mut deframer = RxDeframer::new(1500, 1);
        let mut held = [0u8; 4096];
        let mut host = [[0i16; 2]; 1500];
        let mut fetch = |buf: &mut [u8]| {
            buf.copy_from_slice(fetched.next().expect("a block"));
            Ok::<_, ()>(())
        };
        assert_eq!(deframer.receive(&mut held, &mut host, &mut fetch), Ok(0));
        assert!((0..).zip(host).all(|(t, s)| s == sample(t)));
        assert_eq!(deframer.receive(&mut held, &mut host, &mut fetch), Ok(1500));
        assert!((1500..).zip(host).all(|(t, s)| s == sample(t)));
    }

    #[test]
    fn a_gap_between_bursts_is_zero_filled_and_its_whole_packets_dropped() {
        // Packets of 1020 samples; the second burst comes 2600 samples late: two packets
        // lost, 560 zero samples, then the burst's first 460.
        let blocks = [block(&[0, 1020 + 2600, 2040 + 2600])];
        let mut fetched = blocks.iter();
        let mut deframer = RxDeframer::new(1020, 3);
        let mut held = [0u8; 3 * 4096];
        let mut host = [[0i16; 2]; 1020];
        let mut fetch = |buf: &mut [u8]| {
            buf.copy_from_slice(fetched.next().expect("a block"));
            Ok::<_, ()>(())
        };
        assert_eq!(deframer.receive(&mut held, &mut host, &mut fetch), Ok(0));
        host[0] = [1, 1];
        assert_eq!(
            deframer.receive(&mut held, &mut host, &mut fetch),
            Ok(3620 - 560),
            "the first sample's time is the burst's less the zeroes"
        );
        assert_eq!(deframer.overruns(), 2);
        assert!(host[..560].iter().all(|&s| s == [0, 0]));
        assert!((3620..).zip(&host[560..]).all(|(t, &s)| s == sample(t)));
    }

    #[test]
    fn a_failed_fetch_keeps_nothing_held() {
        let mut deframer = RxDeframer::new(1020, 1);
        let mut held = [0u8; 4096];
        let mut host = [[0i16; 2]; 1020];
        assert_eq!(deframer.receive(&mut held, &mut host, |_| Err(7)), Err(7));
        assert_eq!(deframer, RxDeframer::new(1020, 1));
    }

    #[test]
    fn tx_frames_carry_the_flag_length_and_each_bursts_time() {
        let samples = [[1i16, -1]; 1030];
        let mut out = [0xaa; 2 * 4096];
        let written = frame_tx(&samples, Some(100), &mut out);
        assert_eq!(written, tx_frame_bytes(1030));
        assert_eq!(written, 16 + 4080 + 16 + 40);
        assert_eq!(
            &out[..16],
            &[0, 0xf0, 0x0f, 0, 0, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(&out[16..20], &[1, 0, 0xff, 0xff]);
        let second = &out[4096..4112];
        assert_eq!(&second[..3], &[0, 40, 0]);
        assert_eq!(&second[8..], &(100u64 + 1020).to_le_bytes());

        let written = frame_tx(&samples[..4], None, &mut out);
        assert_eq!(written, 32);
        assert_eq!(out[0], 0x10, "the time is ignored");
        assert_eq!(&out[8..16], &u64::MAX.to_le_bytes(), "libusdr's -1");
    }
}
