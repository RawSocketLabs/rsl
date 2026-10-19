//! The chip's cached state and the operations that read it: libusdr's
//! `lms7002m_state_t` and the `hw/lms7002m/lms7002m.c` functions ported so far.
//!
//! Each operation updates the cache, then returns the [`Batch`] of writes libusdr posts.
//! libusdr updates its cache before posting and keeps the update when a write fails; so
//! does this: the cache describes what was asked for, not what the chip acknowledged.
//!
//! The operations live beside the block they drive: the front ends and AFE here, the
//! `LimeLight` interface in `lml`, the processors in `dsp`, the basebands in `baseband`,
//! the gains in `gain` and the synthesizers' disables in `synth`.

mod baseband;
mod dc;
mod dsp;
mod gain;
mod lml;
mod synth;

pub use baseband::{LpfParams, RbbMode, RbbPath, TbbMode, TbbPath};
pub use dc::DcDac;
pub use dsp::{BistCheck, TspGenerator};
pub use gain::{RxGainStage, TxGainStage};
pub use lml::{LmlConfig, LmlMap};
pub use synth::{CgenTune, SxxTune, Synthesizer, TuneError};

use bnb::{u2, u3, u4, u5};
use hal::SpiAddr;

use super::batch::Batch;
use super::register::Register;
use super::regs::{
    AdcMode, Afe, AfeMux1, AfeMux2, EnDir, LmlControl, Mac, NextChannel, RxBias, RxFrontEnd,
    RxInput, RxPath, RxTspBypass, RxTspMode, TsgMode, TxBand, TxFrontEnd, TxPad, TxTspBypass,
    TxTspMode,
};

/// A digital signal processor: libusdr's `lms7002m_xxtsp_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tsp {
    /// The receive processor (`LMS_RXTSP`).
    Rx,

    /// The transmit processor (`LMS_TXTSP`).
    Tx,
}

/// How a receive front end runs: libusdr's `lms7002m_rfe_mode_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxMode {
    /// Off (`RFE_MODE_DISABLE`).
    Disable,

    /// On, from its LNA (`RFE_MODE_NORMAL`).
    Normal,

    /// On, from the transmit side's RF loopback (`RFE_MODE_LOOPBACKRF`).
    LoopbackRf,
}

/// How a transmit front end runs: libusdr's `lms7002m_trf_mode_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxMode {
    /// Off (`TRF_MODE_DISABLE`).
    Disable,

    /// On (`TRF_MODE_NORMAL`).
    Normal,

    /// On, its pad looped back (`TRF_MODE_LOOPBACK`).
    Loopback,
}

/// A transmit output: libusdr's `lms7002m_trf_path_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxPath {
    /// No band output (`TRF_MUTE`).
    Mute,

    /// Band 1 (`TRF_B1`).
    Band1,

    /// Band 2 (`TRF_B2`).
    Band2,
}

impl TxPath {
    /// The band an RF loopback from the receive LNA `path` uses
    /// (`lms7002m_trf_from_rfe_path`): band 2 for the low-band LNA, band 1 for the wide and
    /// high ones, none otherwise.
    #[must_use]
    pub const fn looped_from(path: RxPath) -> Self {
        match path {
            RxPath::LnaL => Self::Band2,
            RxPath::LnaW | RxPath::LnaH => Self::Band1,
            RxPath::None => Self::Mute,
        }
    }
}

/// An operation on banked registers found no channel selected (`MAC` = none); libusdr
/// returns `-EINVAL` and writes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoChannel;

/// One channel's receive front end as libusdr caches it (`struct lms7002m_rfe_cfg`; its
/// gains are not cached here, since no ported operation reads them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RxChannel {
    /// The LNA input.
    path: RxPath,

    /// In RF loopback.
    loopback: bool,

    /// Enabled.
    enabled: bool,
}

/// One channel's transmit front end as libusdr caches it (`struct lms7002m_trf_cfg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TxChannel {
    /// Pad loopback on.
    loopback: bool,

    /// Enabled.
    enabled: bool,

    /// The pad's loss code (`gain`, 5 bits), written to both pad stages.
    pad_loss: u5,

    /// The loopback loss (`lbloss`, 3 bits), written to the 2-bit field when in loopback.
    loopback_loss: u3,
}

