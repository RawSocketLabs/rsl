use thiserror::Error;

/// Error Codes as defined in <https://nuand.com/libbladeRF-doc/v2.5.0/group___r_e_t_c_o_d_e_s.html>
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BladeRfError {
    #[error("unexpected failure")]
    Unexpected = -1,
    #[error("value is outside the supported range")]
    Range = -2,
    #[error("invalid parameter")]
    Inval = -3,
    #[error("memory allocation failed")]
    Mem = -4,
    #[error("I/O failure")]
    Io = -5,
    #[error("operation timed out")]
    Timeout = -6,
    #[error("device is not present")]
    Nodev = -7,
    #[error("operation is unsupported")]
    Unsupported = -8,
    #[error("buffer is misaligned")]
    Misaligned = -9,
    #[error("checksum validation failed")]
    Checksum = -10,
    #[error("file was not found")]
    NoFile = -11,
    #[error("FPGA update is required")]
    UpdateFpga = -12,
    #[error("firmware update is required")]
    UpdateFw = -13,
    #[error("requested timestamp is in the past")]
    TimePast = -14,
    #[error("queue is full")]
    QueueFull = -15,
    #[error("FPGA operation failed")]
    FpgaOp = -16,
    #[error("permission denied")]
    Permission = -17,
    #[error("operation would block")]
    WouldBlock = -18,
    #[error("device is not initialized")]
    NotInit = -19,
    /// Arbitrarily chosen discriminant
    #[error("unknown libbladeRF error code {0}")]
    Unknown(i32) = i32::MIN,
}

impl BladeRfError {
    pub fn from_code(code: i32) -> Self {
        match code {
            -1 => Self::Unexpected,
            -2 => Self::Range,
            -3 => Self::Inval,
            -4 => Self::Mem,
            -5 => Self::Io,
            -6 => Self::Timeout,
            -7 => Self::Nodev,
            -8 => Self::Unsupported,
            -9 => Self::Misaligned,
            -10 => Self::Checksum,
            -11 => Self::NoFile,
            -12 => Self::UpdateFpga,
            -13 => Self::UpdateFw,
            -14 => Self::TimePast,
            -15 => Self::QueueFull,
            -16 => Self::FpgaOp,
            -17 => Self::Permission,
            -18 => Self::WouldBlock,
            -19 => Self::NotInit,
            x => Self::Unknown(x),
        }
    }
}

impl From<BladeRfError> for i32 {
    fn from(value: BladeRfError) -> Self {
        match value {
            BladeRfError::Unexpected => -1,
            BladeRfError::Range => -2,
            BladeRfError::Inval => -3,
            BladeRfError::Mem => -4,
            BladeRfError::Io => -5,
            BladeRfError::Timeout => -6,
            BladeRfError::Nodev => -7,
            BladeRfError::Unsupported => -8,
            BladeRfError::Misaligned => -9,
            BladeRfError::Checksum => -10,
            BladeRfError::NoFile => -11,
            BladeRfError::UpdateFpga => -12,
            BladeRfError::UpdateFw => -13,
            BladeRfError::TimePast => -14,
            BladeRfError::QueueFull => -15,
            BladeRfError::FpgaOp => -16,
            BladeRfError::Permission => -17,
            BladeRfError::WouldBlock => -18,
            BladeRfError::NotInit => -19,
            BladeRfError::Unknown(x) => x,
        }
    }
}

/// Failure while applying a [`crate::BladeRFModuleConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum BladeRFModuleConfigError {
    /// The device rejected the requested center frequency.
    #[error("failed to set bladeRF frequency: {0}")]
    Frequency(#[source] BladeRfError),
    /// The device rejected the requested sample rate.
    #[error("failed to set bladeRF sample rate: {0}")]
    SampleRate(#[source] BladeRfError),
    /// The device rejected the requested bandwidth.
    #[error("failed to set bladeRF bandwidth: {0}")]
    Bandwidth(#[source] BladeRfError),
    /// The device rejected the requested gain.
    #[error("failed to set bladeRF gain: {0}")]
    Gain(#[source] BladeRfError),
}
