//! The FPGA stream engine libusdr's uSDR (`m2_lm6_1`) and LMS7 family (`m2_lm7_1`)
//! gateware share, at the same addresses: the RX DMA and front end, the sync unit and the
//! TX cores (`ipblks/streams/`), as portable register sequences ([`EngineSequence`]), how
//! an RX stream's packets split into bursts ([`BurstPlan`]), and the original TX core's
//! packet announcement and status words (`tx`). Beside them, the FIR microcode the DSP
//! chains feeding those cores load ([`fir_tables`], `ipblks/fgearbox.c`), shared by the
//! uSDR's and the `LSDr`'s rate changes.
//!
//! **Unstable; unpublished.** What differs between the boards stays in their drivers:
//! which logical channel rides which lanes, the RF side of a stream's create and destroy,
//! host formats, the transports' buffers and how each board loads a FIR table.
#![no_std]
#![forbid(unsafe_code)]

mod burst;
mod engine;
pub mod fir_tables;
mod tx;

pub use burst::{BurstPlan, RxLanes, RxSampleFormat};
pub use engine::{EngineSequence, RX_DMA, RxFormat, StartPart, SyncTrigger, TxLanes};
pub use tx::{
    MAX_TX_PACKET_BYTES, MAX_TX_TIMESTAMP, REG_TX_PUSH, REG_TX_STATUS, TxCoreStatus,
    extend_drop_count, tx_block_bytes, tx_gateware_supported, tx_push_words,
};
