//! The receive NCO: `set_nco` in `device/m2_lsdr/m2_da09_4_ad45_2.c`, which
//! `/dm/sdr/0/rx/frequency` runs with every channel selected.
//!
//! libusdr selects the channels ([`select_channels`]), refuses an ADC without a rate
//! (`-EINVAL`, after that write), folds the offset into one ADC period, shifts it by a
//! quarter of the ADC rate and writes the 32-bit phase increment `offset × 2³² / rate`
//! a byte per GPO bank, after setting the second mixer to 0 ([`SetNco`]).

use hal::{FpgaAccess, SequenceAction};

use super::fpga::{self, ALL_CHANNELS, Gpo};
use super::steps::Steps;

/// The channel selection `set_nco` writes first, whatever follows (`chans` 0xF).
pub(crate) const fn select_channels() -> FpgaAccess {
    Gpo::ChannelMask.write(ALL_CHANNELS)
}

/// The NCO's phase increment for `offset` on an ADC at `adc_rate` S/s, or `None` without
/// a rate (libusdr's `-EINVAL`), with libusdr's integer arithmetic: `offset` is the low 32 bits of the frequency asked for
/// (its `int`), compared and folded as unsigned (`while (offset > adc_rate)`), so a
/// negative offset folds from 2³² down; then `offset − rate / 4` as a signed word, times
/// 2³³, divided by the rate and by 2, truncated to 32 bits.
pub(crate) fn phase_increment(adc_rate: u32, offset: u32) -> Option<u32> {
    if adc_rate == 0 {
        return None;
    }
    let folded = if offset > adc_rate {
        (offset - 1) % adc_rate + 1
    } else {
        offset
    };
    let shifted = i64::from(i32::from_ne_bytes(
        folded.wrapping_sub(adc_rate / 4).to_ne_bytes(),
    ));
    let increment = shifted.wrapping_shl(33) / i64::from(adc_rate) / 2;
    let [a, b, c, d, ..] = increment.to_le_bytes();
    Some(u32::from_le_bytes([a, b, c, d]))
}

/// `set_nco`'s writes after the check: the second mixer to 0, then the increment's bytes,
/// least significant first.
pub(crate) type SetNco = Steps<5>;

/// The writes setting `increment` ([`phase_increment`]).
pub(crate) fn set_nco(increment: u32) -> SetNco {
    let gpo = |bank: Gpo, data| (SequenceAction::Fpga(bank.write(data)), fpga::NAME);
    let [b0, b1, b2, b3] = increment.to_le_bytes();
    Steps::new([
        gpo(Gpo::DspMix, 0),
        gpo(Gpo::Nco0, b0),
        gpo(Gpo::Nco1, b1),
        gpo(Gpo::Nco2, b2),
        gpo(Gpo::Nco3, b3),
    ])
}

#[cfg(test)]
mod tests {
    use hal::Sequence;

    use super::*;

    #[test]
    fn ten_mhz_at_250_msps_is_libusdrs_word() {
        assert_eq!(phase_increment(250_000_000, 10_000_000), Some(0xca3d_70a4));
        assert_eq!(phase_increment(0, 10_000_000), None);
        let mut nco = set_nco(0xca3d_70a4);
        let mut words = [0; 5];
        for word in &mut words {
            let Some(SequenceAction::Fpga(FpgaAccess::Write { value, .. })) = nco.next() else {
                panic!("five GPO writes");
            };
            *word = value;
            nco.complete::<()>(Ok(0)).unwrap();
        }
        assert_eq!(nco.next(), None);
        assert_eq!(
            words,
            [
                0x0900_0000,
                0x0a00_00a4,
                0x0b00_0070,
                0x0c00_003d,
                0x0d00_00ca
            ]
        );
    }

    #[test]
    fn offsets_fold_into_one_adc_period_as_unsigned_words() {
        let rate = 100_000_000;
        // A quarter of the rate is the mixer's centre: increment 0.
        assert_eq!(phase_increment(rate, rate / 4), Some(0));
        assert_eq!(
            phase_increment(rate, rate + rate / 4),
            phase_increment(rate, rate / 4)
        );
        assert_eq!(
            phase_increment(rate, 2 * rate),
            phase_increment(rate, rate),
            "the fold stops at the rate itself"
        );
        // -1 Hz is 2^32 - 1, folded down by whole periods to 94 967 295.
        assert_eq!(
            phase_increment(rate, u32::MAX),
            phase_increment(rate, 94_967_295)
        );
    }
}
