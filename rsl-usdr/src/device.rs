//! The radio as a caller sees it.

use std::fmt;
use std::time::Duration;

use crate::board::Board;
use crate::error::Error;
use crate::lowlevel::Bus;
use crate::thermal::{CoolDown, ThermalPolicy};

/// A powered uSDR.
///
/// Dropping a `Device` powers the board down on a best-effort basis; call
/// [`Device::close`] to see power-down errors.
pub struct Device {
    /// The powered board, which owns the bus; `None` only once powered down.
    board: Option<Board>,

    /// Temperature limits in force.
    thermal: ThermalPolicy,
}

/// Options for opening a [`Device`]; see [`Device::builder`]. `'a` bounds the borrow of
/// a [`DeviceBuilder::wait_for_temperature`] callback.
pub struct DeviceBuilder<'a> {
    /// The hardware seam.
    bus: Box<dyn Bus>,

    /// Temperature limits to enforce.
    thermal: ThermalPolicy,

    /// Wait for a hot board to cool, reporting each reading, instead of failing.
    cool_down: Option<CoolDown<'a>>,
}

impl Device {
    /// Opening options for a board reached through a custom [`Bus`]. Defaults:
    /// [`ThermalPolicy::WithinSpec`], and fail at once if the board is too hot.
    pub fn builder<'a>(bus: impl Bus + 'static) -> DeviceBuilder<'a> {
        DeviceBuilder {
            bus: Box::new(bus),
            thermal: ThermalPolicy::default(),
            cool_down: None,
        }
    }

    /// Powers up a board reached through a custom [`Bus`] with the default options.
    ///
    /// # Errors
    ///
    /// As [`DeviceBuilder::open`].
    pub fn with_bus(bus: impl Bus + 'static) -> Result<Self, Error> {
        Self::builder(bus).open()
    }

    /// The board temperature in °C.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn temperature(&mut self) -> Result<f32, Error> {
        self.board().temperature()
    }

    /// Waits until the board is below the thermal policy's resume limit, reading every
    /// 5 s and passing each reading to `on_reading`.
    ///
    /// # Errors
    ///
    /// [`Error::Overheated`], with the resume limit, if the board is still too warm after
    /// `timeout`; any bus failure.
    pub fn wait_for_temperature(
        &mut self,
        timeout: Duration,
        on_reading: impl FnMut(f32),
    ) -> Result<(), Error> {
        let mut cool_down = CoolDown {
            timeout,
            on_reading: Box::new(on_reading),
        };
        let limits = self.thermal.limits();
        cool_down.wait(self.board(), limits)
    }

    /// Powers the board down.
    ///
    /// # Errors
    ///
    /// The first bus failure; every power-down step is still attempted.
    pub fn close(mut self) -> Result<(), Error> {
        self.power_down()
    }

    /// The powered board.
    fn board(&mut self) -> &mut Board {
        self.board
            .as_mut()
            .expect("invariant: a Device owns its board until close or drop")
    }

    /// Powers the board down if it is still up.
    fn power_down(&mut self) -> Result<(), Error> {
        self.board.take().map_or(Ok(()), Board::power_down)
    }
}

impl<'a> DeviceBuilder<'a> {
    /// Enforces `policy` instead of [`ThermalPolicy::WithinSpec`].
    #[must_use]
    pub fn thermal(mut self, policy: ThermalPolicy) -> Self {
        self.thermal = policy;
        self
    }

    /// If the board is too hot to start, waits up to `timeout` for it to cool below the
    /// policy's resume limit, passing each reading (every 5 s) to `on_reading`.
    #[must_use]
    pub fn wait_for_temperature(
        mut self,
        timeout: Duration,
        on_reading: impl FnMut(f32) + 'a,
    ) -> Self {
        self.cool_down = Some(CoolDown {
            timeout,
            on_reading: Box::new(on_reading),
        });
        self
    }

    /// Identifies the board, confirms its temperature sensor, applies the thermal policy
    /// before anything is powered, then powers the board up.
    ///
    /// # Errors
    ///
    /// - [`Error::Overheated`] if the board is too hot: with the start limit when failing
    ///   at once, with the resume limit after a configured wait times out.
    /// - [`Error::ChipId`] if no TMP114 answers, so temperature cannot be trusted.
    /// - Any bus failure, an unsupported board revision, or another chip that does not
    ///   identify itself as expected.
    pub fn open(self) -> Result<Device, Error> {
        let Self {
            bus,
            thermal,
            cool_down,
        } = self;
        let mut identified = Board::identify(bus)?;
        identified.check_temperature_sensor()?;
        let limits = thermal.limits();
        let celsius = identified.temperature()?;
        if celsius >= limits.start() {
            let Some(mut cool_down) = cool_down else {
                return Err(Error::Overheated {
                    celsius,
                    limit: limits.start(),
                });
            };
            cool_down.wait(&mut identified, limits)?;
        }
        let board = identified.power_up()?;
        Ok(Device {
            board: Some(board),
            thermal,
        })
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("powered", &self.board.is_some())
            .field("thermal", &self.thermal)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for DeviceBuilder<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceBuilder")
            .field("thermal", &self.thermal)
            .field(
                "cool_down",
                &self.cool_down.as_ref().map(|cool_down| cool_down.timeout),
            )
            .finish_non_exhaustive()
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // Errors are reported only by `close`.
        let _ = self.power_down();
    }
}
