//! The LMK05318's bus traffic as libusdr's `hw/lmk05318/lmk05318.c` runs it: bring-up,
//! output plans and retunes, lock waits, lock status, synchronization and output
//! power-down, over `hal`'s blocking [`Bus`].
//!
//! Blocking code, not `Sequence`s: the chip's 16-bit register pointer and four-byte
//! identity read do not fit `hal`'s portable seam. Every board that carries the chip runs
//! this one copy (`PE_SYNC`, and the `SimpleSync` and XMASS front ends through
//! `sdr-frontend`), each mapping [`Error`] into its own. Without the DPLL, which no ported
//! board enables (not ported), APLL1 runs free.

use core::error;
use core::fmt;
use core::time::Duration;

use hal::{Access, Bus, BusError, I2cAddr};

use super::apll::{Apll, BawLock, Pll1Control, Pll2Control, VcoStatus};
use super::control::{DeviceControl, DpllFlags, LossFlags};
use super::map::RegisterMap;
use super::output::{OutputControlReg, without_format};
use super::plan::{Apll2, ClockPlanError, OutputRequest, Xo};
use super::reg::{IndexedLmk05318Register, Lmk05318Register, Reg};
use super::staging::{stage_apll1, stage_common, stage_plan, stage_xo};
use super::{ID, NAME};

/// How long a lock wait sleeps between polls (`usleep(100)`).
const POLL_INTERVAL: Duration = Duration::from_micros(100);

/// The pause after each write of a toggled bit (`lmk05318_trigger_reg_with_sleep`), and
/// after the bring-up's register load.
const SETTLE: Duration = Duration::from_millis(10);

/// A failed LMK05318 operation. Exhaustive on purpose: each driver maps every variant into
/// its own error, and a new one must stop those maps compiling.
#[derive(Debug)]
pub enum Error {
    /// A register access failed on the bus.
    Bus {
        /// Whether it was a read or a write.
        access: Access,
        /// The register address.
        reg: u16,
        /// The bus failure.
        source: BusError,
    },
    /// Another chip answered (libusdr's `-ENODEV`): the identity bytes as one big-endian
    /// word.
    ChipId {
        /// [`ID`].
        expected: u32,
        /// What was read.
        found: u32,
    },
    /// No plan exists for the request (libusdr's `-EINVAL`).
    ClockPlan(ClockPlanError),
    /// An APLL did not lock in time (libusdr's `-ETIMEDOUT`): `"LMK05318 APLL1"` or
    /// `"LMK05318 APLL2"`.
    Unlocked(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus { access, reg, .. } => {
                write!(f, "{NAME} register {reg:#06x} {access} failed")
            }
            Self::ChipId { expected, found } => {
                write!(f, "{NAME} reports ID {found:#x}, expected {expected:#x}")
            }
            Self::ClockPlan(error) => error.fmt(f),
            Self::Unlocked(pll) => write!(f, "{pll} did not lock"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Bus { source, .. } => Some(source),
            Self::ClockPlan(error) => Some(error),
            Self::ChipId { .. } | Self::Unlocked(_) => None,
        }
    }
}

/// A plan the chip cannot take.
impl From<ClockPlanError> for Error {
    fn from(error: ClockPlanError) -> Self {
        Self::ClockPlan(error)
    }
}

/// An LMK05318 on the I2C bus, with the state libusdr keeps for it (`lmk05318_state_t`
/// without the DPLL, and its register staging map).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lmk05318 {
    /// Where the chip answers.
    dev: I2cAddr,
    /// APLL2's dividers and last plan.
    apll2: Apll2,
    /// Registers staged for the next write.
    map: RegisterMap,
}

/// The latched loss flags and BAW lock, as `lmk05318_check_lock` reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockStatus {
    /// XO and APLL losses (`INT_FLAG0`).
    pub loss: LossFlags,
    /// DPLL events (`INT_FLAG1`); read, not judged without a DPLL.
    pub dpll: DpllFlags,
    /// BAW lock.
    pub baw: BawLock,
}

impl Lmk05318 {
    /// The chip at `dev`, as `lmk05318_create` sets its state up before talking to it:
    /// APLL2 cascaded from APLL1, nothing planned or staged.
    #[must_use]
    pub fn at(dev: I2cAddr) -> Self {
        Self {
            dev,
            apll2: Apll2::cascaded(),
            map: RegisterMap::new(),
        }
    }

