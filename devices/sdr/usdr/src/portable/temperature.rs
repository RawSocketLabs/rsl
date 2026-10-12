//! TMP114 wire values and one bounded temperature read, TI SNIS214E §8.6.
//! Identification and thermal policy are executor prerequisites, not machine side effects.

use bnb::{BitEnum, bitfield, u4, u12};

use super::address::I2cAddr;
use super::error::Failure;

/// TMP114 register addresses; registers are 16 bits wide, MSB first.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Latest signed result, 1/128 °C per bit; `Temp_Result`, SNIS214E §8.6.1.
    Temperature = 0x00,
    /// Part and revision; `Device_ID`, SNIS214E §8.6.12.
    DeviceId = 0x0b,
}

/// Part and revision: `Device_ID`, SNIS214E §8.6.12 (read-only).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeviceId {
    /// Device revision, `Rev`, bits 15:12; 1 on the TMP114.
    #[bits(12..=15)]
    pub(crate) revision: u4,
    /// Device ID, `ID`, bits 11:0; 0x114 on the TMP114.
    #[bits(0..=11)]
    pub(crate) device: u12,
}

/// Result register counts per degree Celsius, SNIS214E §8.6.1.
const COUNTS_PER_CELSIUS: f32 = 128.0;

/// A semantic chip request, not USB framing: read exactly two bytes from `Temp_Result`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TemperatureRequest {
    /// Where the TMP114 answers.
    pub(crate) dev: I2cAddr,
    /// Selects `Temp_Result`, SNIS214E §8.6.1.
    pub(crate) register: [u8; 1],
}

/// One temperature read without I/O, clocks, allocation or runtime ownership.
/// The executor correlates its transfer before completing; duplicates produce no result.
pub(crate) struct TemperatureRead {
    /// Whether its one semantic read can still complete.
    pending: bool,
}

impl TemperatureRead {
    /// Begins a read on an already-identified sensor, returning its semantic request.
    pub(crate) fn begin(dev: I2cAddr) -> (Self, TemperatureRequest) {
        (
            Self { pending: true },
            TemperatureRequest {
                dev,
                register: [Reg::Temperature.into()],
            },
        )
    }

    /// Resolves once, retaining the executor's error unchanged and rejecting bad wire data.
    pub(crate) fn complete<E>(
        &mut self,
        result: Result<&[u8], E>,
    ) -> Option<Result<f32, Failure<E>>> {
        if !self.pending {
            return None;
        }
        self.pending = false;
        Some(result.map_err(Failure::Transport).and_then(|bytes| {
            let raw: [u8; 2] = bytes.try_into().map_err(|_| {
                Failure::InvalidData("TMP114 temperature reply must contain exactly two bytes")
            })?;
            Ok(f32::from(i16::from_be_bytes(raw)) / COUNTS_PER_CELSIUS)
        }))
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use super::*;

    #[test]
    fn malformed_data_is_a_protocol_failure_not_an_os_error() {
        let (mut operation, _) = TemperatureRead::begin(I2cAddr::new(0, 0x4e));
        assert!(matches!(
            operation.complete::<Infallible>(Ok(&[0])),
            Some(Err(Failure::InvalidData(_)))
        ));
        assert!(
            operation
                .complete::<Infallible>(Ok(&[0x0c, 0x80]))
                .is_none()
        );
    }

    #[test]
    fn transport_errors_need_no_std_error_trait_or_heap() {
        let (mut operation, _) = TemperatureRead::begin(I2cAddr::new(0, 0x4e));
        assert_eq!(
            operation.complete(Err(42_u8)),
            Some(Err(Failure::Transport(42)))
        );
        assert!(operation.complete::<u8>(Ok(&[0x0c, 0x80])).is_none());
    }
}