/// libusdr's idle front end: everything off and zero, as `memset` leaves it.
const RX_OFF: RxChannel = RxChannel {
    path: RxPath::None,
    loopback: false,
    enabled: false,
};

/// As [`RX_OFF`], for transmit.
const TX_OFF: TxChannel = TxChannel {
    loopback: false,
    enabled: false,
    pad_loss: u5::MIN,
    loopback_loss: u3::MIN,
};

/// `LB_LOSS_24`, the loopback loss `lms7002m_trf_path` writes when not in loopback. Its
/// value, 24, overflows the 2-bit `L_LOOPB_TXPAD_TRF` field; the generated macro masks it,
/// so the chip gets 0.
const LB_LOSS_24: u8 = 24;

/// The channels' banks, in libusdr's loop order: channel A then B (`MAC` = index + 1).
const CHANNELS: [Mac; 2] = [Mac::A, Mac::B];

/// An LMS7002M's cached state: the part of libusdr's `lms7002m_state_t` the ported
/// operations read. Board-independent; the board says where the chip answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lms7002m {
    /// The SPI target (`lsaddr`).
    target: SpiAddr,

    /// The last `LML_0x0020` value with the selected bank (`reg_mac`).
    mac: LmlControl,

    /// Each channel's `SXX_0x0124` (`reg_en_dir`).
    en_dir: [EnDir; 2],

    /// Each channel's receive front end (`rfe`).
    rx: [RxChannel; 2],

    /// Each channel's transmit front end (`trf`).
    tx: [TxChannel; 2],

    /// Each channel's receive front-end gain codes (`rfe[i].lna`, `.tia`, `.lbg`).
    rx_gains: [gain::RxGainCodes; 2],

    /// Each channel's receive processor words (`reg_rxtsp_dscpcfg`, `reg_rxtsp_hbdo_iq`).
    rx_dsp: [dsp::TspWords; 2],

    /// Each channel's transmit processor words (`reg_txtsp_dscpcfg`, `reg_txtsp_hbdo_iq`).
    tx_dsp: [dsp::TspWords; 2],

    /// The TX amplifier's filter-derived gain code (`reg_tbb_gc`).
    tbb_gain: [u8; 2],

    /// The TX amplifier's gain correction (`reg_tbb_gc_corr`).
    tbb_gain_correction: [i8; 2],
}

impl Lms7002m {
    /// The state before `lms7002m_create`: libusdr zeroes it (`xsdr_ctor`'s `memset`), so no
    /// bank is selected and every front end is off.
    ///
    /// libusdr's zeroed state also addresses SPI target 0 until `lms7002m_create` sets it;
    /// here the board names the target from the start.
    #[must_use]
    pub const fn new(target: SpiAddr) -> Self {
        Self {
            target,
            mac: LmlControl::new(),
            en_dir: [EnDir::new(); 2],
            rx: [RX_OFF; 2],
            tx: [TX_OFF; 2],
            rx_gains: [gain::RxGainCodes::ZERO; 2],
            rx_dsp: [dsp::TspWords::ZERO; 2],
            tx_dsp: [dsp::TspWords::ZERO; 2],
            tbb_gain: [0; 2],
            tbb_gain_correction: [0; 2],
        }
    }

    /// The state `lms7002m_create` leaves: every LML bit set with both banks selected, the
    /// front ends, `EN_DIR` and processor caches cleared, the TIA and LNA gain codes at 1
    /// and 12 and the TX amplifier's at 12.
    #[must_use]
    pub(crate) const fn created(target: SpiAddr) -> Self {
        Self {
            mac: LmlControl::running(Mac::Both),
            rx_gains: [gain::RxGainCodes::CREATED; 2],
            tbb_gain: [baseband::TBB_GAIN_CREATED; 2],
            ..Self::new(target)
        }
    }

    /// The banks later writes reach.
    #[must_use]
    pub const fn mac(&self) -> Mac {
        self.mac.mac()
    }

    /// Selects the banks later writes reach (`lms7002m_mac_set`); nothing is written when
    /// they are selected already.
    pub fn mac_set(&mut self, mac: Mac) -> Batch {
        let mut batch = Batch::new(self.target);
        if self.mac.mac() != mac {
            self.mac.set_mac(mac);
            batch.push(self.mac.entry());
        }
        batch
    }

