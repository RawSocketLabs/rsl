//! Lime Microsystems LMS7002M two-channel RF transceiver: its register values and the
//! chip-level register sequences, shared by the drivers whose boards carry one (libusdr's
//! LMS7 family, XSDR first; its `LimeSDR` later).
//!
//! Sources: libusdr's register map (`hw/lms7002m/lms7002m.yaml`, generated into
//! `gen/def_lms7002m.h`), which names every field and option but describes none, and its
//! chip driver (`hw/lms7002m/lms7002m.c`), whose operations the [`Lms7002m`] state and the
//! [`Create`] sequence port. No datasheet is vendored, so field names keep the mnemonics
//! where the source gives nothing more, and nothing here states reset values.
//!
//! - Addresses: [`Reg`]; value types implement this crate's 16-bit [`Register`] trait.
//! - SPI: one 32-bit word per access, bit 31 = write, bits 30:16 the address, 15:0 the data.
//! - Banked registers: the chip keeps per-channel copies of some registers;
//!   [`LmlControl`]'s `MAC` selects the copies a write reaches.
//! - Sequences: [`Create`] (`lms7002m_create`) and [`Batch`]es of writes that the
//!   [`Lms7002m`] operations build (one `lms7002m_spi_post` each), all on `rsl-lowlevel`'s
//!   `Sequence` seam. Where a board wires the chip and which LDOs it enables stay with the
//!   board's driver ([`ResetPlan`]).
//!
//! `no_std`. The sequences allocate nothing (fixed-size state, held by review); `bnb`, which
//! the value types use, requires `alloc`.
#![no_std]
#![forbid(unsafe_code)]

mod batch;
mod chip;
mod create;
mod reg;
mod register;
mod regs;
mod spi;

pub use batch::{BATCH_CAPACITY, Batch};
pub use chip::{Lms7002m, NoChannel, RxMode, Tsp, TxMode, TxPath};
pub use create::{Create, ResetPlan};
pub use reg::Reg;
pub use register::Register;
pub use regs::*;

/// The chip's name in errors.
pub const NAME: &str = "LMS7002M";
