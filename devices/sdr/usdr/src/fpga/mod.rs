//! The uSDR FPGA gateware's host-visible registers.

pub(crate) mod dc_estimator;
mod diag;
mod fir_tables;
pub(crate) mod flash;
mod gpio;
mod phy;
mod stream;
mod tx_stream;

pub(crate) use diag::{read_atcrbs, read_tx_dma_status, write_atcrbs};
pub(crate) use gpio::{Gpi, Gpo, Hwid, IdentityRead};
pub use phy::IqImbalance;
pub(crate) use phy::{Decimation, Nco, Phy, StreamPhyWrites};
pub(crate) use stream::BurstPlan;
pub(crate) use tx_stream::{
    MAX_TX_TIMESTAMP, TX_SAMPLE_BYTES, TxCoreStatus, check_tx_gateware, extend_drop_count, push_tx,
    read_tx_status, tx_block_bytes,
};
