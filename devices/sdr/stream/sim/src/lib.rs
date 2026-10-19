//! Models of the FPGA stream engine the simulated uSDR and LMS7 family boards share, at
//! the register seam libusdr and the drivers both drive: the RX DMA and front end with the
//! blocks they deliver ([`RxEngine`]) and the sync unit ([`SyncUnit`]). A board feeds them
//! every register write and asks them for blocks below the seam.
#![forbid(unsafe_code)]

mod rx;
mod sync;

pub use rx::RxEngine;
pub use sync::{REG_SYNC, SYNC_SELECT, SyncUnit};
