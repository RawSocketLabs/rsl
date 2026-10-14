//! A simulated LimeSDR-mini (libusdr's `u3_limesdr` family).
//!
//! The board is modelled at its control seam: 64-byte LMS64C command frames
//! ([`SimBoard::lms64c`]), the protocol the board firmware speaks over the FT601 USB bridge.
//! Behind it sit the gateware's 16-bit register file (with the reference-clock frequency
//! detector libusdr's `limesdr_init` uses) and the LMS7002M (`lms7002m-sim`). libusdr
//! (through `limesdr-oracle`) and, later, `limesdr` drive the same model, so their
//! recorded [`Op`] traces, frames and decoded register operations alike, can be compared
//! without hardware.
//!
//! The model covers identification, `limesdr_init` and close. Each modelling assumption is
//! stated, with its evidence, where it is encoded; see the family's `AGENTS.md` for the
//! full list.
#![forbid(unsafe_code)]

mod board;
mod lms64c;
mod trace;

pub use board::{DETECTOR_GATE_US, SimBoard};
pub use lms64c::{DATA_LEN, FRAME_LEN, Frame};
pub use trace::Op;
