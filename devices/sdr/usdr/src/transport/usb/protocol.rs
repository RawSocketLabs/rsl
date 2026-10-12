//! Shared USB wire fields; the portable implementation is also compiled by the C oracle.

pub(super) use crate::portable::usb::protocol::I2cCache;
#[cfg(test)]
pub(super) use crate::portable::usb::protocol::Notification;
