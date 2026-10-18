//! The corrections on [`NativeUsb`]: the RX front end's DC and IP2 trims, the RX chain's
//! automatic DC correction, the TX DC offset, both chains' IQ corrections and the TX
//! constant generator, as the blocking [`Board`](crate::board) sets them
//! (`board/correction.rs`, `board/generator.rs`): the same portable scripts, the LMS6002D's
//! for the trims and the gateware's DSP-port words for the rest (`portable::fpga`).

use std::time::Instant;

use num_complex::Complex;

use super::NativeUsb;
use super::radio::{RadioKind, RadioOp};
use crate::board::{RxIqCorrection, RxTrim, TxDcOffset, TxIqCorrection};
use crate::error::Error;
use crate::fpga::{IqImbalance, Phy};
use crate::portable::script::Script;

impl NativeUsb {
    /// Starts writing the RX front end's DC-offset trims, as
    /// [`Device::set_rx_dc_trim`](crate::Device::set_rx_dc_trim) does.
    /// [`poll`](Self::poll) reports [`NativeEvent::CorrectionSet`](super::NativeEvent).
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotPowered`]
    /// before a power-up; [`Error::Bus`] if the handle is closed.
    pub fn start_rx_dc_trim(&mut self, i: RxTrim, q: RxTrim, now: Instant) -> Result<(), Error> {
        let radio = self.refuse_radio_operation()?;
        self.start_dsp(&radio.lms.set_rx_dc_trim(i.bits(), q.bits()), now)
    }

    /// Starts writing the RX front end's IP2 trims, as
    /// [`Device::set_rx_ip2_trim`](crate::Device::set_rx_ip2_trim) does.
    /// [`poll`](Self::poll) reports [`NativeEvent::CorrectionSet`](super::NativeEvent).
    ///
    /// # Errors
    ///
    /// As [`start_rx_dc_trim`](Self::start_rx_dc_trim).
    pub fn start_rx_ip2_trim(&mut self, i: RxTrim, q: RxTrim, now: Instant) -> Result<(), Error> {
        let radio = self.refuse_radio_operation()?;
        self.start_dsp(&radio.lms.set_rx_ip2_trim(i.bits(), q.bits()), now)
    }

    /// Starts switching the RX chain's automatic DC correction, as
    /// [`Device::set_rx_auto_dc_correction`](crate::Device::set_rx_auto_dc_correction)
    /// does. [`poll`](Self::poll) reports
    /// [`NativeEvent::CorrectionSet`](super::NativeEvent).
    ///
    /// # Errors
    ///
    /// As [`start_rx_dc_trim`](Self::start_rx_dc_trim).
    pub fn start_rx_auto_dc_correction(
        &mut self,
        enabled: bool,
        now: Instant,
    ) -> Result<(), Error> {
        self.start_dsp(&Phy::Rx.set_dc_correction(enabled), now)
    }

    /// Starts setting the RX chain's IQ correction, as
    /// [`Device::set_rx_iq_correction`](crate::Device::set_rx_iq_correction) does: the
    /// handle keeps it for its stream group's create even if a write fails.
    /// [`poll`](Self::poll) reports [`NativeEvent::CorrectionSet`](super::NativeEvent).
    ///
    /// # Errors
    ///
    /// As [`start_rx_dc_trim`](Self::start_rx_dc_trim).
    pub fn start_rx_iq_correction(
        &mut self,
        correction: RxIqCorrection,
        now: Instant,
    ) -> Result<(), Error> {
        self.start_rx_iq_imbalance(correction.imbalance(), now)
    }

    /// Starts setting the RX chain's IQ correction in the terms
    /// [`Device::calibrate`](crate::Device::calibrate) reports, as
    /// [`Device::set_rx_iq_imbalance`](crate::Device::set_rx_iq_imbalance) does; kept as
    /// [`start_rx_iq_correction`](Self::start_rx_iq_correction) keeps it.
    ///
    /// # Errors
    ///
    /// As [`start_rx_dc_trim`](Self::start_rx_dc_trim).
    pub fn start_rx_iq_imbalance(
        &mut self,
        imbalance: IqImbalance,
        now: Instant,
    ) -> Result<(), Error> {
        self.refuse_radio_operation()?;
        self.session.board.rx_iq_imbalance = imbalance;
        self.start_dsp(&Phy::Rx.set_iq_correction(imbalance.rx_terms()), now)
    }

    /// Starts writing the TX chain's DC offsets, as
    /// [`Device::set_tx_dc_offset`](crate::Device::set_tx_dc_offset) does.
    /// [`poll`](Self::poll) reports [`NativeEvent::CorrectionSet`](super::NativeEvent).
    ///
    /// # Errors
    ///
    /// As [`start_rx_dc_trim`](Self::start_rx_dc_trim).
    pub fn start_tx_dc_offset(
        &mut self,
        i: TxDcOffset,
        q: TxDcOffset,
        now: Instant,
    ) -> Result<(), Error> {
        self.start_dsp(&Phy::set_tx_dc_offset(i.bits(), q.bits()), now)
    }

    /// Starts setting the TX chain's IQ correction, as
    /// [`Device::set_tx_iq_correction`](crate::Device::set_tx_iq_correction) does: the
    /// handle keeps it for its stream group's create even if a write fails.
    /// [`poll`](Self::poll) reports [`NativeEvent::CorrectionSet`](super::NativeEvent).
    ///
    /// # Errors
    ///
    /// As [`start_rx_dc_trim`](Self::start_rx_dc_trim).
    pub fn start_tx_iq_correction(
        &mut self,
        correction: TxIqCorrection,
        now: Instant,
    ) -> Result<(), Error> {
        self.start_tx_iq_imbalance(correction.imbalance(), now)
    }

    /// Starts setting the TX chain's IQ correction in the calibration's terms, as
    /// [`Device::set_tx_iq_imbalance`](crate::Device::set_tx_iq_imbalance) does; kept as
    /// [`start_tx_iq_correction`](Self::start_tx_iq_correction) keeps it.
    ///
    /// # Errors
    ///
    /// As [`start_rx_dc_trim`](Self::start_rx_dc_trim).
    pub fn start_tx_iq_imbalance(
        &mut self,
        imbalance: IqImbalance,
        now: Instant,
    ) -> Result<(), Error> {
        self.refuse_radio_operation()?;
        self.session.board.tx_iq_imbalance = imbalance;
        self.start_dsp(&Phy::Tx.set_iq_correction(imbalance.tx_terms()), now)
    }

    /// Starts replacing the TX chain's samples with `constant` on both channels, or with
    /// `None` passing them again, as
    /// [`Device::set_tx_constant`](crate::Device::set_tx_constant) does.
    /// [`poll`](Self::poll) reports [`NativeEvent::CorrectionSet`](super::NativeEvent).
    ///
    /// # Errors
    ///
    /// As [`start_rx_dc_trim`](Self::start_rx_dc_trim).
    pub fn start_tx_constant(
        &mut self,
        constant: Option<Complex<i16>>,
        now: Instant,
    ) -> Result<(), Error> {
        let constant = constant.map(|constant| [constant.re, constant.im]);
        self.start_dsp(&Phy::set_tx_constant(constant), now)
    }

    /// Starts a correction's writes on a powered board; they change no RF state the
    /// handle keeps (the LMS6002D's trims are not cached).
    fn start_dsp(&mut self, script: &Script, now: Instant) -> Result<(), Error> {
        self.refuse_radio_operation()?;
        self.start_radio(RadioOp::Script(*script), RadioKind::Correction, now)
    }
}
