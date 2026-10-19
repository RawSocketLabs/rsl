//! The stream engine's register sequences, shared with the LMS7 family's driver
//! (`sdr-stream`), re-exported where the board and `NativeUsb` have always found them.

pub(crate) use sdr_stream::{EngineSequence, SyncTrigger, TxLanes};
