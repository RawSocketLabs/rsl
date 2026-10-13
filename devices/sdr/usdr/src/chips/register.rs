//! Register access for the blocking drivers: `rsl-lowlevel`'s executor with the driver's
//! errors. The typed-register traits ([`Register`],
//! [`IndexedRegister`]) are `rsl-lowlevel`'s, shared with the sans-I/O machines.

use rsl_lowlevel::run_sequence as run_on_bus;

use crate::error::Error;
use crate::lowlevel::Bus;
use crate::portable::sequence::Sequence;

/// Runs a portable register sequence to its end over `bus`: `rsl-lowlevel`'s blocking
/// executor, with the failure in the driver's terms.
pub(crate) fn run_sequence(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    run_on_bus(bus, sequence).map_err(Error::from)
}
