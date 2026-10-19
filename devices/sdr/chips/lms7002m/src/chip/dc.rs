//! The analog DC-offset correction DACs: one DAC's value (`lms7002m_dc_corr`) and the
//! DACs' enables (`lms7002m_dc_corr_en`).

use bnb::u11;

use super::Lms7002m;
use crate::batch::Batch;
use crate::reg::Reg;
use crate::register::Register;
use crate::regs::{DcCalibration, DcClockDivider, DcDacWord, DcPowerDown};

/// A DC correction DAC: libusdr's `dc_param` (`P_TXA_I` 0 to `P_RXB_Q` 7), in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DcDac {
    /// TX channel A, I (`P_TXA_I`).
    TxAI,
    /// TX A, Q (`P_TXA_Q`).
    TxAQ,
    /// TX B, I (`P_TXB_I`).
    TxBI,
    /// TX B, Q (`P_TXB_Q`).
    TxBQ,
    /// RX A, I (`P_RXA_I`).
    RxAI,
    /// RX A, Q (`P_RXA_Q`).
    RxAQ,
    /// RX B, I (`P_RXB_I`).
    RxBI,
    /// RX B, Q (`P_RXB_Q`).
    RxBQ,
}

impl DcDac {
    /// The DAC's word: `DC_0x05C3` plus its `dc_param` code.
    #[must_use]
    pub const fn reg(self) -> Reg {
        match self {
            Self::TxAI => Reg::DcTxAI,
            Self::TxAQ => Reg::DcTxAQ,
            Self::TxBI => Reg::DcTxBI,
            Self::TxBQ => Reg::DcTxBQ,
            Self::RxAI => Reg::DcRxAI,
            Self::RxAQ => Reg::DcRxAQ,
            Self::RxBI => Reg::DcRxBI,
            Self::RxBQ => Reg::DcRxBQ,
        }
    }
}

/// `DC_0x05CB`'s value in `lms7002m_dc_corr_en`: both dividers 255.
const DC_CLOCK_DIVIDER: u8 = 255;

impl Lms7002m {
    /// Sets a DC correction DAC to `value`'s low 11 bits (`lms7002m_dc_corr`): the word
    /// written, then written again with its write strobe set. Not banked.
    pub fn dc_dac(&self, dac: DcDac, value: i16) -> Batch {
        let [low, high] = value.to_le_bytes();
        let word = DcDacWord::new().with_value(u11::from_raw(u16::from_le_bytes([low, high])));
        let mut batch = Batch::new(self.target);
        batch.push((dac.reg(), word.to_raw()));
        batch.push((dac.reg(), word.with_write(true).to_raw()));
        batch
    }

    /// Powers the DC correction DACs and their comparators up per direction and channel, A
    /// then B (`lms7002m_dc_corr_en`): the power-downs and mode, the calibration control
    /// cleared, both clock dividers 255, then the running DACs' calibrations started.
    pub fn dc_corrector_enable(&self, rx: [bool; 2], tx: [bool; 2]) -> Batch {
        let ([rx_a, rx_b], [tx_a, tx_b]) = (rx, tx);
        let mut batch = Batch::new(self.target);
        batch.push(
            DcPowerDown::new()
                .with_mode(rx_a || rx_b || tx_a || tx_b)
                .with_dac_rx_b_powered_down(!rx_b)
                .with_dac_rx_a_powered_down(!rx_a)
                .with_dac_tx_b_powered_down(!tx_b)
                .with_dac_tx_a_powered_down(!tx_a)
                .with_comparator_rx_b_powered_down(!rx_b)
                .with_comparator_rx_a_powered_down(!rx_a)
                .with_comparator_tx_b_powered_down(!tx_b)
                .with_comparator_tx_a_powered_down(!tx_a)
                .entry(),
        );
        batch.push(DcCalibration::new().entry());
        batch.push(
            DcClockDivider::new()
                .with_rx(DC_CLOCK_DIVIDER)
                .with_tx(DC_CLOCK_DIVIDER)
                .entry(),
        );
        // From bit 7 down: RX B Q and I, RX A Q and I, TX B Q and I, TX A Q and I.
        let start = [rx_b, rx_b, rx_a, rx_a, tx_b, tx_b, tx_a, tx_a]
            .into_iter()
            .fold(0u8, |bits, on| bits << 1 | u8::from(on));
        batch.push(DcCalibration::new().with_start(start).entry());
        batch
    }
}

#[cfg(test)]
mod tests {
    use hal::SpiAddr;

    use super::*;

    #[test]
    fn a_dac_value_is_written_then_strobed_in_eleven_bits() {
        let chip = Lms7002m::created(SpiAddr(0));
        let batch = chip.dc_dac(DcDac::RxBQ, -1);
        assert_eq!(
            batch.writes(),
            [(Reg::DcRxBQ, 0x07ff), (Reg::DcRxBQ, 0x87ff)]
        );
    }

    #[test]
    fn the_running_dacs_start_their_calibrations() {
        let chip = Lms7002m::created(SpiAddr(0));
        let batch = chip.dc_corrector_enable([true, false], [false, true]);
        let (_, start) = batch.writes()[3];
        assert_eq!(start, 0b0011_1100, "RX A's pair and TX B's pair");
        let (_, power) = batch.writes()[0];
        assert!(DcPowerDown::from_raw(power).mode());
    }
}
