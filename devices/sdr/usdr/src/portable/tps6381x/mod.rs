//! TPS63810/11 buck-boost converter registers (TI SLVSEK4C §8.6): the address map
//! ([`reg`]), the [`control`] register, the output [`voltage`] registers and the device
//! [`id`].

mod control;
mod id;
mod reg;
mod voltage;

pub(crate) use control::Control;
pub(crate) use id::{DEVICE_ID, DeviceId};
pub(crate) use reg::Reg;
pub(crate) use voltage::{OutputVoltage, Vsel};

/// The chip's name in errors.
pub(crate) const NAME: &str = "TPS6381x";
