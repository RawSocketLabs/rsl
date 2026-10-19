//! The FPGA stream engine libusdr's uSDR (`m2_lm6_1`) and LMS7 family (`m2_lm7_1`)
//! gateware share, at the same addresses: the RX DMA and front end, the sync unit and the
//! TX cores (`ipblks/streams/`), as portable register sequences ([`EngineSequence`]), and
//! how an RX stream's packets split into bursts ([`BurstPlan`]).
//!
//! **Unstable; unpublished.** What differs between the boards stays in their drivers:
//! which logical channel rides which lanes, the RF side of a stream's create and destroy,
//! host formats and the transports' buffers.
#![no_std]
#![forbid(unsafe_code)]

mod burst;
mod engine;

pub use burst::{BurstPlan, RxLanes, RxSampleFormat};
pub use engine::{EngineSequence, RX_DMA, RxFormat, StartPart, SyncTrigger, TxLanes};
