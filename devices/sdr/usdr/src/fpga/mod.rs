//! The uSDR FPGA gateware's host-visible registers.

mod diag;
mod fe_gpio;
mod fir_tables;
pub(crate) mod flash;
mod gpio;
mod phy;
mod stream;

pub(crate) use diag::{read_atcrbs, read_tx_dma_status, write_atcrbs};
pub(crate) use fe_gpio::{FrontEndGpio, PinMode};
pub(crate) use gpio::{Gpi, Gpo, IdentityRead};
pub(crate) use phy::{Decimation, IqImbalance, Nco, Phy};
pub(crate) use stream::{
    BurstPlan, SyncTrigger, configure_rx, reset_rx_dma, run_rx, signal_rx_ready, sync, throttle_rx,
};
