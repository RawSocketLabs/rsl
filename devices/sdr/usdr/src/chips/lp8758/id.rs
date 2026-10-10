//! The chip's revision: its OTP image and its silicon.

/// The OTP image revision (`OTP_REV`, §7.6.1.1) and the device revision (`DEV_REV` in
/// libusdr, at 0x00, which SNVSAC6B's map does not list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Revision {
    /// OTP image revision; 0xE0 on the LP8758-E0.
    otp: u8,

    /// Device revision.
    device: u8,
}

impl Revision {
    /// The revision from its two register bytes.
    pub(super) const fn new(otp: u8, device: u8) -> Self {
        Self { otp, device }
    }

    /// libusdr's single-number form, `OTP_REV << 8 | DEV_REV`, for error reports.
    pub(super) const fn to_raw(self) -> u16 {
        u16::from_be_bytes([self.otp, self.device])
    }
}
