//! The `m2_lm6_1` board's RF sequences without I/O: power-up and power-down, the sample
//! rate, tuning, gains, paths and the RF chains' part of a stream's create and destroy
//! (`device/m2_lm6_1/usdr_ctrl.c`). Each is a composite machine ([`run`]) that owns a copy
//! of the board's [`Radio`] state while it runs; the blocking board and the native USB
//! handle run the same machines and take the state back.

pub(crate) mod gain;
pub(crate) mod path;
pub(crate) mod power;
pub(crate) mod rate;
pub(crate) mod run;
pub(crate) mod state;
pub(crate) mod stream;
pub(crate) mod tune;
pub(crate) mod wiring;

pub(crate) use gain::{RxGain, TxGain};
pub(crate) use path::{RxPath, SetRxPath, TxPath};
pub(crate) use power::{ChainPower, PowerUp, PowerUpPlan, power_down};
pub(crate) use rate::{RateChains, SampleRate};
pub(crate) use run::{OwnsRadio, Run, Steps};
pub(crate) use state::{Chain, Chains, NcoChannels, Radio};
pub(crate) use stream::{StreamUp, stream_down};
pub(crate) use tune::Tune;
