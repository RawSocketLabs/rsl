//! TI LMK05318 network synchronizer clock, the clock tree of the `SimpleSync` and XMASS
//! front ends. Its bring-up, output plans, lock waits, synchronization and output
//! power-down are `lmk05318`'s blocking driver (`lmk05318::driver`, which `PE_SYNC` runs
//! too), re-exported here; this module adds the lock-status log libusdr writes after each
//! step.

use deps::tracing::{debug, info};
use hal::Bus;
pub(crate) use lmk05318::driver::Lmk05318;

/// Reads and logs the lock status, as libusdr does after each bring-up step
/// (`lmk05318_check_lock(.., false)`, whose result its callers ignore). A failed read ends
/// the reads and is only logged.
pub(crate) fn log_lock(lmk: &Lmk05318, bus: &mut dyn Bus) {
    match lmk.check_lock(bus) {
        Ok(status) => info!(?status, "LMK05318 lock status"),
        Err(error) => debug!(%error, "LMK05318 lock status unreadable"),
    }
}
