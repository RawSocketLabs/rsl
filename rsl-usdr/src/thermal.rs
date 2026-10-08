//! Temperature limits for starting and running the radio.
//!
//! The board's TMP114 reads board temperature, not chip junctions, so limits leave margin
//! below the parts' ratings (about 85 °C for the LMS6002D, Si5332 and a commercial-grade
//! FPGA). Whatever the policy, nothing starts at [`HARD_STOP_CELSIUS`]; the PMIC itself
//! cuts power at 125 °C.
//!
//! Pending: the `stop` limits take effect once streaming is ported, when `receive` checks
//! them. Until then only the start and resume limits are enforced.

use std::time::Duration;

use crate::error::Error;

/// Board temperature at which the driver always refuses to start (and, once streaming is
/// ported, stops streaming).
pub const HARD_STOP_CELSIUS: f32 = 110.0;

/// Interval between temperature readings while waiting for the board to cool.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// When the driver refuses to start or stops the radio because of temperature.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum ThermalPolicy {
    /// Stay within the components' rated range: refuse to start at 80 °C, stop at 85 °C,
    /// ready again below 70 °C.
    #[default]
    WithinSpec,
    /// Run past the rated range, where sample quality and part lifetime are no longer
    /// guaranteed, but stop before damage is likely: refuse at 95 °C, stop at 100 °C,
    /// ready again below 85 °C.
    BeyondSpec,
    /// Caller-chosen limits, still capped by [`HARD_STOP_CELSIUS`].
    Custom(ThermalLimits),
    /// Only the hard stop: refuse and stop at 110 °C, ready again below 100 °C. For
    /// development; expect degraded samples and shortened part life well before this.
    HardStopOnly,
}

impl ThermalPolicy {
    /// The limits this policy enforces.
    #[must_use]
    pub fn limits(self) -> ThermalLimits {
        match self {
            Self::WithinSpec => ThermalLimits {
                start: 80.0,
                stop: 85.0,
                resume: 70.0,
            },
            Self::BeyondSpec => ThermalLimits {
                start: 95.0,
                stop: 100.0,
                resume: 85.0,
            },
            Self::Custom(limits) => limits,
            Self::HardStopOnly => ThermalLimits {
                start: HARD_STOP_CELSIUS,
                stop: HARD_STOP_CELSIUS,
                resume: 100.0,
            },
        }
    }
}

/// Board temperatures, in °C, that gate the radio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThermalLimits {
    /// Refuse to start at or above this.
    start: f32,
    /// Stop a running stream at or above this (enforced once streaming is ported).
    stop: f32,
    /// After a refusal or stop, ready again only below this.
    resume: f32,
}

impl ThermalLimits {
    /// Limits that refuse to start at `start`, stop streaming at `stop`, and allow a
    /// restart only below `resume`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidThermalLimits`] unless all three are finite and
    /// `resume < start <= stop < HARD_STOP_CELSIUS`.
    pub fn new(start: f32, stop: f32, resume: f32) -> Result<Self, Error> {
        let finite = start.is_finite() && stop.is_finite() && resume.is_finite();
        if !(finite && resume < start && start <= stop && stop < HARD_STOP_CELSIUS) {
            return Err(Error::InvalidThermalLimits {
                start,
                stop,
                resume,
            });
        }
        Ok(Self {
            start,
            stop,
            resume,
        })
    }

    /// The temperature at or above which the radio refuses to start.
    #[must_use]
    pub fn start(self) -> f32 {
        self.start
    }

    /// The temperature at or above which a running stream stops (enforced once streaming
    /// is ported).
    #[must_use]
    pub fn stop(self) -> f32 {
        self.stop
    }

    /// The temperature below which the radio may start again.
    #[must_use]
    pub fn resume(self) -> f32 {
        self.resume
    }
}

/// Something that reads the board temperature and can wait: the board before and after
/// power-up.
pub(crate) trait Thermometer {
    /// The board temperature in °C.
    fn celsius(&mut self) -> Result<f32, Error>;
    /// Waits for `duration` of bus time.
    fn sleep(&mut self, duration: Duration);
}

/// How long to wait for the board to cool before giving up, and who to tell about each
/// reading.
pub(crate) struct CoolDown<'a> {
    /// Longest total wait.
    pub(crate) timeout: Duration,
    /// Called with every reading taken while waiting.
    pub(crate) on_reading: Box<dyn FnMut(f32) + 'a>,
}

impl CoolDown<'_> {
    /// Reads the board temperature every [`POLL_INTERVAL`] of bus time until it is below
    /// `limits.resume`, waiting at most `timeout` in total.
    pub(crate) fn wait(
        &mut self,
        sensor: &mut dyn Thermometer,
        limits: ThermalLimits,
    ) -> Result<(), Error> {
        let mut waited = Duration::ZERO;
        loop {
            let celsius = sensor.celsius()?;
            (self.on_reading)(celsius);
            if celsius < limits.resume {
                return Ok(());
            }
            if waited >= self.timeout {
                return Err(Error::Overheated {
                    celsius,
                    limit: limits.resume,
                });
            }
            let nap = POLL_INTERVAL.min(self.timeout.saturating_sub(waited));
            sensor.sleep(nap);
            waited += nap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_limits_must_be_ordered_and_below_the_hard_stop() {
        assert!(ThermalLimits::new(80.0, 90.0, 70.0).is_ok());
        assert!(
            ThermalLimits::new(80.0, 80.0, 70.0).is_ok(),
            "start may equal stop"
        );
        assert!(
            ThermalLimits::new(80.0, 90.0, 80.0).is_err(),
            "resume must be below start"
        );
        assert!(
            ThermalLimits::new(90.0, 80.0, 70.0).is_err(),
            "start must not exceed stop"
        );
        assert!(
            ThermalLimits::new(100.0, HARD_STOP_CELSIUS, 90.0).is_err(),
            "stop must be below the hard stop"
        );
        assert!(
            ThermalLimits::new(f32::NAN, 90.0, 70.0).is_err(),
            "NaN is rejected"
        );
        assert!(
            ThermalLimits::new(80.0, 90.0, f32::NEG_INFINITY).is_err(),
            "infinities are rejected"
        );
    }

    #[test]
    fn every_preset_respects_the_hard_stop() {
        for policy in [
            ThermalPolicy::WithinSpec,
            ThermalPolicy::BeyondSpec,
            ThermalPolicy::HardStopOnly,
        ] {
            let limits = policy.limits();
            assert!(
                limits.resume < limits.start
                    && limits.start <= limits.stop
                    && limits.stop <= HARD_STOP_CELSIUS
            );
        }
    }
}
