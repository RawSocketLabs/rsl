//! The uSDR FPGA gateware's host-visible registers.

mod diag;
mod fe_gpio;
mod fir_tables;
pub(crate) mod flash;
mod gpio;
mod phy;
mod stream;
mod tx_stream;

pub(crate) use diag::{read_atcrbs, read_tx_dma_status, write_atcrbs};
pub(crate) use fe_gpio::{FrontEndGpio, PinMode};
pub(crate) use gpio::{Gpi, Gpo, Hwid, IdentityRead};
pub(crate) use phy::{Decimation, IqImbalance, Nco, Phy};
pub(crate) use stream::{
    BurstPlan, SyncTrigger, configure_rx, reset_rx_dma, run_rx, signal_rx_ready, sync, throttle_rx,
};
pub(crate) use tx_stream::{
    MAX_TX_PACKET_BYTES, MAX_TX_TIMESTAMP, TX_SAMPLE_BYTES, TxCoreStatus, extend_drop_count,
    push_tx, read_tx_status, start_tx, stop_tx, update_tx_flags,
};
