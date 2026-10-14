//! Register access for the blocking drivers: `hal`'s executor with the driver's
//! errors. The typed-register traits ([`Register`],
//! [`IndexedRegister`]) are `hal`'s, shared with the sans-I/O machines.

use hal::run_sequence as run_on_bus;

use crate::error::Error;
use crate::lowlevel::Bus;
use crate::portable::sequence::Sequence;

/// Runs a portable register sequence to its end over `bus`: `hal`'s blocking
/// executor, with the failure in the driver's terms.
pub(crate) fn run_sequence(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    run_on_bus(bus, sequence).map_err(Error::from)
}
