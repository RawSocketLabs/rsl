//! Each chain's two NCO channels: their own LOs and baseband offsets around the one PLL LO,
//! as libusdr's `freq_data` (`rx_raw`, `tx_raw`) keeps them for `usdr_rfic_fe_set_freq`
//! (`device/m2_lm6_1/usdr_ctrl.c`), reached through `/dm/sdr/0/{rx,tx}/frequency[/0|/1]`
//! and `/dm/sdr/0/{rx,tx}/frequency/bb[/0|/1]`.
//!
//! The PLL tunes the average of the channels' LOs; each channel's NCO then shifts by its
//! baseband offset, the distance from that average unless set directly. A whole-chain
//! tune gives both channels the same LO, so both offsets 0.

use super::Board;
use super::tune::Chain;
use crate::error::Error;

/// Which of a chain's two NCO channels a frequency call sets (libusdr's channel mask, from
/// the path's `/0` or `/1`, or both without one).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NcoChannels {
    /// Both channels (no suffix: mask 3).
    #[default]
    Both,

    /// Channel 0 (`/0`: mask 1), the NCO an RX stream's channel 0 sees.
    Channel0,

    /// Channel 1 (`/1`: mask 2).
    Channel1,
}

impl NcoChannels {
    /// Whether each channel, 0 then 1, is selected.
    const fn selects(self) -> [bool; 2] {
        match self {
            Self::Both => [true, true],
            Self::Channel0 => [true, false],
            Self::Channel1 => [false, true],
        }
    }
}

/// One chain's per-channel frequencies (libusdr's `freq_data_t`): each channel's LO and
/// baseband offset, unset until a call sets them.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FrequencyPlan {
    /// Each channel's LO, in Hz (`lo[i]`).
    lo_hz: [Option<u32>; 2],

    /// Each channel's NCO offset, in Hz (`bb[i]`).
    baseband_hz: [Option<i32>; 2],
}

impl FrequencyPlan {
    /// Sets the LO of `channels` to `hz`, then makes every channel with an LO offset by its
    /// distance from their average, and returns that average, the LO the PLL is to tune.
    ///
    /// libusdr's arithmetic: the average in `double`, truncated to `unsigned` for the
    /// chain's LO; each offset truncated towards zero to `int32_t`.
    pub(super) fn set_lo(&mut self, channels: NcoChannels, hz: u32) -> u32 {
        for (lo, selected) in self.lo_hz.iter_mut().zip(channels.selects()) {
            if selected {
                *lo = Some(hz);
            }
        }
        let (sum, count) = self
            .lo_hz
            .iter()
            .flatten()
            .fold((0.0, 0_u8), |(sum, count), &lo| {
                (sum + f64::from(lo), count + 1)
            });
        let average = sum / f64::from(count);
        for (lo, baseband) in self.lo_hz.iter().zip(&mut self.baseband_hz) {
            if let Some(lo) = lo {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "libusdr truncates the double to int32_t; half the gap of two \
                              u32s fits"
                )]
                let offset = (f64::from(*lo) - average) as i32;
                *baseband = Some(offset);
            }
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "libusdr truncates the average of u32s, itself within u32, to unsigned"
        )]
        let average = average as u32;
        average
    }

    /// Sets the baseband offset of `channels` to `hz`.
    pub(super) fn set_baseband(&mut self, channels: NcoChannels, hz: i32) {
        for (baseband, selected) in self.baseband_hz.iter_mut().zip(channels.selects()) {
            if selected {
                *baseband = Some(hz);
            }
        }
    }

    /// Channel `channel`'s baseband offset, 0 while unset, as libusdr's NCO restore reads
    /// it.
    pub(super) fn baseband_hz(&self, channel: usize) -> i32 {
        self.baseband_hz[channel].unwrap_or(0)
    }

    /// The largest offset magnitude among set offsets (libusdr's `nco_distance`), which
    /// widens a filter that follows the rate by twice its value.
    pub(super) fn nco_distance_hz(&self) -> u32 {
        self.baseband_hz
            .iter()
            .flatten()
            .map(|hz| hz.unsigned_abs())
            .max()
            .unwrap_or(0)
    }
}

