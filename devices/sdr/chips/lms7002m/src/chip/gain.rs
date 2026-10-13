//! The RF front ends' gains: the receive LNA, TIA and loopback stages
//! (`lms7002m_rfe_gain`) and the transmit pad and its loopback (`lms7002m_trf_gain`).

use bnb::{u2, u3, u4, u5};

use super::{CHANNELS, LB_LOSS_24, Lms7002m, NoChannel};
use crate::batch::Batch;
use crate::register::Register;
use crate::regs::{RxGains, TxPad};

/// One channel's receive gain codes, as indices into libusdr's attenuation tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RxGainCodes {
    /// The LNA's (`rfe[i].lna`).
    lna: u4,

    /// The TIA's (`rfe[i].tia`).
    tia: u2,

    /// The loopback's (`rfe[i].lbg`).
    loopback: u4,
}

impl RxGainCodes {
    /// The zeroed state.
    pub(super) const ZERO: Self = Self {
        lna: u4::MIN,
        tia: u2::MIN,
        loopback: u4::MIN,
    };

    /// As `lms7002m_create` leaves them: TIA 1, LNA 12.
    pub(super) const CREATED: Self = Self {
        lna: u4::new(12),
        tia: u2::new(1),
        ..Self::ZERO
    };
}

/// A receive gain stage: libusdr's `lms7002m_rfe_gain_t` without `RFE_GAIN_NONE` (see
/// [`Lms7002m::rfe_gains`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxGainStage {
    /// The LNA (`RFE_GAIN_LNA`): 0 to 30 dB of attenuation.
    Lna,

    /// The TIA (`RFE_GAIN_TIA`): 12, 3 or 0 dB of attenuation.
    Tia,

    /// The RF loopback (`RFE_GAIN_RFB`): 0 to 40 dB of attenuation.
    Loopback,
}

/// A transmit gain stage: libusdr's `lms7002m_trf_gain_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxGainStage {
    /// The pad (`TRF_GAIN_PAD`): 0 to 52 dB of loss.
    Pad,

    /// The pad's loopback (`TRF_GAIN_LB`): 0, 14, 21 or 24 dB of loss.
    Loopback,
}

/// "Not an allowed tap" in libusdr's attenuation tables (`IGNORE_IDX`).
const IGNORED: Option<i32> = None;

/// The LNA's attenuation per code, in tenths of a dB (`lna_attens`).
const LNA: [Option<i32>; 16] = [
    IGNORED,
    Some(300),
    Some(270),
    Some(240),
    Some(210),
    Some(180),
    Some(150),
    Some(120),
    Some(90),
    Some(60),
    Some(50),
    Some(40),
    Some(30),
    Some(20),
    Some(10),
    Some(0),
];

/// The TIA's attenuation per code (`tia_attens`).
const TIA: [Option<i32>; 4] = [IGNORED, Some(120), Some(30), Some(0)];

/// The loopback's attenuation per code (`lb_attens`).
const LOOPBACK: [Option<i32>; 16] = [
    Some(400),
    Some(240),
    Some(170),
    Some(140),
    Some(110),
    Some(90),
    Some(75),
    Some(62),
    Some(50),
    Some(40),
    Some(30),
    Some(24),
    Some(16),
    Some(10),
    Some(5),
    Some(0),
];

/// The code for an attenuation of `attenuation` tenths of a dB (`_find_idx`): the first
/// allowed code, short of the last, whose attenuation is below it; else the last.
fn code(attenuation: i32, table: &[Option<i32>]) -> u8 {
    let last = table.len() - 1;
    let index = table[..last]
        .iter()
        .position(|&step| step.is_some_and(|step| step < attenuation))
        .unwrap_or(last);
    u8::try_from(index).expect("invariant: the tables are short")
}

/// The attenuation code `code` stands for, in tenths of a dB.
fn attenuation(code: u8, table: &[Option<i32>]) -> i32 {
    table[usize::from(code)].unwrap_or_default()
}

impl RxGainStage {
    /// The stage's attenuation table.
    const fn table(self) -> &'static [Option<i32>] {
        match self {
            Self::Lna => &LNA,
            Self::Tia => &TIA,
            Self::Loopback => &LOOPBACK,
        }
    }

    /// The code libusdr picks for `gain_x10` tenths of a dB.
    fn code(self, gain_x10: i32) -> u8 {
        code(gain_x10.wrapping_neg(), self.table())
    }

    /// The gain, in tenths of a dB, the code for `gain_x10` gives: what
    /// `lms7002m_rfe_gain` reports in `goutx10`.
    #[must_use]
    pub fn reached(self, gain_x10: i32) -> i32 {
        -attenuation(self.code(gain_x10), self.table())
    }
}

impl Lms7002m {
    /// Sets `stage` on the selected channels to the first code whose attenuation is
    /// strictly below the one `gain_x10` tenths of a dB asks for (a gain, so 0 or below:
    /// −30 dB is 30 dB of attenuation), else the last code, then writes both channels'
    /// gains (`lms7002m_rfe_gain`). Returns the gain the code gives, in tenths of a dB.
    ///
    /// libusdr's search takes the first code *below* the attenuation asked for, so an
    /// exact table entry selects the next code; that is reproduced.
    pub fn rfe_gain(&mut self, stage: RxGainStage, gain_x10: i32) -> (Batch, i32) {
        let code = stage.code(gain_x10);
        let selected = self.banks();
        for (gains, _) in self.rx_gains.iter_mut().zip(selected).filter(|(_, on)| *on) {
            match stage {
                RxGainStage::Lna => gains.lna = u4::from_raw(code),
                RxGainStage::Tia => gains.tia = u2::from_raw(code),
                RxGainStage::Loopback => gains.loopback = u4::from_raw(code),
            }
        }
        (self.rfe_gains(), stage.reached(gain_x10))
    }

