//! The processors' operations beyond enabling them: decimation and interpolation, the DC
//! corrector, the mixer's NCO and the built-in self-test (`lms7002m_xxtsp_*`,
//! `lms7002m_rxtsp_dc_corr`).

use bnb::{u3, u4};
use hal::{Sequence, SequenceAction, SequenceFailure};

use super::{Lms7002m, NoChannel, TSG_DIV8, Tsp};
use crate::batch::Batch;
use crate::reg::Reg;
use crate::register::Register;
use crate::regs::{
    BistStatus, Capture, CaptureAdc, Mac, MixerDirection, NcoMode, RxTspBypass, RxTspDc,
    RxTspDecimation, RxTspMode, TsgMode, TsgScale, TspInput, TxTspInterpolation, TxTspMode,
};
use crate::step::Read;

/// One channel's cached processor words: the bypass word (`reg_*tsp_dscpcfg`) and the
/// decimation or interpolation word (`reg_*tsp_hbdo_iq`), raw as libusdr keeps them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TspWords {
    /// `RXTSP_0x040C` or `TXTSP_0x0208`.
    pub(super) config: u16,

    /// `RXTSP_0x0403` or `TXTSP_0x0203`.
    pub(super) decimation: u16,
}

impl TspWords {
    /// As `lms7002m_create` and the zeroed state leave them.
    pub(super) const ZERO: Self = Self {
        config: 0,
        decimation: 0,
    };
}

impl Lms7002m {
    /// The processor's cached words, A then B.
    pub(super) fn dsp_mut(&mut self, tsp: Tsp) -> &mut [TspWords; 2] {
        match tsp {
            Tsp::Rx => &mut self.rx_dsp,
            Tsp::Tx => &mut self.tx_dsp,
        }
    }

