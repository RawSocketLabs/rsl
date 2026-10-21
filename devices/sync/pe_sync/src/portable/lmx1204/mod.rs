//! TI LMX1204 clock buffer, divider and multiplier with SYSREF: libusdr's
//! `hw/lmx1204/lmx1204.c` as `PE_SYNC` uses it, for its clock distribution.
//!
//! CLKIN buffered, divided by 2 to 8 or multiplied by up to 4 (through a calibrated PLL) on
//! four channels, each a clock and a SYSREF output, plus a logic channel with its own
//! dividers; SYSREF is generated, pulsed or repeated, with per-output delays. Registers are
//! 16 bits behind a 7-bit address ([`ti_spi`](super::ti_spi)).
//!
//! - [`reg`]: addresses and value types;
//! - [`plan`]: the solver (`lmx1204_solver`) and its register list;
//! - [`sequence`]: `lmx1204_create`, the solver's bus part, `lmx1204_read_status` and
//!   `lmx1204_wait_pll_lock`;
//! - [`windowing`]: `lmx1204_reload_sysrefout_ch_delay` and the SYSREF windowing, which
//!   `PE_SYNC` does not call (ported for parity, not reachable from `Device`).
//!
//! Not ported: `lmx1204_calibrate` and `lmx1204_reset_main_divider`, which nothing in
//! libusdr calls.
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
