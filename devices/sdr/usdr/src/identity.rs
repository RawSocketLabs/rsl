//! What the board reports about itself.

/// The board's identity as its gateware reports it: libusdr's `/dm/revision`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Identity {
    /// The board revision, hardware ID bits 15:8; this driver supports 1 to 3.
    pub revision: u8,
    /// The gateware has an RX chain (hardware ID bit 25).
    pub has_rx: bool,
    /// The gateware has a TX chain (hardware ID bit 24). This driver does not drive TX yet.
    pub has_tx: bool,
    /// The gateware build identifier (the FPGA's `USR_ACCESS2`), which libusdr only reports.
    pub gateware_build: u32,
    /// The whole hardware ID word (FPGA GPI 12).
    pub hwid: u32,
}

impl Identity {
    /// libusdr's single-number form: the hardware ID in the high 32 bits, the gateware
    /// build in the low.
    #[must_use]
    pub const fn to_raw(self) -> u64 {
        (self.hwid as u64) << 32 | self.gateware_build as u64
    }
}