    /// Enables or disables a processor with libusdr's fixed configuration
    /// (`lms7002m_xxtsp_enable`): its correction stages bypassed, the test-signal generator
    /// configured but unused, input from `LimeLight`. The write reaches the selected banks.
    ///
    /// libusdr also caches the bypass word per selected channel, which its later
    /// processor writes (mixer, DC corrector) modify; so does this.
    pub fn tsp_enable(&mut self, tsp: Tsp, enable: bool) -> Batch {
        let mut batch = Batch::new(self.target);
        match tsp {
            Tsp::Rx => {
                batch.push(
                    RxTspBypass::new()
                        .with_mixer_bypassed(true)
                        .with_agc_bypassed(true)
                        .with_gfir3_bypassed(true)
                        .with_gfir2_bypassed(true)
                        .with_gfir1_bypassed(true)
                        .with_dc_bypassed(true)
                        .with_gain_bypassed(true)
                        .with_phase_bypassed(true)
                        .entry(),
                );
                batch.push(
                    RxTspMode::new()
                        .with_tsg_frequency(TSG_DIV8)
                        .with_tsg_mode(TsgMode::Dc)
                        .with_enabled(enable)
                        .entry(),
                );
            }
            Tsp::Tx => {
                batch.push(
                    TxTspBypass::new()
                        .with_isinc_bypassed(true)
                        .with_gfir3_bypassed(true)
                        .with_gfir2_bypassed(true)
                        .with_gfir1_bypassed(true)
                        .with_gain_bypassed(true)
                        .with_phase_bypassed(true)
                        .entry(),
                );
                batch.push(
                    TxTspMode::new()
                        .with_tsg_frequency(TSG_DIV8)
                        .with_tsg_mode(TsgMode::Dc)
                        .with_enabled(enable)
                        .entry(),
                );
            }
        }
        let (_, bypass) = batch.writes()[0];
        let selected = self.banks();
        for (words, on) in self.dsp_mut(tsp).iter_mut().zip(selected) {
            if on {
                words.config = bypass;
            }
        }
        batch
    }

    /// Sets the selected channels' receive front ends (`lms7002m_rfe_path`), then writes
    /// both channels' front ends, bank by bank, and restores the bank selection.
    ///
    /// # Errors
    ///
    /// [`NoChannel`] when no bank is selected; nothing changes.
    pub fn rfe_path(&mut self, path: RxPath, mode: RxMode) -> Result<Batch, NoChannel> {
        let selected = self.selected()?;
        for (channel, _) in self.rx.iter_mut().zip(selected).filter(|(_, on)| *on) {
            *channel = RxChannel {
                path,
                loopback: mode == RxMode::LoopbackRf,
                enabled: mode != RxMode::Disable,
            };
        }
        let [a, b] = self.rx;
        self.en_dir[0].set_rfe(a.enabled || b.enabled);
        self.en_dir[1].set_rfe(b.enabled);

        let mut batch = Batch::new(self.target);
        for (index, mac) in CHANNELS.into_iter().enumerate() {
            let rx = self.rx[index];
            let lb_input_off = !matches!(rx.path, RxPath::LnaW | RxPath::LnaH);
            let lb_low_off = rx.path != RxPath::LnaL;
            let looped = rx.enabled && rx.loopback;
            let next = if index == 0 && b.enabled {
                NextChannel::Mimo
            } else {
                NextChannel::Siso
            };
            batch.push(self.mac.with_mac(mac).entry());
            batch.push(self.en_dir[index].entry());
            batch.push(
                RxFrontEnd::new()
                    .with_cdc_i(u4::new(8))
                    .with_cdc_q(u4::new(8))
                    .with_lna_powered_down(looped)
                    .with_loopback1_powered_down(looped && lb_input_off)
                    .with_loopback2_powered_down(looped && lb_low_off)
                    .with_mixer_lo_buffer_powered_down(!rx.enabled)
                    .with_qgen_powered_down(!rx.enabled)
                    .with_rssi_powered_down(true)
                    .with_tia_powered_down(!rx.enabled)
                    .with_enabled(rx.enabled)
                    .entry(),
            );
            batch.push(
                RxInput::new()
                    .with_path(rx.path)
                    .with_dc_offset_enabled(true)
                    .with_inshsw_lb1(!rx.enabled || (rx.loopback && lb_input_off))
                    .with_inshsw_lb2(!rx.enabled || (rx.loopback && lb_low_off))
                    .with_inshsw_l(!rx.enabled || (!rx.loopback && lb_low_off))
                    .with_inshsw_w(!rx.enabled || (!rx.loopback && rx.path != RxPath::LnaW))
                    .with_next(next)
                    .entry(),
            );
            let loopback_bias = if rx.loopback { 3 } else { 0 };
            batch.push(
                RxBias::new()
                    .with_loopback(u5::new(loopback_bias))
                    .with_tia_main(u5::new(2))
                    .with_tia_out(u5::new(2))
                    .entry(),
            );
        }
        batch.push(self.mac.entry());
        Ok(batch)
    }