    /// Brings the chip up (`lmk05318_create` without a DPLL): checks its identity, plans
    /// `xo`, APLL1 and `outputs`, writes the plan with the common settings in address
    /// order, waits for it to settle and soft-resets.
    ///
    /// The state starts over first and keeps whatever the steps reached, so a caller that
    /// carries on after a failure (XMASS does) sees libusdr's state.
    ///
    /// # Errors
    ///
    /// [`Error::ChipId`] when another chip answers, before any write;
    /// [`Error::ClockPlan`] when no plan exists, before any write; any bus failure.
    pub fn create(
        &mut self,
        bus: &mut dyn Bus,
        xo: Xo,
        outputs: &[OutputRequest],
    ) -> Result<(), Error> {
        *self = Self::at(self.dev);
        let id = self.read_id(bus)?;
        if id != ID {
            return Err(Error::ChipId {
                expected: u32::from_be_bytes(ID),
                found: u32::from_be_bytes(id),
            });
        }
        self.configure(bus, xo, outputs)
    }

    /// The rest of `lmk05318_create` once the chip has identified itself (`SimpleSync`'s
    /// probe reads the ID itself): plans `xo`, APLL1 and `outputs`, writes the plan with
    /// the common settings, waits for it to settle and soft-resets. Call it on a state
    /// fresh from [`Lmk05318::at`].
    ///
    /// # Errors
    ///
    /// [`Error::ClockPlan`] when no plan exists; any bus failure.
    pub fn configure(
        &mut self,
        bus: &mut dyn Bus,
        xo: Xo,
        outputs: &[OutputRequest],
    ) -> Result<(), Error> {
        let path = stage_xo(&mut self.map, xo)?;
        stage_apll1(&mut self.map, xo.apll1(path)?);
        self.plan(outputs)?;
        stage_common(&mut self.map);
        self.write_staged(bus)?;
        bus.sleep(SETTLE);
        self.soft_reset(bus)
    }

    /// Plans `outputs` and stages their registers and APLL2's (`lmk05318_solver`); the
    /// next [`Lmk05318::write_staged`] writes them. Outputs not named keep their settings.
    ///
    /// # Errors
    ///
    /// [`Error::ClockPlan`] when no plan exists; nothing is staged then, but APLL2's state
    /// may have moved on, as libusdr's does.
    pub fn plan(&mut self, outputs: &[OutputRequest]) -> Result<(), Error> {
        let plan = self.apll2.plan(outputs)?;
        Ok(stage_plan(&mut self.map, &self.apll2, &plan)?)
    }

