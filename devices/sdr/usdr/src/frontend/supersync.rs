//! The `SuperSync` clock board (libusdr's `supersync`, `device/ext_supersync/ext_supersync.c`):
//! an LMK5C33216 clock synchronizer, loaded with libusdr's fixed configuration once
//! `rsl-sdr-frontend` has found it.

use rsl_deps::tracing::debug;
use rsl_sdr_frontend::supersync::LMK5C33216;

use crate::chips::lmk5c33216::Lmk5c33216;
use crate::error::Error;
use crate::lowlevel::Bus;

/// Loads the clock synchronizer of a board `rsl-sdr-frontend` found with libusdr's
/// register load (`board_ext_supersync_init` after its ID check).
///
/// # Errors
///
/// Any bus failure.
pub(super) fn bring_up(bus: &mut dyn Bus) -> Result<(), Error> {
    Lmk5c33216::at(LMK5C33216).load(bus)?;
    debug!(board = "supersync", "front end initialized");
    Ok(())
}