    /// Sets the selected channels' transmit front ends (`lms7002m_trf_path`), then writes
    /// both channels' front ends, bank by bank, restores the bank selection and selects
    /// `path`'s band on the selected banks.
    ///
    /// libusdr sets each channel's `EN_DIR_TRF` from the receive front ends' enables, as
    /// for `EN_DIR_RFE`; that is reproduced.
    ///
    /// # Errors
    ///
    /// [`NoChannel`] when no bank is selected; nothing changes.
    pub fn trf_path(&mut self, path: TxPath, mode: TxMode) -> Result<Batch, NoChannel> {
        let selected = self.selected()?;
        for (channel, _) in self.tx.iter_mut().zip(selected).filter(|(_, on)| *on) {
            channel.enabled = mode != TxMode::Disable;
            channel.loopback = mode == TxMode::Loopback;
        }
        let [rx_a, rx_b] = self.rx;
        self.en_dir[0].set_trf(rx_a.enabled || rx_b.enabled);
        self.en_dir[1].set_trf(rx_b.enabled);

        let b_enabled = self.tx[1].enabled;
        let mut batch = Batch::new(self.target);
        for (index, mac) in CHANNELS.into_iter().enumerate() {
            let tx = self.tx[index];
            let serves_b = index == 0 && b_enabled;
            let next = if serves_b {
                NextChannel::Mimo
            } else {
                NextChannel::Siso
            };
            let loopback_loss = if tx.loopback {
                tx.loopback_loss.value()
            } else {
                LB_LOSS_24
            };
            batch.push(self.mac.with_mac(mac).entry());
            batch.push(self.en_dir[index].entry());
            batch.push(
                TxFrontEnd::new()
                    .with_next(next)
                    .with_power_detector_amplifier(u2::new(2))
                    .with_power_detector_load(u2::new(1))
                    .with_power_detector_powered_down(true)
                    .with_enabled(serves_b || tx.enabled)
                    .entry(),
            );
            batch.push(
                TxPad::new()
                    .with_f(u3::new(3))
                    .with_loopback_loss(u2::from_raw(loopback_loss))
                    .with_linear_loss(tx.pad_loss)
                    .with_main_loss(tx.pad_loss)
                    .with_loopback(tx.loopback)
                    .entry(),
            );
        }
        batch.push(self.mac.entry());
        batch.push(
            TxBand::new()
                .with_band1(path == TxPath::Band1)
                .with_band2(path == TxPath::Band2)
                .with_lo_bias_n(u5::new(16))
                .with_lo_bias_p(u5::new(18))
                .entry(),
        );
        Ok(batch)
    }

    /// Powers the converters up or down per direction and channel, A then B
    /// (`lms7002m_afe_enable`, which libusdr calls with its `rx_run` and `tx_run` flags);
    /// the AFE's global enable follows any of them.
    pub fn afe_enable(&self, rx: [bool; 2], tx: [bool; 2]) -> Batch {
        let ([rx_a, rx_b], [tx_a, tx_b]) = (rx, tx);
        let mut batch = Batch::new(self.target);
        batch.push(
            afe_idle()
                .with_rx_a_powered_down(!rx_a)
                .with_rx_b_powered_down(!rx_b)
                .with_tx_a_powered_down(!tx_a)
                .with_tx_b_powered_down(!tx_b)
                .with_enabled(rx_a || rx_b || tx_a || tx_b)
                .entry(),
        );
        batch
    }

