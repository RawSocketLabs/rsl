//! The uSDR FPGA gateware's host-visible registers.

mod fir_tables;
mod gpio;
mod phy;
mod stream;

pub(crate) use gpio::{Gpi, Gpo, Hwid};
pub(crate) use phy::{Decimation, Nco, Phy};
pub(crate) use stream::{
    BurstPlan, SyncMode, configure_rx, reset_rx_dma, run_rx, signal_rx_ready, sync,
};
