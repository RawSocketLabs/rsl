//! Client commands: one opcode byte followed by a big-endian `u32` parameter.

use bnb::{BitEnum, bin};

/// A command opcode, per <https://k3xec.com/rtl-tcp/>. Dual-use: any other value is
/// preserved as [`Opcode::Other`].
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum Opcode {
    /// Tune to a center frequency, in Hz.
    SetCenterFrequency = 0x01,
    /// Set the sample rate, in complex samples per second.
    SetSampleRate = 0x02,
    /// Tuner gain mode: `0` automatic, `1` manual.
    SetGainMode = 0x03,
    /// Manual tuner gain, in tenths of a dB.
    SetGain = 0x04,
    /// Frequency correction, in PPM.
    SetFrequencyCorrection = 0x05,
    /// IF gain: stage in the high 16 bits, tenths of a dB in the low 16 bits.
    SetIfGain = 0x06,
    /// Test mode: `1` on, `0` off.
    SetTestMode = 0x07,
    /// Automatic gain correction: `1` on, `0` off. Independent of the tuner gain mode.
    SetAgcMode = 0x08,
    /// Direct sampling: `0` off, `1` I-ADC, `2` Q-ADC.
    SetDirectSampling = 0x09,
    /// Offset tuning: `1` on, `0` off.
    SetOffsetTuning = 0x0a,
    /// Tuner gain by index into the tuner's sorted gain table.
    SetTunerGainByIndex = 0x0d,
    /// Bias tee on GPIO pin 0: `1` on, `0` off.
    SetBiasTee = 0x0e,
    /// Any other opcode, preserved verbatim.
    #[catch_all]
    Other(u8),
}

/// One 5-byte client command.
///
/// The typed constructors encode the documented parameter for each opcode; build the
/// struct directly (or use [`Command::new`]) to send any opcode/parameter pair.
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Command {
    /// What the command does.
    pub opcode: Opcode,
    /// The opcode's parameter.
    pub param: u32,
}

impl Command {
    /// Any opcode with any parameter (no validation).
    #[must_use]
    pub const fn new(opcode: Opcode, param: u32) -> Self {
        Self { opcode, param }
    }

    /// Tune to `hz`.
    #[must_use]
    pub const fn set_center_frequency(hz: u32) -> Self {
        Self::new(Opcode::SetCenterFrequency, hz)
    }

    /// Stream at `samples_per_second`.
    #[must_use]
    pub const fn set_sample_rate(samples_per_second: u32) -> Self {
        Self::new(Opcode::SetSampleRate, samples_per_second)
    }

    /// Manual (`true`) or automatic (`false`) tuner gain.
    #[must_use]
    pub const fn set_gain_mode(manual: bool) -> Self {
        Self::new(Opcode::SetGainMode, manual as u32)
    }

    /// Manual tuner gain in tenths of a dB.
    #[must_use]
    pub const fn set_gain(tenths_db: u32) -> Self {
        Self::new(Opcode::SetGain, tenths_db)
    }

    /// Automatic gain correction on or off.
    #[must_use]
    pub const fn set_agc_mode(on: bool) -> Self {
        Self::new(Opcode::SetAgcMode, on as u32)
    }

    /// Tuner gain by gain-table index.
    #[must_use]
    pub const fn set_tuner_gain_by_index(index: u32) -> Self {
        Self::new(Opcode::SetTunerGainByIndex, index)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn opcodes_round_trip_and_preserve_unknown() {
        assert_eq!(u8::from(Opcode::SetCenterFrequency), 0x01);
        assert_eq!(u8::from(Opcode::SetTunerGainByIndex), 0x0d);
        assert_eq!(Opcode::from(0x08), Opcode::SetAgcMode);
        assert_eq!(Opcode::from(0x0b), Opcode::Other(0x0b));
        assert_eq!(u8::from(Opcode::Other(0x0b)), 0x0b);
    }

    #[test]
    fn boolean_constructors_encode_one_and_zero() {
        assert_eq!(Command::set_gain_mode(true).param, 1);
        assert_eq!(Command::set_gain_mode(false).param, 0);
        assert_eq!(Command::set_agc_mode(true).param, 1);
        assert_eq!(Command::set_agc_mode(false).param, 0);
    }
}

#[cfg(test)]
mod component {
    use super::*;

    #[test]
    fn command_round_trips_big_endian() {
        let wire = [0x02, 0x00, 0x1f, 0x40, 0x00];
        let command = Command::decode_exact(&wire).unwrap();
        assert_eq!(command, Command::set_sample_rate(2_048_000));
        assert_eq!(command.to_bytes().unwrap(), wire);
    }
}
