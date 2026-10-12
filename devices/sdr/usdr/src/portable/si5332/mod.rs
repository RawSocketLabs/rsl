//! Si5332 clock-generator registers and sequences (register meanings from libusdr's
//! `hw/si5332/si5332.yaml`): the address map ([`reg`]), the value types by stage
//! ([`input`], [`divider`], [`output`], [`state`]) and the [`sequence`] machines.

mod divider;
mod input;
pub(crate) mod output;
mod reg;
pub(crate) mod sequence;
mod state;

pub(crate) use sequence::{ClockSequence, LvpeclOutput, Reference};

/// The chip's name in errors.
pub(crate) const NAME: &str = "Si5332";
