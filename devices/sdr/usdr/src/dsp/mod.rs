//! Host sample-format conversion and channel (de)interleaving, the scalar port of libusdr's
//! `xdsp/conv.c` ([`Converter`], [`SampleFormat`], [`ConversionError`]): the shared
//! `sdr-formats`'s (`../formats`, 2026-10-21, for the XSDR's and `LimeSDR`'s streams),
//! re-exported unchanged; its crate docs hold the selection table and the arithmetic.
//!
//! Spectrum and demodulation kernels, from libusdr's other `xdsp` files, are in
//! [`window`], [`fftad`], [`rtsa`] and [`fm`]. libusdr's integer FFT (`xdsp/intfft.c`) is
//! compiled out (`#if 0`) in the reference, so there is no FFT to port. The [`filter`],
//! [`trig`], [`nco`] and [`sincos`] submodules port the filter and generator kernels;
//! `filter` allocates and `sincos` needs `std`.

pub mod fftad;
pub mod filter;
pub mod fm;
mod log2;
pub mod nco;
pub mod rtsa;
pub mod sincos;
pub mod trig;
pub mod window;

pub use sdr_formats::{ConversionError, Converter, SampleFormat};
