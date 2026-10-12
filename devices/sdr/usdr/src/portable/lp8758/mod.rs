//! LP8758-E0 PMIC registers (TI SNVSAC6B §7.6): the address map ([`reg`]), the per-channel
//! registers ([`buck`]), the chip-wide [`config`] and the revision ([`id`]).

mod buck;
mod config;
mod id;
mod reg;

pub(crate) use buck::{Buck, BuckControl, BuckVoltage};
pub(crate) use config::Config;
pub(crate) use id::{REVISION, Revision};
pub(crate) use reg::Reg;

/// The chip's name in errors.
pub(crate) const NAME: &str = "LP8758";
