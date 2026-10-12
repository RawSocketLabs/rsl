//! Platform-independent production algorithms, shared unchanged with the `no_std` fixture.

pub(crate) mod address;
pub(crate) mod clock;
pub(crate) mod error;
pub(crate) mod temperature;
#[cfg(feature = "usb")]
pub(crate) mod usb;
