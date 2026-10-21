//! The streams' sample formats: what the FPGA packs on the wire ([`WireFormat`]) and the
//! host buffers' elements ([`HostSample`]), converted by libusdr's converters
//! (`xdsp/conv.c`, `sdr-formats`'s [`Converter`]) as `stream_sfetrx4_dma32.c` selects
//! them.

use sdr_formats::{Converter, SampleFormat};

/// Samples converted at a time through a stack scratch per channel.
const CHUNK_SAMPLES: usize = 512;

/// Most bytes one host element takes (`cf32`).
const MAX_HOST_SAMPLE_BYTES: usize = 8;

/// Most channels a stream carries.
const MAX_CHANNELS: usize = 2;

/// A sample format the FPGA's stream cores pack, little-endian: complex (one channel's I
/// then Q, channels in order) or real (one lane). The RX front end (`sfe_rx_4`) packs all
/// four; the original TX core (`sfe_tx_4`) takes `Ci16` only, the extended one `Ci16` or
/// `Ci12`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WireFormat {
    /// 16 bits per value, 4 bytes per sample (`ci16`, `IFMT_16BIT`).
    #[default]
    Ci16,

    /// 12 bits per value, packed back to back, 3 bytes per sample (`ci12`, `IFMT_12BIT`).
    Ci12,

    /// Real: one lane's 16-bit values, 2 bytes per sample (`i16`).
    I16,

    /// Real: one lane's 12-bit values, two samples per 3 bytes (`i12`).
    I12,
}

impl WireFormat {
    /// Bits one sample (I and Q, or one lane) takes on the wire.
    pub(super) const fn sample_bits(self) -> u32 {
        match self {
            Self::Ci16 => 32,
            Self::Ci12 => 24,
            Self::I16 => 16,
            Self::I12 => 12,
        }
    }

    /// Whether a sample is complex, I and Q, rather than one real lane.
    pub(super) const fn is_complex(self) -> bool {
        matches!(self, Self::Ci16 | Self::Ci12)
    }

    /// The same format as libusdr's converters name it.
    pub(super) const fn sample_format(self) -> SampleFormat {
        match self {
            Self::Ci16 => SampleFormat::Ci16,
            Self::Ci12 => SampleFormat::Ci12,
            Self::I16 => SampleFormat::I16,
            Self::I12 => SampleFormat::I12,
        }
    }
}

/// A host buffer element, one sample of one channel: `[i16; 2]` (`ci16`, I then Q, full
/// scale ±32767), `[f32; 2]` (`cf32`, full scale ±1.0), `[u8; 3]` (`ci12`, packed as on
/// the wire), `i16` (`i16`) or `f32` (`f32`). A real `i12` host has no per-sample element,
/// so it is not offered.
pub trait HostSample: Copy + sealed::Sealed {
    /// The format this element holds.
    const FORMAT: SampleFormat;
}

impl HostSample for [i16; 2] {
    const FORMAT: SampleFormat = SampleFormat::Ci16;
}

impl HostSample for [f32; 2] {
    const FORMAT: SampleFormat = SampleFormat::Cf32;
}

impl HostSample for [u8; 3] {
    const FORMAT: SampleFormat = SampleFormat::Ci12;
}

impl HostSample for i16 {
    const FORMAT: SampleFormat = SampleFormat::I16;
}

impl HostSample for f32 {
    const FORMAT: SampleFormat = SampleFormat::F32;
}

/// Whether `host` has a [`HostSample`] element: every format but the real `i12`.
pub(super) const fn has_element(host: SampleFormat) -> bool {
    !matches!(host, SampleFormat::I12)
}

/// Keeps [`HostSample`] to the formats the converters produce, and holds how each element
/// reads from and writes to converted bytes.
mod sealed {
    /// A [`HostSample`](super::HostSample) element as the converters read and write it.
    pub trait Sealed: Sized {
        /// Bytes one element takes.
        const BYTES: usize;

        /// The element in `bytes`, exactly [`Self::BYTES`] little-endian bytes.
        fn from_le(bytes: &[u8]) -> Self;

        /// Writes the element into `bytes`, exactly [`Self::BYTES`] long, little-endian.
        fn to_le(&self, bytes: &mut [u8]);
    }

    impl Sealed for [i16; 2] {
        const BYTES: usize = 4;

        fn from_le(bytes: &[u8]) -> Self {
            [
                i16::from_le_bytes([bytes[0], bytes[1]]),
                i16::from_le_bytes([bytes[2], bytes[3]]),
            ]
        }

        fn to_le(&self, bytes: &mut [u8]) {
            bytes[..2].copy_from_slice(&self[0].to_le_bytes());
            bytes[2..4].copy_from_slice(&self[1].to_le_bytes());
        }
    }

    impl Sealed for [f32; 2] {
        const BYTES: usize = 8;

        fn from_le(bytes: &[u8]) -> Self {
            [
                f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            ]
        }

        fn to_le(&self, bytes: &mut [u8]) {
            bytes[..4].copy_from_slice(&self[0].to_le_bytes());
            bytes[4..8].copy_from_slice(&self[1].to_le_bytes());
        }
    }

    impl Sealed for [u8; 3] {
        const BYTES: usize = 3;

        fn from_le(bytes: &[u8]) -> Self {
            [bytes[0], bytes[1], bytes[2]]
        }

