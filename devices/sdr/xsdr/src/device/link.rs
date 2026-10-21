//! The `LimeLight` link's clock-phase overrides (`m2_lm7_1.c`'s `/dm/sdr/0/tx/phase_ovr`,
//! `.../tx/phase_ovr_iq`, `.../tx/phase_ovr_rc` and `.../rx/phase_ovr`, `xsdr_ctrl.c`'s
//! `xsdr_txphase_ovr`) and the TX quality check on its own (`/dm/sdr/0/checkrxtx`).

use super::{Device, run};
use crate::error::Error;
use crate::portable::mmcm::{MmcmCall, MmcmLink, MmcmPlan, OverrideTxPhase, PHASE_OVERRIDE_MAX};
use crate::portable::txcal::tx_serdes_flag;

impl Device {
    /// Sets the TX fabric clock's phase in place of the calibration's (libusdr's
    /// `/dm/sdr/0/tx/phase_ovr`), in eighths of a VCO cycle counted from 1; 0 lets the
    /// calibration choose again. It takes effect at the next link calibration (a rate, or
    /// a stream's create): with TX running, wherever a TX MMCM configuration names no TX
    /// phase of its own; without a TX MMCM, as the TX delay (`phase - 1`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidPhase`] above 512, whose delay no MMCM output holds (libusdr keeps
    /// it and later writes a configuration's earlier outputs before refusing), before any
    /// state change.
    pub fn set_tx_phase_override(&mut self, phase: u16) -> Result<(), Error> {
        self.rate.overrides.tx = checked_phase(phase)?;
        Ok(())
    }

    /// Sets the separate `CLKDIV` output's phase (libusdr's `/dm/sdr/0/tx/phase_ovr_iq`),
    /// as [`Self::set_tx_phase_override`]; only gateware with that output (HWID PHY bit
    /// 0x80) has it.
    ///
    /// # Errors
    ///
    /// As [`Self::set_tx_phase_override`].
    pub fn set_tx_iq_phase_override(&mut self, phase: u16) -> Result<(), Error> {
        self.rate.overrides.tx_iq = checked_phase(phase)?;
        Ok(())
    }

    /// Sets the RX I/O clock's phase in place of the calibration's (libusdr's
    /// `/dm/sdr/0/rx/phase_ovr`), as [`Self::set_tx_phase_override`].
    ///
    /// Set, it also skips the link calibration's RX and TX sweeps, as in libusdr, whose
    /// calibration then judges an RX badness it never measured: on gateware with a TX
    /// MMCM, every rate and stream create fails with [`Error::LinkCalibration`] while it
    /// is set.
    ///
    /// # Errors
    ///
    /// As [`Self::set_tx_phase_override`].
    pub fn set_rx_phase_override(&mut self, phase: u16) -> Result<(), Error> {
        self.rate.overrides.rx = checked_phase(phase)?;
        Ok(())
    }

    /// Overrides both TX phases now (libusdr's `/dm/sdr/0/tx/phase_ovr_rc`,
    /// `xsdr_txphase_ovr`): both set as [`Self::set_tx_phase_override`] and
    /// [`Self::set_tx_iq_phase_override`] would, then the TX MMCM configured with TX as the
    /// master at the RX phase the last calibration kept, and the TX serializer reset.
    /// libusdr does this whatever the gateware's MMCMs.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidPhase`] as [`Self::set_tx_phase_override`]; then, the overrides
    /// kept, before any bus access: [`Error::NoSampleRate`] before any rate (libusdr
    /// divides by the zero converter divider),
    /// [`Error::TxClockOutOfRange`] for a link the TX MMCM cannot take; then
    /// [`Error::TxClockUnlocked`] (libusdr's `-EIO`, the serializer not reset) or
    /// [`Error::Bus`].
    pub fn apply_tx_phase_override(&mut self, phase: u16) -> Result<(), Error> {
        let phase = checked_phase(phase)?;
        self.rate.overrides.tx = phase;
        self.rate.overrides.tx_iq = phase;
        if self.control.clocks().cgen == 0 {
            return Err(Error::NoSampleRate);
        }
        let gateware = self.gateware();
        let lml = self.control.lml();
        let link = MmcmLink {
            clocks: self.control.clocks(),
            lml,
            double_pump: self.rate.double_pump,
            separate_clkdiv: gateware.separate_clkdiv,
            overrides: self.rate.overrides,
        };
        let call = MmcmCall {
            rx_phase: self.rate.rx_phase,
            clock_reduce: self.rate.clock_reduce,
            ..MmcmCall::default()
        };
        let plan = MmcmPlan::new(&link, call).map_err(|_| Error::TxClockOutOfRange)?;
        let flag = tx_serdes_flag(self.rate.double_pump, lml.tx_siso_ddr);
        let mut override_phase = OverrideTxPhase::new(&plan, flag);
        run(&mut *self.bus, &mut override_phase)?;
        match override_phase.outcome() {
            Some(Err(_)) => Err(Error::TxClockUnlocked),
            Some(Ok(())) | None => Ok(()),
        }
    }

    /// Runs the TX quality check on its own (libusdr's `/dm/sdr/0/checkrxtx`,
    /// `xsdr_check_rxtx_quality(d, fix)`): the tone played through the baseband loopback
    /// and scored; with `fix`, the TX processors' clock adjusted until the score holds, up
    /// to eight attempts; then the generator stopped and the running channels' baseband
    /// filters set for the rates.
    ///
    /// Divergence, as for a rate's check: libusdr ends dividing the rates by factors only
    /// DUC/DDC gateware sets, 0 here, and traps (SIGFPE); this divides by the board's, 1.
    ///
    /// # Errors
    ///
    /// [`Error::CaptureBusy`] for a capture that stays busy; [`Error::Bus`].
    pub fn check_tx_quality(&mut self, fix: bool) -> Result<(), Error> {
        self.check_quality(fix)
    }
}

/// `phase` if an MMCM output's delay holds it.
fn checked_phase(phase: u16) -> Result<u16, Error> {
    if phase > PHASE_OVERRIDE_MAX {
        return Err(Error::InvalidPhase(phase));
    }
    Ok(phase)
}
