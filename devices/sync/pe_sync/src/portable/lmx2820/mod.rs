//! TI LMX2820 wideband synthesizer: libusdr's `hw/lmx2820/lmx2820.c` as `PE_SYNC` uses it,
//! for its two synthesizers.
//!
//! A PLL from a reference (optional doubler, pre-divider, multiplier, post-divider) into a
//! 5.65–11.3 GHz VCO with a fractional feedback divider; two outputs each take the VCO, its
//! doubler or a channel divider. Registers are 16 bits behind a 7-bit address
//! ([`ti_spi`](super::ti_spi)).
//!
//! - [`reg`]: addresses and value types;
//! - [`plan`]: the solver (`lmx2820_solver`) and the register list a tune writes;
//! - [`sequence`]: `lmx2820_create`, `lmx2820_tune`, `lmx2820_instant_calibration_init` and
//!   `lmx2820_read_status` on the `Sequence` seam;
//! - [`controls`]: `lmx2820_tune_instcal` and `lmx2820_sync`.
//!
//! `PE_SYNC` tunes with SYSREF off and without instant calibration, and calls neither
//! control; SYSREF, instant calibration and the controls are ported for parity, not
//! reachable from `Device`.
//!
//! Only `PE_SYNC` uses the chip in libusdr, so it lives in this driver until a second one
//! needs it (`devices/AGENTS.md`); it knows no board.

#[allow(
    dead_code,
    reason = "ported for op-for-op parity; no libusdr PE_SYNC path calls it, and exposing it on `Device` awaits the owner"
)]
pub(crate) mod controls;
pub(crate) mod plan;
pub(crate) mod reg;
pub(crate) mod sequence;
