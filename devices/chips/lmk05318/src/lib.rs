//! TI LMK05318 network synchronizer clock: its register values and frequency plans, shared
//! by the drivers whose boards carry one (the uSDR's `SimpleSync` and XMASS front ends,
//! `PE_SYNC`).
//!
//! # What the chip does
//!
//! A DPLL plus two APLLs that discipline a reference and fan it out on eight outputs.
//! APLL1 runs a 2.5 GHz BAW VCO from the XO; APLL2 (5.5–6.25 GHz) is cascaded from it, with
//! two post-dividers. Each output's mux picks APLL1 or one of APLL2's post-dividers, then
//! an output divider (output 7's has a second stage). Registers are bytes behind a 16-bit
//! address, sent most significant byte first.
//!
//! # What this crate holds
//!
//! libusdr's `hw/lmk05318/lmk05318.c`: the frequency plan ([`plan`]: XO path, APLL1, the
//! output solver with APLL2's state, output 7's stages), the registers a plan and the
//! common settings stage ([`staging`]) into libusdr's staging map ([`map`]), the address
//! map ([`reg`]) and the value types by block ([`apll`], [`control`], [`output`],
//! [`xo`]); and, behind feature `std`, the bus traffic every board runs (`driver`:
//! identity check, writing the map, soft reset, lock waits, output synchronization) as
//! blocking code over `hal`'s `Bus`, since the 16-bit addresses and four-byte identity
//! read do not fit its portable `Sequence` seam. Without the DPLL, which no board here
//! enables (not ported), APLL1 runs free.
//!
//! # Sources
//!
//! libusdr's `hw/lmk05318/lmk05318.c` and register YAML `lmk05318.yaml` (vendored beside
//! it) for fields and meanings. The YAML does not document addresses 0 to 3, so the
//! identity stays four raw bytes.
//!
//! `no_std` with `alloc`: plans are vectors; `bnb`'s bitfields need `alloc` too. Feature
//! `std` adds the blocking driver.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod apll;
pub mod control;
#[cfg(feature = "std")]
pub mod driver;
pub mod map;
pub mod output;
pub mod plan;
pub mod reg;
pub mod staging;
pub mod xo;

/// The chip's name in errors.
pub const NAME: &str = "LMK05318";

/// The identification bytes from address 0, in wire order; libusdr's `lmk05318_create`
/// refuses any other chip (`-ENODEV`).
pub const ID: [u8; 4] = [0x10, 0x0b, 0x35, 0x42];