    /// Clears both channels' `EN_DIR` bits and powers the whole AFE down
    /// (`lms7002m_destroy`).
    pub fn destroy(&mut self) -> Batch {
        self.en_dir = [EnDir::new(); 2];
        let mut batch = Batch::new(self.target);
        for (index, mac) in CHANNELS.into_iter().enumerate() {
            batch.push(self.mac.with_mac(mac).entry());
            batch.push(self.en_dir[index].entry());
        }
        batch.push(
            afe_idle()
                .with_powered_down(true)
                .with_rx_a_powered_down(true)
                .with_rx_b_powered_down(true)
                .with_tx_a_powered_down(true)
                .with_tx_b_powered_down(true)
                .entry(),
        );
        batch
    }

    /// Which channels the selected banks cover, A then B; none when no bank is selected
    /// (libusdr's `_lms7002m_check_chan`).
    const fn banks(&self) -> [bool; 2] {
        match self.mac.mac() {
            Mac::None => [false, false],
            Mac::A => [true, false],
            Mac::B => [false, true],
            Mac::Both => [true, true],
        }
    }

    /// Which channels the selected banks cover, A then B.
    fn selected(&self) -> Result<[bool; 2], NoChannel> {
        match self.mac.mac() {
            Mac::None => Err(NoChannel),
            Mac::A => Ok([true, false]),
            Mac::B => Ok([false, true]),
            Mac::Both => Ok([true, true]),
        }
    }
}

/// The test-signal frequency libusdr configures, `DIV8` (1).
const TSG_DIV8: u2 = u2::new(1);

/// The AFE settings libusdr's later AFE writes share: DAC current code 4, two ADCs, both
/// muxes off; every power-down clear, the global enable off.
const fn afe_idle() -> Afe {
    Afe::new()
        .with_dac_current(u3::new(4))
        .with_adc_mode(AdcMode::TwoAdcs)
        .with_mux1(AfeMux1::Off)
        .with_mux2(AfeMux2::Off)
}

#[cfg(test)]
mod tests {
    use hal::Sequence;

    use super::*;
    use crate::reg::Reg;

    /// A chip as `lms7002m_create` leaves it.
    fn created() -> Lms7002m {
        Lms7002m::created(SpiAddr(0))
    }

    /// The value `batch` writes to `reg` at its `nth` write of that register.
    fn written(batch: &Batch, reg: Reg, nth: usize) -> u16 {
        batch
            .writes()
            .iter()
            .filter(|(r, _)| *r == reg)
            .nth(nth)
            .map(|&(_, value)| value)
            .expect("the batch writes the register")
    }

    #[test]
    fn selecting_the_selected_banks_writes_nothing() {
        let mut chip = created();
        assert_eq!(chip.mac_set(Mac::Both).writes(), []);

        let batch = chip.mac_set(Mac::A);
        assert_eq!(batch.writes(), [(Reg::LmlControl, 0xfffd)]);
        assert_eq!(chip.mac(), Mac::A, "the cache follows the write");
    }

    #[test]
    fn a_front_end_operation_without_a_selected_bank_changes_nothing() {
        let mut chip = Lms7002m::new(SpiAddr(0));
        assert_eq!(
            chip.rfe_path(RxPath::LnaH, RxMode::Normal).err(),
            Some(NoChannel)
        );
        assert_eq!(
            chip.trf_path(TxPath::Band1, TxMode::Normal).err(),
            Some(NoChannel)
        );
        assert_eq!(
            chip,
            Lms7002m::new(SpiAddr(0)),
            "libusdr returns -EINVAL first"
        );
    }

