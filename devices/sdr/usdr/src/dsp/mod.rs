//! Host sample-format conversion and channel (de)interleaving: the scalar port of libusdr's
//! `xdsp/conv.c` and its generic kernels.
//!
//! [`Converter::new`] selects what libusdr's `get_transform_fn` selects:
//!
//! | From | To | Inputs → outputs |
//! |------|----|------------------|
//! | any format | the same format | 1 → 1 (copy) |
//! | `I16`/`Ci16` | `F32`/`Cf32` (real to real, complex to complex), and back | 1 → 1 |
//! | `I12`/`Ci12` | `F32`/`Cf32` or `I16`/`Ci16`, and back | 1 → 1 |
//! | `I16` | `F32` | 1 → 4 |
//! | `Ci16` | `Cf32` or `Ci16` | 1 → 2, 3, 4, 6, 8 |
//! | `Ci12` | `Cf32` or `Ci16` | 1 → 2, 4 |
//! | `Cf32` or `Ci16` | `Ci16` | 2, 3, 4 → 1 |
//! | `Cf32` or `Ci16` | `Ci12` | 2, 4 → 1 |
//!
//! The arithmetic is that of libusdr's generic (scalar) kernels, bit for bit (checked
//! against the compiled C by the oracle's `conv_parity` test). libusdr's SIMD kernels, which
//! a normal x86-64 build selects, round floats to nearest and saturate instead, so they can
//! differ by one LSB. In the generic kernels, integers become floats as `value * (1.0f / 32767)` and floats
//! become integers as `(int16_t)(x / (1.0f / 32767))`, truncating; 12-bit packing keeps the
//! top 12 bits of a 16-bit value. Kernels work in whole groups (one sample per channel), and
//! what they do with a partial group is libusdr's too: most ignore it, the one-to-one 12-bit
//! kernels convert a lone last value. Where libusdr is undefined or unbounded, this module
//! is defined instead:
//!
//! - Floats that scale outside (-32769, 32768), just beyond ±1.0, and NaN are undefined
//!   behaviour in the C; here they saturate to `i16::MIN`/`i16::MAX`, and NaN becomes 0.
//! - libusdr reads and writes host-endian memory; buffers here are always little-endian, the
//!   same bytes on every little-endian host.
//! - libusdr's byte counts are 32-bit and wrap; these are `usize` and exact.
//! - libusdr can write past a short channel buffer (`Ci12` → two `Cf32`); this writes only
//!   what fits.
//! - libusdr returns a byte copy for one-to-one pairs it does not know; [`Converter::new`]
//!   rejects them.
//!
//! One libusdr quirk is kept: an odd-length one-to-one conversion to 12 bits packs its lone
//! last value as the low byte, then the high byte shifted down four bits, which libusdr's
//! own unpacking of a lone value does not invert.
//!
//! Conversion uses only `core`, so it can join the portable tree unchanged.
//!
//! Spectrum and demodulation kernels, from libusdr's other `xdsp` files, are in
//! [`window`], [`fftad`], [`rtsa`] and [`fm`]. libusdr's integer FFT (`xdsp/intfft.c`) is
//! compiled out (`#if 0`) in the reference, so there is no FFT to port. The [`filter`],
//! [`trig`], [`nco`] and [`sincos`] submodules port the filter and generator kernels;
//! `filter` allocates and `sincos` needs `std`.

mod conv;
pub mod fftad;
pub mod filter;
pub mod fm;
mod format;
mod log2;
pub mod nco;
pub mod rtsa;
pub mod sincos;
#[cfg(test)]
mod tests;
pub mod trig;
pub mod window;

pub use conv::{ConversionError, Converter};
pub use format::SampleFormat;
