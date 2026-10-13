//! The public device: identification, supply bring-up, RFIC power-up and close.

use std::fmt;

use rsl_lms7002m::Lms7002m;
use rsl_lowlevel::{Sequence, run_sequence};
use rsl_lp8758::{self as lp8758, Revision};

use crate::board::{XSDR_LMS, XSDR_SUPPLIES};
use crate::error::Error;
use crate::lowlevel::Bus;
use crate::portable::close::Close;
use crate::portable::hwid::{IdentityRead, Variant};
use crate::portable::power::PowerUp;
use crate::portable::supplies::Supplies;

/// An open LMS7 family board. Only the XSDR opens so far.
pub struct Device {
    /// The board's bus.
    bus: Box<dyn Bus>,

    /// The hardware ID read at open.
    hwid: u32,

    /// The PMIC's revision, read at open.
    pmic: Revision,

    /// The LMS7002M's cached state: zeroed until a power-up creates it, as in libusdr.
    lms: Lms7002m,
}

impl Device {
    /// Opens the board on `bus`, as libusdr's open does up to its front-end probe: reads the
    /// hardware ID, pulses the LMS7002M power control, checks the LP8758's revision, sets
    /// the rails, turns on all four converters and selects the internal reference clock.
    /// The RFIC stays unpowered ([`Self::power_up`]).
    ///
    /// Not yet ported: the front-end probe libusdr runs next on I2C bus 1, so no front end
    /// is detected or brought up.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownDevice`] or [`Error::UnsupportedVariant`] after the identification
    /// reads, before anything is driven; [`Error::Bus`] when an access fails.
    pub fn open(bus: impl Bus + 'static) -> Result<Self, Error> {
        let mut bus: Box<dyn Bus> = Box::new(bus);
        let mut identity = IdentityRead::new();
        run(&mut *bus, &mut identity)?;
        let hwid = identity.hwid();
        match Variant::from_hwid(hwid).map_err(Error::UnknownDevice)? {
            Variant::Xsdr => {}
            other => return Err(Error::UnsupportedVariant(other)),
        }

        let mut supplies = Supplies::new(XSDR_SUPPLIES);
        run(&mut *bus, &mut supplies)?;
        Ok(Self {
            bus,
            hwid,
            pmic: supplies.pmic_revision(),
            lms: Lms7002m::new(XSDR_LMS.target),
        })
    }

    /// Powers the LMS7002M up (libusdr's `/dm/power/en` = 1): its LDOs on, its reset
    /// released, its reset programme loaded and its version checked.
    ///
    /// # Errors
    ///
    /// [`Error::ChipId`] for an LP8758 whose revision is not 0xE001, the one libusdr
    /// accepts, before any bus access; [`Error::ChipId`] for an LMS7002M other than version 7,
    /// revision 1; [`Error::Bus`] when an access fails.
    pub fn power_up(&mut self) -> Result<(), Error> {
        if self.pmic != lp8758::REVISION {
            return Err(Error::ChipId {
                chip: lp8758::NAME,
                expected: lp8758::REVISION.to_raw().into(),
                found: self.pmic.to_raw().into(),
            });
        }
        let mut power = PowerUp::new(XSDR_LMS);
        run(&mut *self.bus, &mut power)?;
        if let Some(lms) = power.chip() {
            self.lms = lms;
        }
        Ok(())
    }

    /// Closes the board as libusdr's `usdr_dmd_close` does on an XSDR without streams
    /// (`xsdr_dtor`): the LMS7002M's receive and transmit paths brought down and the chip
    /// destroyed, its power control, LDOs and the LED off, the TX MMCM powered down.
    ///
    /// The chip writes follow its cached state, so a board never powered up gets the words
    /// libusdr sends from its zeroed state. A device dropped without `close` leaves the
    /// board as it is.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] for the first failure that ends the close, as libusdr's: the AFE write
    /// that ends streaming-down, a `lms7002m_destroy` write or a board write. Failed writes
    /// earlier in streaming-down are skipped without an error, as libusdr skips them.
    pub fn close(mut self) -> Result<(), Error> {
        run(&mut *self.bus, &mut Close::new(self.lms))
    }

    /// The hardware ID word: device ID in bits 23:16, gateware revision in 15:8, PHY
    /// features in 7:0.
    #[must_use]
    pub fn hwid(&self) -> u32 {
        self.hwid
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("hwid", &format_args!("{:#010x}", self.hwid))
            .field("pmic", &self.pmic)
            .finish_non_exhaustive()
    }
}

/// Runs a portable sequence to its end over `bus`, with the failure in the driver's terms.
fn run(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    run_sequence(bus, sequence).map_err(Error::from)
}
