//! The READY/ACTIVE state machine's registers: [`RequestedState`] commands a state,
//! [`CurrentState`] reports it.

use bnb::BitEnum;

use super::reg::Reg;
use crate::chips::register::Register;

/// The operating state the device is commanded into (write-only). `USYS_CTRL`, 0x06.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
pub(super) enum RequestedState {
    /// Hold the outputs so the configuration can be changed.
    Ready = 0x01,

    /// Run with the current configuration.
    Active = 0x02,

    /// Any other value.
    #[catch_all]
    Other(u8),
}
impl Register for RequestedState {
    type Map = Reg;
    const ADDR: Reg = Reg::RequestedState;
}

/// The state the device is in (read-only). `USYS_STAT`, 0x07.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
pub(super) enum CurrentState {
    /// Outputs held; configuration can be changed.
    Ready = 0x01,

    /// Running.
    Active = 0x02,

    /// No input clock detected, so it cannot become active.
    NoInputClock = 0x89,

    /// Any other value, including while a transition is in progress.
    #[catch_all]
    Other(u8),
}
impl Register for CurrentState {
    type Map = Reg;
    const ADDR: Reg = Reg::CurrentState;
}
