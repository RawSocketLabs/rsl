//! Platform-independent board sequences: the gateware registers, the `LSDr`'s chips' register
//! values, and the hand-written sans-I/O sequences the blocking [`Device`](crate::Device)
//! runs with `hal`'s executor. Core only: no std, no allocation; keep imports within
//! this tree and the shared `no_std` crates (`nostd-check` compiles it unchanged).

pub(crate) mod adc_power;
pub(crate) mod ads42lbx9;
pub(crate) mod close;
pub(crate) mod fpga;
pub(crate) mod lmk04832;
pub(crate) mod outcome;
pub(crate) mod pga;
pub(crate) mod probes;
pub(crate) mod si549;
pub(crate) mod single;
pub(crate) mod supplies;
pub(crate) mod tps6594;
pub(crate) mod wiring;
