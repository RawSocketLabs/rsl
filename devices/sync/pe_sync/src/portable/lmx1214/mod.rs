//! TI LMX1214 LO buffer and divider: libusdr's `hw/lmx1214/lmx1214.c` as `PE_SYNC` uses it,
//! for its LO distribution.
//!
//! The input, buffered or divided by 2 to 8, on four outputs, plus an auxiliary output
//! through a pre-divider and a divider. Registers are 16 bits behind a 7-bit address
//! ([`ti_spi`](super::ti_spi)).
//!
//! - [`reg`]: addresses and value types;
//! - [`plan`]: the solver (`lmx1214_solver`) and its register list;
//! - [`sequence`]: `lmx1214_create`, the solver's bus part and `lmx1214_get_temperature`;
//! - [`windowing`]: the SYNC windowing (`lmx1214_sysref_windowing_*`), which `PE_SYNC` does
//!   not call (ported for parity, not reachable from `Device`).
//!
//! Only `PE_SYNC` uses the chip in libusdr, so it lives in this driver until a second one
//! needs it (`devices/AGENTS.md`); it knows no board.

pub(crate) mod plan;
pub(crate) mod reg;
pub(crate) mod sequence;
#[allow(
    dead_code,
    reason = "ported for op-for-op parity; no libusdr PE_SYNC path calls it, and exposing it on `Device` awaits the owner"
)]
pub(crate) mod windowing;
