//! The LMK05318B's bus traffic as `PE_SYNC`'s open runs it: libusdr's `lmk05318_create`,
//! `lmk05318_wait_apll1_lock`, `lmk05318_wait_apll2_lock`, `lmk05318_check_lock` and
//! `lmk05318_sync` (`hw/lmk05318/lmk05318.c`), on `lmk05318`'s plan, staging map and
//! register types.
//!
//! Blocking code: the chip's 16-bit register pointer and four-byte identity read do not fit
//! `hal`'s `Sequence` seam. `sdr-frontend` runs the same steps for the front ends in
//! its own blocking driver (`src/chips/lmk05318.rs`, with its own error type); the two
//! merge into `lmk05318` once the seam carries such transfers (`PORTING.md`).

use std::time::Duration;

use lmk05318::apll::{Apll, BawLock, Pll1Control, Pll2Control, VcoStatus};
use lmk05318::control::{DeviceControl, DpllFlags, LossFlags};
use lmk05318::map::RegisterMap;
use lmk05318::plan::{Apll2, OutputRequest, Xo};
use lmk05318::reg::{IndexedLmk05318Register, Lmk05318Register, Reg};
use lmk05318::staging::{stage_apll1, stage_common, stage_plan, stage_xo};
use lmk05318::{ID, NAME};

use crate::error::{Access, Error};
use crate::lowlevel::{Bus, BusError, I2cAddr};

/// How a lock wait sleeps between polls (`usleep(100)`).
const POLL_INTERVAL: Duration = Duration::from_micros(100);

/// The pause after each write of a toggled bit (`lmk05318_trigger_reg_with_sleep`), and
/// after the bring-up's register load.
const SETTLE: Duration = Duration::from_millis(10);

/// An LMK05318B brought up, with the APLL2 state its lock wait needs.
#[derive(Clone, Debug)]
pub(crate) struct Lmk05318 {
    /// Where the chip answers.
    dev: I2cAddr,
    /// APLL2's dividers and plan.
    apll2: Apll2,
}

impl Lmk05318 {
    /// Brings the chip up (`lmk05318_create` without a DPLL): checks its identity, plans
    /// `xo`, APLL1 and `outputs`, writes the plan with the common settings in address order,
    /// waits for it to settle and soft-resets.
    ///
    /// # Errors
    ///
    /// [`Error::ChipId`] when another chip answers (libusdr's `-ENODEV`), before any write;
    /// [`Error::ClockPlan`] when no plan exists, before any write; any bus failure.
    pub(crate) fn create(
        bus: &mut dyn Bus,
        dev: I2cAddr,
        xo: Xo,
        outputs: &[OutputRequest],
    ) -> Result<Self, Error> {
        let mut chip = Self {
            dev,
            apll2: Apll2::cascaded(),
        };
        let id = chip.read_id(bus)?;
        if id != ID {
            return Err(Error::ChipId {
                chip: NAME,
                expected: u32::from_be_bytes(ID),
                found: u32::from_be_bytes(id),
            });
        }
        let mut map = RegisterMap::new();
        let path = stage_xo(&mut map, xo)?;
        stage_apll1(&mut map, xo.apll1(path)?);
        let plan = chip.apll2.plan(outputs)?;
        stage_plan(&mut map, &chip.apll2, &plan)?;
        stage_common(&mut map);
        for (reg, value) in map.staged() {
            chip.write(bus, reg, value)?;
        }
        bus.sleep(SETTLE);
        let control: DeviceControl = chip.read_typed(bus)?;
        chip.toggle(bus, control.with_reset(true), control.with_reset(false))?;
        Ok(chip)
    }

    /// Calibrates APLL1 and waits up to `timeout` for it to lock
    /// (`lmk05318_wait_apll1_lock` without a DPLL): its charge pump in range and the BAW
    /// locked.
    ///
    /// # Errors
    ///
    /// [`Error::Unlocked`] on timeout; any bus failure.
    pub(crate) fn wait_apll1_lock(
        &self,
        bus: &mut dyn Bus,
        timeout: Duration,
    ) -> Result<(), Error> {
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
    /// Nothing happens when no output uses APLL2.
    ///
    /// # Errors
    ///
    /// [`Error::Unlocked`] on timeout; any bus failure.
    pub(crate) fn wait_apll2_lock(
        &self,
        bus: &mut dyn Bus,
        timeout: Duration,
    ) -> Result<(), Error> {
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

    /// Reads the latched loss flags and the BAW lock (`lmk05318_check_lock`), which libusdr
    /// logs after the lock waits; the first failed read ends it.
    ///
    /// # Errors
    ///
    /// Any bus failure; `PE_SYNC`'s open ignores it, as libusdr does.
    pub(crate) fn check_lock(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.lock_status(bus).map(|_| ())
    }

    /// Synchronizes the outputs: `SYNC_SW` set, then cleared (`lmk05318_sync`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub(crate) fn sync(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let control: DeviceControl = self.read_typed(bus)?;
        self.toggle(bus, control.with_sync(true), control.with_sync(false))
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

    /// Clears the latched loss flags (`lmk05318_reset_los_flags`).
    fn reset_loss_flags(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.write_typed(bus, LossFlags::new())?;
        self.write_typed(bus, DpllFlags::new())
    }

    /// The latched loss flags and BAW lock, read in that order.
    fn lock_status(&self, bus: &mut dyn Bus) -> Result<LockStatus, Error> {
        Ok(LockStatus {
            loss: self.read_typed(bus)?,
            _dpll: self.read_typed(bus)?,
            baw: self.read_typed(bus)?,
        })
    }

    /// Polls `apll`'s VCO status and the lock status until the VCO is in range and
    /// `locked` holds, sleeping between polls, for at most `timeout` of sleeps; `false`
    /// when it times out. libusdr budgets the timeout in wall-clock time; counting the
    /// sleeps waits at least as long on hardware.
    fn poll_lock(
        &self,
        bus: &mut dyn Bus,
        apll: Apll,
        timeout: Duration,
        locked: impl Fn(&LockStatus) -> bool,
    ) -> Result<bool, Error> {
        let mut waited = Duration::ZERO;
        while waited < timeout {
            let vco: VcoStatus = self.read_at(bus, apll)?;
            let status = self.lock_status(bus)?;
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

/// The latched loss flags and BAW lock, as `lmk05318_check_lock` reads them.
#[derive(Clone, Copy, Debug)]
struct LockStatus {
    /// XO and APLL losses (`INT_FLAG0`).
    loss: LossFlags,
    /// DPLL events (`INT_FLAG1`); read, not judged without a DPLL.
    _dpll: DpllFlags,
    /// BAW lock.
    baw: BawLock,
}

/// A failed LMK05318 access.
fn bus_error(access: Access, reg: u16, source: BusError) -> Error {
    Error::Bus {
        chip: NAME,
        access,
        reg: reg.into(),
        source,
    }
}
