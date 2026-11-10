//! The RTL2832U USB demodulator side of the dongle.
//!
//! The identities and stream interfaces come from the dongles' descriptor dumps
//! (`DESC-V4-0001`, `DESC-V5-0001`). Both share one `idVendor`:`idProduct`, so only their
//! product strings tell them apart. How the host addresses the dongle's registers and the
//! tuner's I2C port comes from captures of librtlsdr's traffic on the `NESDR SMArt v5`
//! (`GPLOBS-V5-0001` to `-0003`, class GPLOBS): every vendor transfer, the paged register
//! block, the addressed blocks, the I2C block and the tuner gate; the open and close are in
//! [`super::sequence`]. The register meanings and the stream setup are still to come
//! (`RTL-GAP-09` to `RTL-GAP-11`); block names here are this crate's own, by `wIndex`.

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

/// `bmRequestType` of every vendor transfer, direction bit clear (the port sets it for
/// reads). prov: RTL-OBS-01
pub const REQUEST_TYPE: u8 = 0x40;

/// `bRequest` of every vendor transfer. prov: RTL-OBS-01
pub const VENDOR_REQUEST: u8 = 0x00;

/// The `wIndex` bit set on every write and clear on every read. prov: RTL-OBS-02
pub const WRITE: u16 = 0x0010;

/// The low byte of `wValue` in every paged-block access. prov: RTL-OBS-03
pub const PAGED_VALUE: u16 = 0x0020;

/// The block whose `wValue` is an I2C bus address. prov: RTL-OBS-06
pub const I2C_BLOCK: u8 = 6;

/// A page of the paged block (block 0): below 0x10, since `wIndex` bit 4 is [`WRITE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Page(u8);

impl Page {
    /// Page `page`, if it leaves `wIndex` bit 4 to [`WRITE`]. prov: RTL-OBS-03
    #[must_use]
    pub const fn new(page: u8) -> Option<Self> {
        if page < 0x10 { Some(Self(page)) } else { None }
    }
}

/// An addressed block: one written by a 16-bit address in `wValue`. prov: RTL-OBS-05
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Block {
    /// Block 1 (`0x2000`, `0x2148`, `0x2158` written).
    One = 1,

    /// Block 2 (`0x3000`, `0x300b` written).
    Two = 2,
}

/// Paged-block page 0. prov: RTL-OBS-03
pub const PAGE_0: Page = Page(0);

/// Paged-block page 1. prov: RTL-OBS-03
pub const PAGE_1: Page = Page(1);

/// Where a vendor transfer lands: its `wValue` and its read `wIndex` ([`WRITE`] clear).
/// Built only by the constructors below, so a page stays below `0x10`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    /// `wValue`.
    value: u16,

    /// `wIndex` for a read; a write sets [`WRITE`].
    index: u16,
}

impl Location {
    /// Register `register` of paged-block page `page`. prov: RTL-OBS-03
    #[must_use]
    pub const fn paged(page: Page, register: u8) -> Self {
        Self {
            value: (register as u16) << 8 | PAGED_VALUE,
            index: page.0 as u16,
        }
    }

    /// Address `address` of block `block`, whose `wValue` is the address itself; only
    /// writes are seen, so the read encoding is inferred from the I2C block's.
    /// prov: RTL-OBS-05
    #[must_use]
    pub const fn block(block: Block, address: u16) -> Self {
        Self::addressed(block as u8, address)
    }

    /// Address `address` of block number `block`.
    const fn addressed(block: u8, address: u16) -> Self {
        Self {
            value: address,
            index: (block as u16) << 8,
        }
    }

    /// The I2C device at 7-bit address `address`, which the wire carries shifted left by one
    /// in both directions; `None` past 7 bits. prov: RTL-OBS-06
    #[must_use]
    pub const fn i2c(address: u8) -> Option<Self> {
        if address < 0x80 {
            Some(Self::addressed(I2C_BLOCK, (address as u16) << 1))
        } else {
            None
        }
    }

    /// `wValue`.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.value
    }

    /// `wIndex` for a read.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.index
    }

    /// `wIndex` for a write.
    #[must_use]
    pub const fn write_index(self) -> u16 {
        self.index | WRITE
    }
}

/// The paged-block register read back, one byte, right after every paged-block write; why
/// is not known. prov: RTL-OBS-04
pub const POST_WRITE_READ: Location = Location::paged(Page(0x0a), 0x01);

/// The paged-block register whose bit 3 opens the tuner's I2C port. prov: RTL-OBS-07
pub const GATE: Location = Location::paged(Page(0x01), 0x01);

/// What [`GATE`] is written to open the tuner's port. prov: RTL-OBS-07
pub const GATE_OPEN: u8 = 0x18;

/// What [`GATE`] is written to close it again. prov: RTL-OBS-07
pub const GATE_CLOSED: u8 = 0x10;
