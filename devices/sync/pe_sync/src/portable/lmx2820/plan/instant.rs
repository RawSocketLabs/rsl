//! Instant calibration's plans: its set-up (`lmx2820_instant_calibration_init`) and the
//! retunes over it (`lmx2820_solver_instcal`).

use super::super::super::float::{c_f64, c_unsigned};
use super::{
    CHANNEL_DIVIDER_LOG2, FEEDBACK_MAX, OUTPUT_HZ, Outputs, PFD_MAX_HZ, PFD_MIN_HZ, Plan,
    PlanError, Request, VCO_MIN_HZ, calibration_clock_divider,
};

/// The feedback divider's smallest value (`PLL_N_MIN`), which bounds instant calibration's
/// lowest output.
const FEEDBACK_MIN: u16 = 12;

impl Plan {
    /// Plans instant calibration's set-up (`lmx2820_instant_calibration_init`): `request`'s
    /// reference path and SYSREF with both outputs at the VCO's lowest frequency (its
    /// outputs are ignored), registered for instant calibration, whose wait follows the
    /// reference.
    pub(crate) fn for_instant_calibration(request: &Request) -> Result<Self, PlanError> {
        let mut plan = Self::solve(&Request {
            out_a_hz: VCO_MIN_HZ,
            out_b_hz: VCO_MIN_HZ,
            ..*request
        })?;
        plan.instant_delay = Some(instant_delay(request.reference_hz));
        Ok(plan)
    }

    /// The plan for new outputs over this plan's reference path, its phase detector
    /// unchanged (`lmx2820_solver_instcal`): their VCO, feedback divider and channel
    /// dividers. SYSREF and instant calibration carry over, as libusdr's state does.
    ///
    /// # Errors
    ///
    /// [`PlanError::OutputOutOfRange`] for an output beyond what the phase detector's
    /// feedback range reaches (or 45 MHz to 22.6 GHz); otherwise as [`Plan::solve`].
    pub(crate) fn retune(&self, a_hz: u64, b_hz: u64) -> Result<Self, PlanError> {
        let mash = self.mash as usize;
        if self.pfd_hz < c_f64(PFD_MIN_HZ) || self.pfd_hz > c_f64(PFD_MAX_HZ[mash]) {
            return Err(PlanError::ReferencePath);
        }
        let highest = (self.pfd_hz * f64::from(FEEDBACK_MAX) * 2.0).min(c_f64(*OUTPUT_HZ.end()));
        let lowest = (self.pfd_hz * f64::from(FEEDBACK_MIN)
            / f64::from(1_u32 << *CHANNEL_DIVIDER_LOG2.end()))
        .max(c_f64(*OUTPUT_HZ.start()));
        let in_range = |hz: u64| (lowest..=highest).contains(&c_f64(hz));
        if !in_range(a_hz) || !in_range(b_hz) {
            return Err(PlanError::OutputOutOfRange);
        }
        let outputs = Outputs::prepare(a_hz, b_hz)?;
        let plan = Self { outputs, ..*self }.tune_vco()?;
        plan.validate(a_hz, b_hz)?;
        Ok(plan)
    }
}

/// Instant calibration's wait for a reference of `reference_hz`, in µs
/// (`lmx2820_tune_internal`: `2.5 * C_BIAS / 0.47` µs per MHz of reference over the
/// calibration clock's divider, rounded; `C_BIAS` = 1 µF).
fn instant_delay(reference_hz: u64) -> u16 {
    let divider = 1_u32 << calibration_clock_divider(reference_hz).value();
    let delay = 2.5 / 0.47 * (c_f64(reference_hz) / 1e6) / f64::from(divider) + 0.5;
    u16::try_from(c_unsigned(delay)).expect("invariant: at most about 1064 µs (200 MHz over 1)")
}
