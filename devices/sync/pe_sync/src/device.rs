//! The public device: open (power-on, status check, clock tree) and close (power-off), as
//! libusdr's `usdr_device_pe_sync_initialize` and `usdr_device_pe_sync_destroy` run them.

use std::fmt;
use std::time::Duration;

use rsl_lowlevel::{Sequence, run_sequence};

use crate::error::Error;
use crate::lmk05318::Lmk05318;
use crate::lowlevel::{Bus, SpiAddr};
use crate::portable::board::{
    LMK1D1208I_LCK, LMK1D1208I_LRF, LMK1D1208I_SETUP, LMK05318, LMK05318_OUTPUTS, LMK05318_XO,
    LMX1204, LMX1204_SETUP, LMX1214, LMX1214_SETUP, LMX2820_0, LMX2820_0_TUNE, LMX2820_1,
    LMX2820_1_TUNE,
};
use crate::portable::chain::Lenient;
use crate::portable::lmk1d1208i;
use crate::portable::lmx1204::{self, sequence::WaitLock};
use crate::portable::lmx1214;
use crate::portable::lmx2820::{self, plan::Request, sequence::Tune};
use crate::portable::power::Power;
use crate::portable::status::status_check;

/// How long each LMK05318 APLL may take to lock (`lmk05318_wait_apll1_lock(.., 100000)`).
const APLL_LOCK_TIMEOUT: Duration = Duration::from_millis(100);

/// The pause after the LMK05318's synchronization ("LMX2820[0] will not start without
/// it").
const LMX2820_START_DELAY: Duration = Duration::from_secs(2);

/// An open `PE_SYNC` board: powered, its clock tree configured and locked.
///
/// Dropping it leaves the board as it is; [`Device::close`] powers it off.
pub struct Device {
    /// The board's bus.
    bus: Box<dyn Bus>,
}

impl Device {
    /// Opens the board on `bus`, as libusdr's open does: powers the inputs, synthesizers,
    /// generator and distribution up behind 1.1 s of LED fades; reads one status value from
    /// each chip; then configures the clock tree from the LMK05318B down (see the crate
    /// docs) and waits for each PLL to lock. On failure the board is powered off again, as
    /// libusdr does.
    ///
    /// # Errors
    ///
    /// [`Error::ChipId`] for an LMK05318 or LMK1D1208I that is not one; [`Error::Unlocked`]
    /// for a PLL that does not lock in time; [`Error::ClockPlan`] or
    /// [`Error::NoClockPlan`] should a chip have no configuration for the board's clock
    /// tree (none is expected); [`Error::Bus`] when an access fails.
    pub fn open(bus: impl Bus + 'static) -> Result<Self, Error> {
        let mut bus: Box<dyn Bus> = Box::new(bus);
        if let Err(error) = bring_up(&mut *bus) {
            // libusdr powers the board off after a failed open and returns the open's error,
            // whatever the power-off's outcome.
            run(&mut *bus, &mut Lenient(Power::off()))?;
            return Err(error);
        }
        Ok(Self { bus })
    }

    /// Powers the board off (`usdr_device_pe_sync_destroy`): LEDs and every supply off, then
    /// a 1 s fade to dark.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] when a supply write fails; the remaining writes and the fade are
    /// skipped, as in libusdr.
    pub fn close(mut self) -> Result<(), Error> {
        run(&mut *self.bus, &mut Power::off())
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device").finish_non_exhaustive()
    }
}

/// `usdr_device_pe_sync_initialize` up to its power-off on failure.
fn bring_up(bus: &mut dyn Bus) -> Result<(), Error> {
    run(bus, &mut Power::on())?;
    run(bus, &mut status_check())?;
    configure(bus)
}

/// `pe_sync_configure`: the clock tree, from the LMK05318B down.
fn configure(bus: &mut dyn Bus) -> Result<(), Error> {
    let lmk05318 = Lmk05318::create(bus, LMK05318, LMK05318_XO, &LMK05318_OUTPUTS)?;
    // The DPLL is off, so libusdr's reference validation wait returns at once.
    let locked = lmk05318
        .wait_apll1_lock(bus, APLL_LOCK_TIMEOUT)
        .and_then(|()| lmk05318.wait_apll2_lock(bus, APLL_LOCK_TIMEOUT));
    // "just to log state": libusdr ignores the result.
    let _logged = lmk05318.check_lock(bus);
    locked?;
    lmk05318.sync(bus)?;
    bus.sleep(LMX2820_START_DELAY);

    synthesizer(bus, LMX2820_0, &LMX2820_0_TUNE)?;
    synthesizer(bus, LMX2820_1, &LMX2820_1_TUNE)?;
    lo_distribution(bus)?;
    clock_distribution(bus)?;
    run(
        bus,
        &mut lmk1d1208i::Create::new(LMK1D1208I_LCK, LMK1D1208I_SETUP),
    )?;
    run(
        bus,
        &mut lmk1d1208i::Create::new(LMK1D1208I_LRF, LMK1D1208I_SETUP),
    )
}

/// One LMX2820: created, tuned and locked, then its status read "just for logging",
/// whatever the outcome.
fn synthesizer(bus: &mut dyn Bus, target: SpiAddr, request: &Request) -> Result<(), Error> {
    let name = lmx2820::sequence::NAME;
    let tuned = run(bus, &mut lmx2820::sequence::Create::new(target)).and_then(|()| {
        let plan = lmx2820::plan::Plan::solve(request).map_err(|error| Error::NoClockPlan {
            chip: name,
            reason: error.reason(),
        })?;
        let mut tune = Tune::new(target, &plan);
        run(bus, &mut tune)?;
        tune.locked().then_some(()).ok_or(Error::Unlocked(name))
    });
    run(bus, &mut Lenient(lmx2820::sequence::status(target)))?;
    tuned
}

/// The LMX1214: created and configured, then its temperature read "just for logging",
/// whatever the outcome.
fn lo_distribution(bus: &mut dyn Bus) -> Result<(), Error> {
    let configured = run(bus, &mut lmx1214::sequence::create(LMX1214)).and_then(|()| {
        let plan =
            lmx1214::plan::Plan::solve(&LMX1214_SETUP).map_err(|error| Error::NoClockPlan {
                chip: lmx1214::sequence::NAME,
                reason: error.reason(),
            })?;
        run(bus, &mut lmx1214::sequence::configure(LMX1214, &plan))
    });
    run(bus, &mut Lenient(lmx1214::sequence::temperature(LMX1214)))?;
    configured
}

/// The LMX1204: created and configured, then the multiplier's lock wait between two status
/// reads libusdr only logs.
fn clock_distribution(bus: &mut dyn Bus) -> Result<(), Error> {
    let name = lmx1204::sequence::NAME;
    run(bus, &mut lmx1204::sequence::create(LMX1204))?;
    let plan = lmx1204::plan::Plan::solve(&LMX1204_SETUP).map_err(|error| Error::NoClockPlan {
        chip: name,
        reason: error.reason(),
    })?;
    run(bus, &mut lmx1204::sequence::configure(LMX1204, &plan))?;
    run(bus, &mut Lenient(lmx1204::sequence::status(LMX1204)))?;
    let mut wait = WaitLock::new(LMX1204, &plan);
    let locked =
        run(bus, &mut wait).and_then(|()| wait.locked().then_some(()).ok_or(Error::Unlocked(name)));
    run(bus, &mut Lenient(lmx1204::sequence::status(LMX1204)))?;
    locked
}

/// Runs a portable sequence to its end over `bus`, with the failure in the driver's terms.
fn run(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    run_sequence(bus, sequence).map_err(Error::from)
}
