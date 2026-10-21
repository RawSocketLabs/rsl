//! libusdr's FPGA FIR gearbox (`ipblks/fgearbox.c`): the microcode its rate changers load
//! for each RX decimation and TX interpolation factor, on the 7-series FPGAs of the uSDR,
//! the LMS7 family and the `LSDr`. 32 DSP lines of 40 bits, a byte per register write; which register
//! and how far apart each driver writes them stay in the driver.
//!
//! **Unstable; unpublished.** The tables were `usdr`'s (`portable/fir_tables.rs`), moved
//! when the XSDR's DUC/DDC gateware needed them.
#![no_std]
#![forbid(unsafe_code)]

mod tables;

/// Bytes in one table (`UCODE_SIZE`: 32 DSP lines of 40 bits).
pub const UCODE_BYTES: usize = 1280;

/// A rate changer's factor, as libusdr's `fgearbox_firs_t` names the ones its 7-series
/// tables cover (`FGBOX_BP`, `FGBOX_X2`, ... `FGBOX_X32`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Factor {
    /// No rate change: the FIR passes samples through (`FGBOX_BP`, 1).
    X1,

    /// 2.
    X2,

    /// 4.
    X4,

    /// 8.
    X8,

    /// 16.
    X16,

    /// 32.
    X32,
}

impl Factor {
    /// Every factor, smallest first.
    pub const ALL: [Self; 6] = [Self::X1, Self::X2, Self::X4, Self::X8, Self::X16, Self::X32];

    /// The factor as a number (`fgearbox_firs_t`'s value).
    #[must_use]
    pub const fn get(self) -> u32 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::X8 => 8,
            Self::X16 => 16,
            Self::X32 => 32,
        }
    }

    /// The factor that is `factor`, if a table covers it.
    #[must_use]
    pub const fn from_factor(factor: u32) -> Option<Self> {
        Some(match factor {
            1 => Self::X1,
            2 => Self::X2,
            4 => Self::X4,
            8 => Self::X8,
            16 => Self::X16,
            32 => Self::X32,
            _ => return None,
        })
    }

    /// The decimator's microcode for this factor on a 7-series FPGA
    /// (`fgearbox_load_fir_ex` with `DSP_7SERIES`).
    #[must_use]
    pub const fn decimation_ucode(self) -> &'static [u8; UCODE_BYTES] {
        match self {
            Self::X1 => &tables::BYPASS,
            Self::X2 => &tables::DECIMATE_2,
            Self::X4 => &tables::DECIMATE_4,
            Self::X8 => &tables::DECIMATE_8,
            Self::X16 => &tables::DECIMATE_16,
            Self::X32 => &tables::DECIMATE_32,
        }
    }

    /// The interpolator's microcode for this factor on a 7-series FPGA
    /// (`fgearbox_load_fir_i_ex` with `DSP_7SERIES`).
    #[must_use]
    pub const fn interpolation_ucode(self) -> &'static [u8; UCODE_BYTES] {
        match self {
            Self::X1 => &tables::BYPASS,
            Self::X2 => &tables::INTERPOLATE_2,
            Self::X4 => &tables::INTERPOLATE_4,
            Self::X8 => &tables::INTERPOLATE_8,
            Self::X16 => &tables::INTERPOLATE_16,
            Self::X32 => &tables::INTERPOLATE_32,
        }
    }
}

/// A decimation factor only the decimator has a 7-series table for, beyond [`Factor`]'s
/// (`FGBOX_X3`, `FGBOX_X5`, `FGBOX_X6`, `FGBOX_X64`, `FGBOX_X128`): the `LSDr` loads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecimationOnly {
    /// 3.
    X3,

    /// 5.
    X5,

    /// 6.
    X6,

    /// 64.
    X64,

    /// 128.
    X128,
}

impl DecimationOnly {
    /// The decimator's microcode for this factor on a 7-series FPGA
    /// (`fgearbox_load_fir_ex` with `DSP_7SERIES`).
    #[must_use]
    pub const fn decimation_ucode(self) -> &'static [u8; UCODE_BYTES] {
        match self {
            Self::X3 => &tables::DECIMATE_3,
            Self::X5 => &tables::DECIMATE_5,
            Self::X6 => &tables::DECIMATE_6,
            Self::X64 => &tables::DECIMATE_64,
            Self::X128 => &tables::DECIMATE_128,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_factor_round_trips_and_only_those() {
        for factor in Factor::ALL {
            assert_eq!(Factor::from_factor(factor.get()), Some(factor));
        }
        assert_eq!(Factor::from_factor(3), None, "no 7-series x3 table here");
        assert_eq!(Factor::from_factor(0), None);
    }

    #[test]
    fn bypass_serves_both_directions_and_the_rest_differ() {
        assert_eq!(
            Factor::X1.decimation_ucode(),
            Factor::X1.interpolation_ucode()
        );
        assert_ne!(
            Factor::X2.decimation_ucode(),
            Factor::X2.interpolation_ucode()
        );
    }
}