impl Board {
    /// Tunes `channels` of the receiver to `hz` and the PLL to the average of the channels'
    /// LOs (`usdr_rfic_fe_set_freq` with `FE_FREQ_LO_RX` and a channel mask).
    pub(crate) fn set_rx_channel_frequency(
        &mut self,
        channels: NcoChannels,
        hz: u32,
    ) -> Result<(), Error> {
        let lo = self.rx.plan.set_lo(channels, hz);
        self.retune(Chain::Rx, lo)
    }

    /// Tunes `channels` of the transmitter, as [`Self::set_rx_channel_frequency`] does the
    /// receiver's (`FE_FREQ_LO_TX`).
    pub(crate) fn set_tx_channel_frequency(
        &mut self,
        channels: NcoChannels,
        hz: u32,
    ) -> Result<(), Error> {
        let lo = self.tx.plan.set_lo(channels, hz);
        self.retune(Chain::Tx, lo)
    }

    /// Sets the RX baseband offset of `channels`, then tunes the receiver again to its LO
    /// (`usdr_rfic_fe_set_freq` with `FE_FREQ_BB_RX`).
    pub(crate) fn set_rx_baseband_offset(
        &mut self,
        channels: NcoChannels,
        hz: i32,
    ) -> Result<(), Error> {
        self.rx.plan.set_baseband(channels, hz);
        self.retune(Chain::Rx, self.rx.lo_hz)
    }

    /// Sets the TX baseband offset of `channels`, then tunes the transmitter again to its
    /// LO (`FE_FREQ_BB_TX`).
    pub(crate) fn set_tx_baseband_offset(
        &mut self,
        channels: NcoChannels,
        hz: i32,
    ) -> Result<(), Error> {
        self.tx.plan.set_baseband(channels, hz);
        self.retune(Chain::Tx, self.tx.lo_hz)
    }

    /// Tunes `chain` to `lo` with its current plan.
    fn retune(&mut self, chain: Chain, lo: u32) -> Result<(), Error> {
        match chain {
            Chain::Rx => self.tune_rx(lo),
            Chain::Tx => self.tune_tx(lo),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_chain_tune_leaves_both_offsets_zero() {
        let mut plan = FrequencyPlan::default();
        assert_eq!(plan.set_lo(NcoChannels::Both, 1_000_000_000), 1_000_000_000);
        assert_eq!((plan.baseband_hz(0), plan.baseband_hz(1)), (0, 0));
        assert_eq!(plan.nco_distance_hz(), 0);
    }

    #[test]
    fn two_channel_los_centre_the_pll_and_split_the_gap() {
        let mut plan = FrequencyPlan::default();
        plan.set_lo(NcoChannels::Channel0, 1_000_000_000);
        // 1000 and 1000.000003 MHz: the average's half hertz is truncated, and so is each
        // offset, towards zero.
        assert_eq!(
            plan.set_lo(NcoChannels::Channel1, 1_000_000_003),
            1_000_000_001
        );
        assert_eq!((plan.baseband_hz(0), plan.baseband_hz(1)), (-1, 1));
        assert_eq!(plan.nco_distance_hz(), 1);
    }

    #[test]
    fn a_lone_channel_lo_leaves_the_other_offset_unset() {
        let mut plan = FrequencyPlan::default();
        plan.set_baseband(NcoChannels::Channel0, -5_000);
        assert_eq!(plan.set_lo(NcoChannels::Channel1, 900_000_000), 900_000_000);
        assert_eq!(
            (plan.baseband_hz(0), plan.baseband_hz(1)),
            (-5_000, 0),
            "channel 0 keeps its offset: it has no LO to re-centre"
        );
        assert_eq!(plan.nco_distance_hz(), 5_000);
    }

    #[test]
    fn the_distance_is_the_largest_offset_magnitude() {
        let mut plan = FrequencyPlan::default();
        plan.set_baseband(NcoChannels::Both, 2_000);
        plan.set_baseband(NcoChannels::Channel1, i32::MIN);
        assert_eq!(plan.nco_distance_hz(), 1 << 31);
    }
}