        fn to_le(&self, bytes: &mut [u8]) {
            bytes[..3].copy_from_slice(self);
        }
    }

    impl Sealed for i16 {
        const BYTES: usize = 2;

        fn from_le(bytes: &[u8]) -> Self {
            Self::from_le_bytes([bytes[0], bytes[1]])
        }

        fn to_le(&self, bytes: &mut [u8]) {
            bytes[..2].copy_from_slice(&self.to_le_bytes());
        }
    }

    impl Sealed for f32 {
        const BYTES: usize = 4;

        fn from_le(bytes: &[u8]) -> Self {
            Self::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }

        fn to_le(&self, bytes: &mut [u8]) {
            bytes[..4].copy_from_slice(&self.to_le_bytes());
        }
    }
}

/// Converts `wire`, whole samples of every channel, into `channels` from `first` on, at
/// most [`CHUNK_SAMPLES`] at a time through a stack scratch per channel: the chunked
/// form of libusdr's one-call conversion. `wire_bits` is one sample of every channel's
/// width.
pub(super) fn from_wire<S: HostSample>(
    converter: &Converter,
    wire: &[u8],
    wire_bits: usize,
    channels: &mut [&mut [S]],
    first: usize,
    samples: usize,
) {
    const { assert!(S::BYTES <= MAX_HOST_SAMPLE_BYTES) };
    let mut scratch = [[0; CHUNK_SAMPLES * MAX_HOST_SAMPLE_BYTES]; MAX_CHANNELS];
    let wire_bytes = |samples: usize| samples * wire_bits / 8;
    let chunks = (0..samples).step_by(CHUNK_SAMPLES);
    for (start, bytes) in chunks.zip(wire.chunks(wire_bytes(CHUNK_SAMPLES))) {
        let len = CHUNK_SAMPLES.min(samples - start);
        let bytes = &bytes[..wire_bytes(len)];
        let mut hosts = scratch.each_mut().map(|host| &mut host[..len * S::BYTES]);
        converter
            .convert(&[bytes], &mut hosts[..channels.len()])
            .expect("invariant: the converter takes one buffer in, one per channel out");
        for (channel, host) in channels.iter_mut().zip(&hosts) {
            let out = &mut channel[first + start..first + start + len];
            for (sample, raw) in out.iter_mut().zip(host.chunks_exact(S::BYTES)) {
                *sample = S::from_le(raw);
            }
        }
    }
}

/// Converts `channels`' samples into `wire`, exactly their wire size, at most
/// [`CHUNK_SAMPLES`] at a time through a stack scratch per channel: the chunked form of
/// libusdr's one-call conversion of the packet.
pub(super) fn to_wire<S: HostSample>(converter: &Converter, channels: &[&[S]], wire: &mut [u8]) {
    const { assert!(S::BYTES <= MAX_HOST_SAMPLE_BYTES) };
    let mut scratch = [[0; CHUNK_SAMPLES * MAX_HOST_SAMPLE_BYTES]; MAX_CHANNELS];
    let samples = channels.first().map_or(0, |channel| channel.len());
    let Some(wire_sample_bytes) = wire.len().checked_div(samples) else {
        return;
    };
    let outs = wire.chunks_mut(CHUNK_SAMPLES * wire_sample_bytes);
    for (start, out) in (0..samples).step_by(CHUNK_SAMPLES).zip(outs) {
        let len = CHUNK_SAMPLES.min(samples - start);
        for (channel, host) in channels.iter().zip(&mut scratch) {
            let chunk = &channel[start..start + len];
            for (sample, raw) in chunk.iter().zip(host.chunks_exact_mut(S::BYTES)) {
                sample.to_le(raw);
            }
        }
        let hosts = scratch.each_ref().map(|host| &host[..len * S::BYTES]);
        converter
            .convert(&hosts[..channels.len()], &mut [out])
            .expect("invariant: the converter takes one buffer per channel in, one out");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_conversion_equals_one_call_both_ways() {
        let samples = CHUNK_SAMPLES * 2 + 7;
        let wire: Vec<u8> = (0..samples * 6)
            .map(|n| n.to_le_bytes()[0].wrapping_mul(7))
            .collect();
        let split = Converter::new(SampleFormat::Ci12, SampleFormat::Cf32, 1, 2).expect("ported");
        let (mut a, mut b) = (vec![[0.0_f32; 2]; samples], vec![[0.0_f32; 2]; samples]);
        from_wire(&split, &wire, 48, &mut [&mut a, &mut b], 0, samples);

        let mut whole = [vec![0; samples * 8], vec![0; samples * 8]];
        let [first, second] = &mut whole;
        split
            .convert(&[&wire], &mut [first, second])
            .expect("buffers match");
        let bytes = |channel: &[[f32; 2]]| -> Vec<u8> {
            channel
                .iter()
                .flatten()
                .flat_map(|value| value.to_le_bytes())
                .collect()
        };
        assert_eq!([bytes(&a), bytes(&b)], whole, "chunked as one call");

        let merge = Converter::new(SampleFormat::Cf32, SampleFormat::Ci12, 2, 1).expect("ported");
        let mut back = vec![0; samples * 6];
        to_wire(&merge, &[&a, &b], &mut back);
        let mut once = vec![0; samples * 6];
        merge
            .convert(&[&whole[0], &whole[1]], &mut [&mut once])
            .expect("buffers match");
        assert_eq!(back, once, "chunked as one call");
    }
}
