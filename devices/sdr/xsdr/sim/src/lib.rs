//! A simulated LMS7 family board (libusdr's `m2_lm7_1`: XSDR, XTRX, sSDR, sSDR Pro).
//!
//! The board is modelled at libusdr's low-level operation seam (`ls_op`): FPGA register
//! reads and writes (GPO/GPI), 32-bit SPI transactions to the LMS7002M, and I2C transfers
//! to the board's support chips (LP8758 PMICs, the XTRX DAC80501). libusdr (through
//! `xsdr-oracle`) and, later, `xsdr` drive the same model, so their recorded [`Op`]
//! traces can be compared without hardware.
//!
//! The model covers identification, `xsdr_init` and RFIC power-up/close for XSDR and XTRX,
//! and the front-end boards libusdr probes for on the XSDR's M.2 connector
//! ([`SimBoard::with_front_end`], `sdr-frontend-sim`'s models, shared with `usdr-sim`),
//! the RX and TX stream engines and the sync unit ([`SimBoard::rx_next_block`],
//! [`SimBoard::tx_grant`], `sdr-stream-sim`'s models, likewise shared), and the TX PHY port
//! with its MMCM behind the DRP (`tx_phy.rs`).
//! sSDR variants are identified but carry no LMS8001, so libusdr refuses to open them.
//! Each modelling assumption is stated, with its evidence, where it is encoded; see the
//! family's `AGENTS.md` for the full list.
#![forbid(unsafe_code)]

mod board;
mod chips;
mod trace;
mod tx_phy;

pub use board::{I2cAddress, SimBoard, SimError, Variant};
pub use sdr_frontend_sim::FrontEnd;
pub use sdr_stream_sim::{TxGrant, TxPacket};
pub use trace::Op;
pub use tx_phy::MmcmFault;
