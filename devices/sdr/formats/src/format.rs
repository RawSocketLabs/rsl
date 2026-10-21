//! Sample formats and the scalar value codecs every converter is built from.

/// libusdr's float scale, `CONV_SCALE` (`1.0f/32767`): full scale ±1.0 is ±32767.
const SCALE: f32 = 1.0 / 32767.0;

/// A host or wire sample format libusdr converts between (`xdsp/conv.c`'s format names).
///
/// Buffers are little-endian bytes. Complex formats interleave I then Q. Packed 12-bit
/// formats hold two values in three bytes, the first value in byte 0 and the low nibble of
/// byte 1, the second in the high nibble of byte 1 and byte 2. A 12-bit value is the top 12
/// bits of a 16-bit one: unpacking shifts it up four bits, packing drops the low four bits
/// (except a lone last value; see [the crate docs](crate)). Floats map ±32767 to ±1.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SampleFormat {
    /// Real signed 16-bit values (`i16`).
    I16,
    /// Real signed 12-bit values, packed (`i12`).
    I12,
    /// Real 32-bit floats (`f32`).
    F32,
    /// Complex signed 16-bit I/Q pairs (`ci16`).
    Ci16,
    /// Complex signed 12-bit I/Q pairs, packed (`ci12`).
    Ci12,
    /// Complex 32-bit float I/Q pairs (`cf32`).
    Cf32,
}

impl SampleFormat {
    /// The encoding of one value, real or one half of a complex pair.
    pub(super) fn element(self) -> Element {
        match self {
            Self::I16 | Self::Ci16 => Element::I16,
            Self::I12 | Self::Ci12 => Element::I12,
            Self::F32 | Self::Cf32 => Element::F32,
        }
    }
}

/// How one value is stored. Every conversion decodes values to `i16` and encodes them again,
/// as libusdr's generic kernels do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Element {
    /// Packed 12-bit, two values per three bytes.
    I12,
    /// Little-endian `i16`.
    I16,
    /// Little-endian `f32`.
    F32,
}

impl Element {
    /// Stored size of one value in bits.
    pub(super) fn bits(self) -> usize {
        match self {
            Self::I12 => 12,
            Self::I16 => 16,
            Self::F32 => 32,
        }
    }

    /// Decodes whole values from `bytes` into `values`, stopping at whichever runs out
    /// first. 12-bit values decode in pairs.
    pub(super) fn decode(self, bytes: &[u8], values: &mut [i16]) {
        match self {
            Self::I16 => {
                for (value, raw) in values.iter_mut().zip(bytes.chunks_exact(2)) {
                    *value = i16::from_le_bytes([raw[0], raw[1]]);
                }
            }
            Self::F32 => {
                for (value, raw) in values.iter_mut().zip(bytes.chunks_exact(4)) {
                    *value = from_float(f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]));
                }
            }
            Self::I12 => {
                for (pair, raw) in values.chunks_exact_mut(2).zip(bytes.chunks_exact(3)) {
                    let [first, second] = unpack_pair([raw[0], raw[1], raw[2]]);
                    pair[0] = first;
                    pair[1] = second;
                }
            }
        }
    }

    /// Encodes `values` into `bytes`, keeping only the bytes that fit (a value that would
    /// cross the end is cut there, as libusdr's overrunning store leaves the in-bounds part).
    /// 12-bit values encode in pairs.
    pub(super) fn encode(self, values: &[i16], bytes: &mut [u8]) {
        match self {
            Self::I16 => store(bytes, values.iter().map(|value| value.to_le_bytes())),
            Self::F32 => store(
                bytes,
                values.iter().map(|&value| to_float(value).to_le_bytes()),
            ),
            Self::I12 => store(
                bytes,
                values
                    .chunks_exact(2)
                    .map(|pair| pack_pair(pair[0], pair[1])),
            ),
        }
    }
}

/// Writes `encoded` back to back into `bytes`, cutting the first one that crosses its end.
fn store<const N: usize>(bytes: &mut [u8], mut encoded: impl Iterator<Item = [u8; N]>) {
    let mut chunks = bytes.chunks_exact_mut(N);
    for (raw, value) in (&mut chunks).zip(&mut encoded) {
        raw.copy_from_slice(&value);
    }
    let rest = chunks.into_remainder();
    if let Some(value) = encoded.next() {
        rest.copy_from_slice(&value[..rest.len()]);
    }
}

/// A value as a float: `value * CONV_SCALE`, in single precision as the C computes it.
fn to_float(value: i16) -> f32 {
    f32::from(value) * SCALE
}

/// A float as a value: `(int16_t)(x / CONV_SCALE)`, truncating toward zero.
///
/// libusdr's conversion is undefined behaviour in C when the quotient is outside
/// `(-32769, 32768)` or not a number. Here it saturates instead, and NaN becomes 0: the
/// defined Rust conversion, chosen over the wrap x86 scalar code happens to produce.
#[expect(
    clippy::cast_possible_truncation,
    reason = "truncation toward zero is libusdr's conversion; `as` saturates outside it"
)]
pub(super) fn from_float(x: f32) -> i16 {
    (x / SCALE) as i16
}

/// The two's-complement bits of `value`.
fn bits_of(value: i16) -> u16 {
    u16::from_le_bytes(value.to_le_bytes())
}

/// `value`'s bits read as two's complement.
fn signed(value: u16) -> i16 {
    i16::from_le_bytes(value.to_le_bytes())
}

/// Packs two values' top 12 bits into three bytes (`wu_u32b_t` bytes 1 to 3 in the C).
fn pack_pair(first: i16, second: i16) -> [u8; 3] {
    let packed = u32::from(bits_of(first) >> 4) | (u32::from(bits_of(second) >> 4) << 12);
    let [low, middle, high, _] = packed.to_le_bytes();
    [low, middle, high]
}

/// Unpacks three bytes into two values, each shifted to the top of 16 bits.
fn unpack_pair([b0, b1, b2]: [u8; 3]) -> [i16; 2] {
    let first = (u16::from(b0) << 4) | (u16::from(b1) << 12);
    let second = (u16::from(b2) << 8) | u16::from(b1 & 0xf0);
    [signed(first), signed(second)]
}

/// The lone value ending an odd-length 12-bit stream, packed as libusdr's tail does: the
/// value's low byte, then its high byte shifted down four bits. This is not the pair
/// layout; libusdr's own unpacking of a lone value does not invert it.
pub(super) fn pack_lone(value: i16) -> [u8; 2] {
    let [low, high] = value.to_le_bytes();
    [low, high >> 4]
}

/// The lone value ending an odd-length 12-bit stream, unpacked as libusdr's tail does: the
/// low 12 bits of a little-endian 16-bit word, shifted to the top.
pub(super) fn unpack_lone(raw: [u8; 2]) -> i16 {
    signed(u16::from_le_bytes(raw) << 4)
}