    /// Sets the processor's half-band stages to decimate or interpolate by `2^order` on
    /// both channels (`lms7002m_xxtsp_int_dec`): order 0 bypasses them (the field takes
    /// `order - 1`, which wraps to 7). Written bank by bank, then the selection restored.
    ///
    /// # Errors
    ///
    /// [`NoChannel`] when no bank is selected; nothing changes.
    pub fn tsp_int_dec(&mut self, tsp: Tsp, order: u32) -> Result<Batch, NoChannel> {
        self.selected()?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the 3-bit field keeps the low bits, as the C macro masks them"
        )]
        let field = u3::from_raw(order.wrapping_sub(1) as u8);
        let (reg, words) = match tsp {
            Tsp::Rx => (Reg::RxTspDecimation, &mut self.rx_dsp),
            Tsp::Tx => (Reg::TxTspInterpolation, &mut self.tx_dsp),
        };
        for words in words.iter_mut() {
            words.decimation = match tsp {
                Tsp::Rx => RxTspDecimation::from_raw(words.decimation)
                    .with_order(field)
                    .to_raw(),
                Tsp::Tx => TxTspInterpolation::from_raw(words.decimation)
                    .with_order(field)
                    .to_raw(),
            };
        }
        let words = *words;
        let mut batch = Batch::new(self.target);
        for (bank, words) in [Mac::A, Mac::B].into_iter().zip(words) {
            batch.push(self.mac.with_mac(bank).entry());
            batch.push((reg, words.decimation));
        }
        batch.push(self.mac.entry());
        Ok(batch)
    }

    /// Bypasses the receive DC corrector or not on the selected channels and sets its
    /// averaging window (`lms7002m_rxtsp_dc_corr`), then rewrites the bypass words.
    ///
    /// # Errors
    ///
    /// [`NoChannel`] when no bank is selected; nothing changes.
    pub fn rxtsp_dc_corr(&mut self, bypass: bool, window: u3) -> Result<Batch, NoChannel> {
        let selected = self.selected()?;
        for (words, _) in self.rx_dsp.iter_mut().zip(selected).filter(|(_, on)| *on) {
            words.config = RxTspBypass::from_raw(words.config)
                .with_dc_bypassed(bypass)
                .to_raw();
        }
        let mut batch = Batch::new(self.target);
        batch.push(RxTspDc::new().with_window(window).entry());
        self.push_config_words(Tsp::Rx, &mut batch);
        Ok(batch)
    }

    /// Sets the processor's mixer to shift by `frequency` (a 32-bit fraction of the
    /// converter clock; negative down-converts, 0 bypasses the mixer) on the selected
    /// channels (`lms7002m_xxtsp_cmix`): the NCO's frequency word and mode, then the
    /// bypass words.
    ///
    /// libusdr marks the bypass in the receive layout for both processors, so for TX it
    /// sets `ISINC_BYP` (bit 7) where `CMIX_BYP` is bit 8; that is reproduced.
    ///
    /// # Errors
    ///
    /// [`NoChannel`] when no bank is selected; nothing changes.
    pub fn tsp_cmix(&mut self, tsp: Tsp, frequency: i32) -> Result<Batch, NoChannel> {
        let selected = self.selected()?;
        let direction = if frequency < 0 {
            MixerDirection::Down
        } else {
            MixerDirection::Up
        };
        for (words, _) in self
            .dsp_mut(tsp)
            .iter_mut()
            .zip(selected)
            .filter(|(_, on)| *on)
        {
            words.config = RxTspBypass::from_raw(words.config)
                .with_mixer_bypassed(frequency == 0)
                .with_mixer_direction(direction)
                .to_raw();
        }
        let [b0, b1, b2, b3] = frequency.unsigned_abs().to_le_bytes();
        let (fcw_high, fcw_low, mode) = match tsp {
            Tsp::Rx => (Reg::RxNcoFcwHigh, Reg::RxNcoFcwLow, Reg::RxNcoMode),
            Tsp::Tx => (Reg::TxNcoFcwHigh, Reg::TxNcoFcwLow, Reg::TxNcoMode),
        };
        let mut batch = Batch::new(self.target);
        batch.push((fcw_high, u16::from_le_bytes([b2, b3])));
        batch.push((fcw_low, u16::from_le_bytes([b0, b1])));
        batch.push((mode, NcoMode::new().with_dither(u4::new(1)).to_raw()));
        self.push_config_words(tsp, &mut batch);
        Ok(batch)
    }

    /// Starts the processor's built-in self-test (`lms7002m_xxtsp_bst`): its mode word
    /// written enabled, then again with `BSTART` set.
    pub fn tsp_bist_start(&self, tsp: Tsp) -> Batch {
        let mut batch = Batch::new(self.target);
        match tsp {
            Tsp::Rx => {
                let mode = rx_mode().with_enabled(true);
                batch.push(mode.entry());
                batch.push(mode.with_bstart(true).entry());
            }
            Tsp::Tx => {
                let mode = tx_mode().with_enabled(true);
                batch.push(mode.entry());
                batch.push(mode.with_bstart(true).entry());
            }
        }
        batch
    }

    /// Resets the processor (`lms7002m_xxtsp_reset`): its mode word written disabled, then
    /// enabled.
    pub fn tsp_reset(&self, tsp: Tsp) -> Batch {
        let mut batch = Batch::new(self.target);
        match tsp {
            Tsp::Rx => {
                batch.push(rx_mode().entry());
                batch.push(rx_mode().with_enabled(true).entry());
            }
            Tsp::Tx => {
                batch.push(tx_mode().entry());
                batch.push(tx_mode().with_enabled(true).entry());
            }
        }
        batch
    }

    /// Reads whether the processor's self-test has finished
    /// (`lms7002m_xxtsp_bst_isdone`).
    #[must_use]
    pub fn bist_check(&self, tsp: Tsp) -> BistCheck {
        let mut select = Batch::new(self.target);
        let status = match tsp {
            Tsp::Rx => {
                let mode = rx_mode()
                    .with_capture_select(Capture::BsigIBstate)
                    .with_enabled(true);
                select.push(mode.entry());
                select.push(mode.with_capture(true).entry());
                Reg::RxTspBistStatus
            }
            Tsp::Tx => Reg::TxTspBistStatus,
        };
        BistCheck {
            select,
            status: Read::new(self.target, status),
            failed: false,
        }
    }

    /// Appends libusdr's rewrite of the cached bypass words
    /// (`_lms7002m_xxtsp_wregwith_dspcfg`): with both banks selected, bank by bank and the
    /// selection restored; with one, that bank's word alone.
    fn push_config_words(&self, tsp: Tsp, batch: &mut Batch) {
        let (reg, words) = match tsp {
            Tsp::Rx => (Reg::RxTspBypass, self.rx_dsp),
            Tsp::Tx => (Reg::TxTspBypass, self.tx_dsp),
        };
        match self.mac.mac() {
            Mac::Both => {
                for (bank, words) in [Mac::A, Mac::B].into_iter().zip(words) {
                    batch.push(self.mac.with_mac(bank).entry());
                    batch.push((reg, words.config));
                }
                batch.push(self.mac.entry());
            }
            Mac::A => batch.push((reg, words[0].config)),
            Mac::B => batch.push((reg, words[1].config)),
            Mac::None => {}
        }
    }
}

