//! Platform-independent production algorithms, shared unchanged with the `no_std` fixture.

pub(crate) mod address;
pub(crate) mod clock;
pub(crate) mod error;
pub(crate) mod lp8758;
pub(crate) mod power;
pub(crate) mod register;
pub(crate) mod sequence;
pub(crate) mod si5332;
pub(crate) mod stream;
pub(crate) mod temperature;
pub(crate) mod tps6381x;
#[cfg(feature = "usb")]
pub(crate) mod usb;
