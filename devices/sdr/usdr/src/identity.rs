//! What the board reports about itself.

use crate::fpga::Hwid;

/// The board's identity as its gateware reports it: libusdr's `/dm/revision`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Identity {
    /// The board revision, hardware ID bits 15:8; this driver supports 1 to 3.
    pub revision: u8,
    /// The gateware has an RX chain (hardware ID bit 25).
    pub has_rx: bool,
    /// The gateware has a TX chain (hardware ID bit 24).
    pub has_tx: bool,
    /// The gateware build identifier (the FPGA's `USR_ACCESS2`), which libusdr reports and
    /// checks before TX streaming.
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

    /// The TX chain has the extended stream core (hardware ID bit 26) rather than the
    /// original one.
    #[must_use]
    pub const fn has_extended_tx_core(self) -> bool {
        Hwid::from_raw(self.hwid).has_extended_tx_core()
    }

    /// The most channels an RX stream carries ([`RxChannels`](crate::RxChannels)): 2 with
    /// an RX chain, whose two NCOs each feed one, else 1 (libusdr's
    /// `/ll/sdr/max_sw_rx_chans`). The board has one receiver either way
    /// (`/ll/sdr/max_hw_rx_chans`, 1).
    #[must_use]
    pub const fn max_rx_channels(self) -> u8 {
        if self.has_rx { 2 } else { 1 }
    }

    /// The most channels a TX stream carries ([`TxChannels`](crate::TxChannels)): 2 with a
    /// TX chain, whose two NCOs each take one, else 1 (libusdr's
    /// `/ll/sdr/max_sw_tx_chans`). The board has one transmitter either way
    /// (`/ll/sdr/max_hw_tx_chans`, 1).
    #[must_use]
    pub const fn max_tx_channels(self) -> u8 {
        if self.has_tx { 2 } else { 1 }
    }
}
