//! The uSDR FPGA gateware's host-visible registers.

mod diag;
mod fir_tables;
pub(crate) mod flash;
mod gpio;
mod phy;
mod stream;

pub(crate) use diag::{read_atcrbs, read_tx_dma_status, write_atcrbs};
pub(crate) use gpio::{Gpi, Gpo, Hwid};
pub(crate) use phy::{Decimation, Nco, Phy};
pub(crate) use stream::{
    BurstPlan, SyncMode, configure_rx, reset_rx_dma, run_rx, signal_rx_ready, sync, throttle_rx,
};
