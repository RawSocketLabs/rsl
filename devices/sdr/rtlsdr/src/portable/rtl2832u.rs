//! The RTL2832U USB demodulator side of the dongle.
//!
//! No admissible source for any RTL2832U fact is held yet: the identifiers, request codes,
//! register map, how the host reaches the tuner and the init order will come from
//! descriptor dumps and usbmon captures of the owner's RTL-SDR Blog V4 and V5 dongles
//! (`CAPTURES.md`). Even the architecture the seam and simulator assume (vendor control
//! transfers plus one bulk IN stream, `RTL-GAP-12`; a demodulator-controlled gate in front
//! of the tuner's I2C port, `RTL-GAP-13`) is a GAP row until a capture shows it. Each value
//! below is a [`Pending`] naming its row in `PROVENANCE.md`; rows without a typed value
//! here have no code until a capture shows their shape, except the open (`RTL-GAP-07`),
//! which `Device::open` refuses with directly.

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
