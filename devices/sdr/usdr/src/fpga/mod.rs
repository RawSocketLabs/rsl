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
pub(crate) use phy::{Decimation, Nco, Phy};
pub(crate) use stream::BurstPlan;
pub(crate) use tx_stream::{
    ExtendedTxPush, MAX_EXTENDED_TX_TIMESTAMP, MAX_TX_TIMESTAMP, TxCoreStatus, check_tx_gateware,
    configure_extended_tx, extend_drop_count, map_extended_tx, mute_extended_tx, push_tx,
    read_tx_status, stream_block_bytes,
};
#[cfg(all(feature = "mio", target_os = "linux"))]
pub(crate) use tx_stream::{TX_SAMPLE_BYTES, tx_block_bytes};
