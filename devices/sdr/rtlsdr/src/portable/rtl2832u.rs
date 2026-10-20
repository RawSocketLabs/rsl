//! The RTL2832U USB demodulator side of the dongle.
//!
//! No admissible source for any RTL2832U fact is held yet: the identifiers, vendor request
//! codes, register map, I2C repeater and init order will come from descriptor dumps and
//! usbmon captures of the owner's RTL-SDR Blog V4 and V5 dongles (`CAPTURES.md`). Each is
//! a [`Pending`] naming its GAP row in `PROVENANCE.md`; the rows without a typed value
//! here (request codes, register addressing, the tuner's I2C route, the sample format)
//! have no code until a capture shows their shape.

use super::pending::Pending;

/// A USB device identity, as a descriptor dump shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UsbId {
    /// `idVendor`.
    pub vendor: u16,

    /// `idProduct`.
    pub product: u16,
}

/// Where the sample stream arrives: the interface to claim and its bulk IN endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UsbInterface {
    /// `bInterfaceNumber`.
    pub number: u8,

    /// The bulk IN `bEndpointAddress` (direction bit set).
    pub rx_endpoint: u8,
}

/// The RTL-SDR Blog V4's identity. prov: RTL-GAP-01
pub const BLOG_V4_ID: Pending<UsbId> = Pending::gap("RTL-GAP-01");

/// The "V5" dongle's identity. prov: RTL-GAP-02
pub const V5_ID: Pending<UsbId> = Pending::gap("RTL-GAP-02");

/// The interface and bulk IN endpoint the sample stream uses. prov: RTL-GAP-03
pub const STREAM_INTERFACE: Pending<UsbInterface> = Pending::gap("RTL-GAP-03");

/// The open: the control transfers that bring the demodulator and tuner up. Its shape (a
/// fixed list, or reads whose answers steer it) is part of the gap. prov: RTL-GAP-07
pub const OPEN: Pending<()> = Pending::gap("RTL-GAP-07");