    #[test]
    fn enabling_channel_a_writes_both_banks_and_restores_the_selection() {
        let mut chip = created();
        let _ = chip.mac_set(Mac::A);
        let batch = chip.rfe_path(RxPath::LnaH, RxMode::Normal).unwrap();

        let lml: [u16; 3] = core::array::from_fn(|n| written(&batch, Reg::LmlControl, n));
        assert_eq!(
            lml,
            [0xfffd, 0xfffe, 0xfffd],
            "bank A, bank B, then A again"
        );
        let a = RxFrontEnd::from_raw(written(&batch, Reg::RxFrontEnd, 0));
        let b = RxFrontEnd::from_raw(written(&batch, Reg::RxFrontEnd, 1));
        assert!(a.enabled() && !a.tia_powered_down(), "channel A runs");
        assert!(!b.enabled() && b.tia_powered_down(), "channel B stays off");
        assert_eq!(
            RxInput::from_raw(written(&batch, Reg::RxInput, 0)).path(),
            RxPath::LnaH
        );
        assert!(
            EnDir::from_raw(written(&batch, Reg::EnDir, 0)).rfe(),
            "EN_DIR_RFE on bank A follows either channel"
        );
        assert!(!EnDir::from_raw(written(&batch, Reg::EnDir, 1)).rfe());
    }

    #[test]
    fn channel_a_serves_b_while_bs_front_end_runs() {
        let mut chip = created();
        let batch = chip.trf_path(TxPath::Band2, TxMode::Normal).unwrap();
        let a = TxFrontEnd::from_raw(written(&batch, Reg::TxFrontEnd, 0));
        let b = TxFrontEnd::from_raw(written(&batch, Reg::TxFrontEnd, 1));
        assert_eq!((a.next(), b.next()), (NextChannel::Mimo, NextChannel::Siso));
        let band = TxBand::from_raw(written(&batch, Reg::TxBand, 0));
        assert!(
            !band.band1() && band.band2(),
            "the band follows the path argument"
        );
    }

    #[test]
    fn en_dir_trf_follows_the_receive_front_ends_as_in_libusdr() {
        let mut chip = created();
        let transmit_only = chip.trf_path(TxPath::Band1, TxMode::Normal).unwrap();
        assert!(
            !EnDir::from_raw(written(&transmit_only, Reg::EnDir, 0)).trf(),
            "a running transmit front end alone leaves EN_DIR_TRF clear"
        );

        let _ = chip.rfe_path(RxPath::LnaW, RxMode::Normal).unwrap();
        let with_receive = chip.trf_path(TxPath::Band1, TxMode::Normal).unwrap();
        assert!(EnDir::from_raw(written(&with_receive, Reg::EnDir, 0)).trf());
    }

    #[test]
    fn the_out_of_range_loopback_loss_reaches_the_chip_masked() {
        let mut chip = created();
        let batch = chip.trf_path(TxPath::Mute, TxMode::Disable).unwrap();
        let pad = TxPad::from_raw(written(&batch, Reg::TxPad, 0));
        assert_eq!(
            pad.loopback_loss().value(),
            0,
            "LB_LOSS_24 (0b11000) in two bits"
        );
    }

    #[test]
    fn destroy_clears_en_dir_and_powers_the_afe_down() {
        let mut chip = created();
        let _ = chip.rfe_path(RxPath::LnaL, RxMode::Normal).unwrap();
        let batch = chip.destroy();
        assert_eq!(written(&batch, Reg::EnDir, 0), 0);
        assert_eq!(written(&batch, Reg::EnDir, 1), 0);
        let afe = Afe::from_raw(written(&batch, Reg::Afe, 0));
        assert!(afe.powered_down() && !afe.enabled());
    }

    #[test]
    fn the_afe_global_enable_follows_any_converter() {
        let chip = created();
        let off = chip.afe_enable([false; 2], [false; 2]);
        let off = Afe::from_raw(written(&off, Reg::Afe, 0));
        assert!(!off.enabled() && off.rx_a_powered_down() && off.tx_b_powered_down());
        let one = chip.afe_enable([false; 2], [false, true]);
        let one = Afe::from_raw(written(&one, Reg::Afe, 0));
        assert!(one.enabled() && !one.tx_b_powered_down() && one.tx_a_powered_down());
    }

    #[test]
    fn a_batch_issues_each_write_once() {
        let mut batch = created().tsp_enable(Tsp::Rx, false);
        batch.complete::<()>(Ok(0)).unwrap();
        batch.complete::<()>(Ok(0)).unwrap();
        assert_eq!(batch.next(), None, "two writes, then done");
    }
}