    /// Writes the staged registers in address order, then empties the map
    /// (`lmk05318_reg_wr_from_map`). A failed write leaves the map as it was.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn write_staged(&mut self, bus: &mut dyn Bus) -> Result<(), Error> {
        for (reg, value) in self.map.staged() {
            self.write(bus, reg, value)?;
        }
        self.map.clear();
        Ok(())
    }

    /// Calibrates APLL1 and waits up to `timeout` for it to lock
    /// (`lmk05318_wait_apll1_lock` without a DPLL): its charge pump in range and the BAW
    /// locked. A zero timeout waits for ever, as libusdr's does.
    ///
    /// libusdr budgets the timeout in wall-clock time; this counts the 100 µs sleeps
    /// between polls, so on real buses it polls at least as long.
    ///
    /// # Errors
    ///
    /// [`Error::Unlocked`] on timeout; any bus failure.
    pub fn wait_apll1_lock(&self, bus: &mut dyn Bus, timeout: Duration) -> Result<(), Error> {
        self.toggle(
            bus,
            Pll1Control::new().with_powered_down(true),
            Pll1Control::new(),
        )?;
        self.reset_loss_flags(bus)?;
        self.poll_lock(bus, Apll::Apll1, timeout, |status| status.baw.locked())?
            .then_some(())
            .ok_or(Error::Unlocked("LMK05318 APLL1"))
    }

    /// Calibrates APLL2 and waits up to `timeout` for it to lock
    /// (`lmk05318_wait_apll2_lock`): its charge pump in range and no loss of lock latched.
    /// Nothing happens until a plan has used APLL2. Timing as for
    /// [`Lmk05318::wait_apll1_lock`].
    ///
    /// # Errors
    ///
    /// [`Error::Unlocked`] on timeout; any bus failure.
    pub fn wait_apll2_lock(&self, bus: &mut dyn Bus, timeout: Duration) -> Result<(), Error> {
        if self.apll2.vco_hz() == 0 {
            return Ok(());
        }
        let (primary, secondary) = (self.apll2.primary_divider(), self.apll2.secondary_divider());
        self.toggle(
            bus,
            Pll2Control::with_dividers(primary, secondary, true),
            Pll2Control::with_dividers(primary, secondary, false),
        )?;
        self.reset_loss_flags(bus)?;
        self.poll_lock(bus, Apll::Apll2, timeout, |status| {
            !status.loss.apll2_unlocked()
        })?
        .then_some(())
        .ok_or(Error::Unlocked("LMK05318 APLL2"))
    }

    /// Reads the latched loss flags and BAW lock (`lmk05318_check_lock`), stopping at the
    /// first failed read. libusdr only logs them; its callers ignore the result.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn check_lock(&self, bus: &mut dyn Bus) -> Result<LockStatus, Error> {
        Ok(LockStatus {
            loss: self.read_typed(bus)?,
            dpll: self.read_typed(bus)?,
            baw: self.read_typed(bus)?,
        })
    }

    /// Synchronizes the outputs: `SYNC_SW` set, then cleared (`lmk05318_sync`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn sync(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let control: DeviceControl = self.read_typed(bus)?;
        self.toggle(bus, control.with_sync(true), control.with_sync(false))
    }

    /// Turns output `port` off, keeping its source (`lmk05318_disable_port`).
    ///
    /// # Errors
    ///
    /// [`Error::ClockPlan`] for an output past 7, before any bus access; any bus failure.
    pub fn disable_output(&self, bus: &mut dyn Bus, port: u8) -> Result<(), Error> {
        let reg = OutputControlReg::of(port)
            .ok_or(ClockPlanError::BadRequest)?
            .addr();
        let control = self.read(bus, reg.into())?;
        self.write(bus, reg.into(), without_format(control))
    }

    /// Reads the four identification bytes from address 0, in wire order
    /// (`lmk05318_reg_get_u32(.., 0, ..)`).
    fn read_id(&self, bus: &mut dyn Bus) -> Result<[u8; 4], Error> {
        let reg = u16::from(Reg::Id);
        let mut raw = [0; 4];
        bus.i2c(self.dev, &reg.to_be_bytes(), &mut raw)
            .map_err(|source| bus_error(Access::Read, reg, source))?;
        Ok(raw)
    }

    /// Soft-resets the chip: `RESET_SW` set, then cleared (`lmk05318_softreset`).
    fn soft_reset(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let control: DeviceControl = self.read_typed(bus)?;
        self.toggle(bus, control.with_reset(true), control.with_reset(false))
    }

    /// Clears the latched loss flags (`lmk05318_reset_los_flags`).
    fn reset_loss_flags(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.write_typed(bus, LossFlags::new())?;
        self.write_typed(bus, DpllFlags::new())
    }

    /// Polls `apll`'s VCO status and the lock status until the VCO is in range and
    /// `locked` holds, sleeping between polls, for at most `timeout` of sleeps (for ever
    /// when zero); `false` when it times out.
    fn poll_lock(
        &self,
        bus: &mut dyn Bus,
        apll: Apll,
        timeout: Duration,
        locked: impl Fn(&LockStatus) -> bool,
    ) -> Result<bool, Error> {
        let mut waited = Duration::ZERO;
        while timeout.is_zero() || waited < timeout {
            let vco: VcoStatus = self.read_at(bus, apll)?;
            let status = self.check_lock(bus)?;
            if vco.in_range() && locked(&status) {
                return Ok(true);
            }
            bus.sleep(POLL_INTERVAL);
            waited += POLL_INTERVAL;
        }
        Ok(false)
    }

    /// Writes `set`, then `clear`, pausing after each (`lmk05318_trigger_reg_with_sleep`).
    fn toggle<R: Lmk05318Register>(
        &self,
        bus: &mut dyn Bus,
        set: R,
        clear: R,
    ) -> Result<(), Error> {
        self.write_typed(bus, set)?;
        bus.sleep(SETTLE);
        self.write_typed(bus, clear)?;
        bus.sleep(SETTLE);
        Ok(())
    }

    /// Reads a typed register.
    fn read_typed<R: Lmk05318Register>(&self, bus: &mut dyn Bus) -> Result<R, Error> {
        self.read(bus, R::ADDR.into()).map(R::from)
    }

    /// Reads one copy of a repeated register.
    fn read_at<R: IndexedLmk05318Register>(
        &self,
        bus: &mut dyn Bus,
        index: R::Index,
    ) -> Result<R, Error> {
        self.read(bus, R::addr(index).into()).map(R::from)
    }

    /// Writes a typed register.
    fn write_typed<R: Lmk05318Register>(&self, bus: &mut dyn Bus, value: R) -> Result<(), Error> {
        self.write(bus, R::ADDR.into(), value.into())
    }

    /// Reads one register (`lmk05318_reg_rd`).
    fn read(&self, bus: &mut dyn Bus, reg: u16) -> Result<u8, Error> {
        let mut value = [0];
        bus.i2c(self.dev, &reg.to_be_bytes(), &mut value)
            .map_err(|source| bus_error(Access::Read, reg, source))?;
        Ok(value[0])
    }

    /// Writes one register (`lmk05318_reg_wr`).
    fn write(&self, bus: &mut dyn Bus, reg: u16, value: u8) -> Result<(), Error> {
        let [high, low] = reg.to_be_bytes();
        bus.i2c(self.dev, &[high, low, value], &mut [])
            .map_err(|source| bus_error(Access::Write, reg, source))
    }
}

/// A failed access to register `reg`.
const fn bus_error(access: Access, reg: u16, source: BusError) -> Error {
    Error::Bus {
        access,
        reg,
        source,
    }
}