    /// Writes both channels' receive gains from the cache, bank by bank, and restores the
    /// bank selection (`_lms7002m_rfe_update_gains`, or `lms7002m_rfe_gain` with
    /// `RFE_GAIN_NONE`). A channel in RF loopback gets LNA code 1 and its loopback code;
    /// otherwise its LNA code and loopback 0.
    pub fn rfe_gains(&self) -> Batch {
        let mut batch = Batch::new(self.target);
        for (index, bank) in CHANNELS.into_iter().enumerate() {
            let (rx, gains) = (self.rx[index], self.rx_gains[index]);
            let (lna, loopback) = if rx.loopback {
                (u4::new(1), gains.loopback)
            } else {
                (gains.lna, u4::MIN)
            };
            batch.push(self.mac.with_mac(bank).entry());
            batch.push(
                RxGains::new()
                    .with_lna(lna)
                    .with_loopback(loopback)
                    .with_tia(gains.tia)
                    .entry(),
            );
        }
        batch.push(self.mac.entry());
        batch
    }

    /// Sets `stage`'s loss on the selected channels from `gain_x10` tenths of a dB (0 or
    /// below), then writes both channels' pads (`lms7002m_trf_gain`). Returns the pad's
    /// gain in tenths of a dB; libusdr reports none for the loopback.
    ///
    /// The pad codes 0–10 dB in 1 dB steps, then 2 dB steps to 52 dB. A positive gain
    /// gives a negative loss, which libusdr stores masked into its 5-bit (pad) or 3-bit
    /// (loopback) cache field; so does this. The loopback losses 14, 21 and 24 reach the
    /// 2-bit register field masked too.
    ///
    /// # Errors
    ///
    /// [`NoChannel`] when no bank is selected; nothing changes.
    pub fn trf_gain(
        &mut self,
        stage: TxGainStage,
        gain_x10: i32,
    ) -> Result<(Batch, Option<i32>), NoChannel> {
        let selected = self.selected()?;
        let mut loss = gain_x10.wrapping_neg() / 10;
        let actual = match stage {
            TxGainStage::Pad => Some(match loss {
                53.. => {
                    loss = 31;
                    -520
                }
                11.. => {
                    loss = (loss + 10) / 2;
                    -10 * (loss * 2 - 10)
                }
                _ => -10 * loss,
            }),
            TxGainStage::Loopback => {
                loss = match loss {
                    25.. => i32::from(LB_LOSS_24),
                    21.. => 21,
                    14.. => 14,
                    _ => 0,
                };
                None
            }
        };
        let [low, ..] = loss.to_le_bytes();
        for (tx, _) in self.tx.iter_mut().zip(selected).filter(|(_, on)| *on) {
            match stage {
                TxGainStage::Pad => tx.pad_loss = u5::from_raw(low),
                TxGainStage::Loopback => tx.loopback_loss = u3::from_raw(low),
            }
        }
        let mut batch = Batch::new(self.target);
        for (index, bank) in CHANNELS.into_iter().enumerate() {
            let tx = self.tx[index];
            let loopback_loss = if tx.loopback {
                tx.loopback_loss.value()
            } else {
                LB_LOSS_24
            };
            batch.push(self.mac.with_mac(bank).entry());
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
        Ok((batch, actual))
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::SpiAddr;

    use super::*;
    use crate::reg::Reg;
    use crate::regs::Mac;

    #[test]
    fn an_exact_table_entry_selects_the_next_code_as_in_libusdr() {
        // 0 dB of LNA gain is 30 dB of attenuation: code 1 is not below 300, code 2 is.
        let mut chip = Lms7002m::created(SpiAddr(0));
        let (_, actual) = chip.rfe_gain(RxGainStage::Lna, -300);
        assert_eq!(actual, -270);
        let (_, full) = chip.rfe_gain(RxGainStage::Lna, 0);
        assert_eq!(full, 0, "no code is below 0 dB: the last, 0 dB");
    }

    #[test]
    fn rfe_gains_write_both_banks_from_the_cache() {
        let mut chip = Lms7002m::created(SpiAddr(0));
        let _ = chip.mac_set(Mac::B);
        let (batch, _) = chip.rfe_gain(RxGainStage::Tia, -30);
        let gains = |nth| {
            batch
                .writes()
                .iter()
                .filter(|(reg, _)| *reg == Reg::RxGains)
                .nth(nth)
                .map(|&(_, v)| RxGains::from_raw(v))
                .expect("both banks are written")
        };
        assert_eq!(
            gains(0).tia().value(),
            1,
            "channel A keeps its created code"
        );
        assert_eq!(
            gains(1).tia().value(),
            3,
            "3 dB: the first code below 30 is 0 dB"
        );
        assert_eq!(gains(0).lna().value(), 12);
    }

    #[test]
    fn pad_losses_step_by_two_db_above_ten() {
        let mut chip = Lms7002m::created(SpiAddr(0));
        let (_, actual) = chip.trf_gain(TxGainStage::Pad, -200).unwrap();
        assert_eq!(actual, Some(-200), "20 dB is code 15, 20 dB");
        let (_, actual) = chip.trf_gain(TxGainStage::Pad, -210).unwrap();
        assert_eq!(actual, Some(-200), "21 dB rounds down to the 2 dB step");
        let (_, actual) = chip.trf_gain(TxGainStage::Pad, -600).unwrap();
        assert_eq!(actual, Some(-520), "the pad's limit");
    }
}