/// The receive mode word libusdr's processor operations share: RSSI capture of the
/// input, the test generator at DC, −6 dB, /8, input from `LimeLight`, disabled.
fn rx_mode() -> RxTspMode {
    RxTspMode::new()
        .with_capture_select(Capture::Rssi)
        .with_capture_adc(CaptureAdc::Input)
        .with_tsg_scale(TsgScale::Minus6Db)
        .with_tsg_frequency(TSG_DIV8)
        .with_tsg_mode(TsgMode::Dc)
        .with_input(TspInput::Lml)
}

/// The transmit counterpart of [`rx_mode`].
fn tx_mode() -> TxTspMode {
    TxTspMode::new()
        .with_tsg_scale(TsgScale::Minus6Db)
        .with_tsg_frequency(TSG_DIV8)
        .with_tsg_mode(TsgMode::Dc)
        .with_input(TspInput::Lml)
}

/// `lms7002m_xxtsp_bst_isdone`: for RX, the mode word selecting the self-test state for
/// capture, written then latched; then the status read.
#[derive(Clone, Copy, Debug)]
pub struct BistCheck {
    /// The capture selection (RX only; empty for TX).
    select: Batch,

    /// The status read.
    status: Read,

    /// A selection write failed: libusdr skips the read (`res ? res :`).
    failed: bool,
}

impl BistCheck {
    /// Whether the self-test has finished (status bit 0 clear), once read.
    #[must_use]
    pub fn done(&self) -> Option<bool> {
        self.status
            .value()
            .map(|status| !BistStatus::from_raw(status).running())
    }
}

impl Sequence for BistCheck {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.select.next().or_else(|| self.status.next())
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.failed {
            return Ok(());
        }
        if self.select.next().is_some() {
            return self
                .select
                .complete(result)
                .inspect_err(|_| self.failed = true);
        }
        self.status.complete(result)
    }
}

#[cfg(test)]
mod tests {
    use hal::SpiAddr;

    use super::*;

    /// A chip as `lms7002m_create` leaves it, both banks selected.
    fn created() -> Lms7002m {
        Lms7002m::created(SpiAddr(0))
    }

    #[test]
    fn decimation_order_zero_bypasses_the_half_band_stages() {
        let mut chip = created();
        let batch = chip.tsp_int_dec(Tsp::Rx, 0).unwrap();
        let (_, word) = batch.writes()[1];
        assert_eq!(
            RxTspDecimation::from_raw(word).order().value(),
            7,
            "order - 1 wraps to the bypass code"
        );
        assert_eq!(batch.writes().len(), 5, "both banks, then the selection");
    }

    #[test]
    fn the_mixer_bypass_follows_the_frequency_on_the_selected_channel_only() {
        let mut chip = created();
        let _ = chip.mac_set(Mac::B);
        let batch = chip.tsp_cmix(Tsp::Rx, -1).unwrap();
        let (reg, word) = *batch.writes().last().unwrap();
        assert_eq!(reg, Reg::RxTspBypass, "one bank: its word alone");
        let config = RxTspBypass::from_raw(word);
        assert!(!config.mixer_bypassed());
        assert_eq!(config.mixer_direction(), MixerDirection::Down);
        assert_eq!(
            chip.rx_dsp[0].config, 0,
            "channel A's word is not selected, so not changed"
        );
    }

    #[test]
    fn a_tx_processor_check_reads_its_status_without_selecting_a_capture() {
        let mut check = created().bist_check(Tsp::Tx);
        assert!(matches!(
            check.next(),
            Some(SequenceAction::Fpga(hal::FpgaAccess::Spi {
                reg: 0x0209,
                write: false,
                ..
            }))
        ));
        check.complete::<()>(Ok(1)).unwrap();
        assert_eq!(check.done(), Some(false), "bit 0 set: still running");
    }
}
