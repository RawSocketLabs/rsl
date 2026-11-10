//! The RTL2832U USB demodulator side of the dongle.
//!
//! The identities and stream interfaces come from the dongles' descriptor dumps
//! (`DESC-V4-0001`, `DESC-V5-0001`). Both share one `idVendor`:`idProduct`, so only their
//! product strings tell them apart. No other RTL2832U fact is evidenced yet: the request
//! codes, register map, how the host reaches the tuner and the init order will come from
//! usbmon captures of the owner's RTL-SDR Blog V4 and Nooelec `NESDR SMArt v5`
//! (`CAPTURES.md`). Even the architecture the seam and simulator assume (vendor control
//! transfers plus one bulk IN stream, `RTL-GAP-12`; a demodulator-controlled gate in front
//! of the tuner's I2C port, `RTL-GAP-13`) is a GAP row until a capture shows it. Each value
//! below is a [`Pending`] naming its row in `PROVENANCE.md`; rows without a typed value
//! here have no code until a capture shows their shape, except the open (`RTL-GAP-07`),
//! which `Device::open` refuses with directly.

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

/// The RTL-SDR Blog V4's identity. prov: RTL-DESC-01
pub const BLOG_V4_ID: UsbId = UsbId {
    vendor: 0x0bda,
    product: 0x2838,
};

/// The Nooelec `NESDR SMArt v5`'s identity, the same as the Blog V4's. prov: RTL-DESC-03
pub const V5_ID: UsbId = UsbId {
    vendor: 0x0bda,
    product: 0x2838,
};

/// The Blog V4's interface and bulk IN endpoint for the sample stream. prov: RTL-DESC-02
pub const BLOG_V4_STREAM: UsbInterface = UsbInterface {
    number: 0,
    rx_endpoint: 0x81,
};

/// The `NESDR SMArt v5`'s interface and bulk IN endpoint for the sample stream. prov: RTL-DESC-04
pub const V5_STREAM: UsbInterface = UsbInterface {
    number: 0,
    rx_endpoint: 0x81,
};
