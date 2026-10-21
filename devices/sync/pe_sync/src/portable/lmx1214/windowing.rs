//! The LMX1214's SYNC windowing as `hw/lmx1214/lmx1214.c` runs it
//! (`lmx1214_sysref_windowing_beforesync`, `_capture`, `_aftersync`), on the plan the solver
//! left: arm the clock-position capture ([`before_sync`]), let a SYNC edge arrive, read
//! where it fell and delay SYNC to the middle ([`capture`]), then disarm ([`after_sync`]).
//! No libusdr device calls these; `PE_SYNC` has no control for them (`PORTING.md`).

use bnb::u5;
use hal::SpiAddr;

use super::plan::{Plan, capture as capture_register};
use super::reg::{Reg, Sync};
use super::sequence::NAME;
use crate::portable::chain::Then;
use crate::portable::script::Script;
use crate::portable::ti_spi::{self, Modify};
use crate::portable::ti_sync::Capture;

/// `SYNC_CLR`'s bit in [`Sync`].
const SYNC_CLEAR: u16 = 1;

/// `lmx1214_sysref_windowing_beforesync`: the SYNC path on, the clock-position capture
/// armed and the delay step set for the plan's input; then `SYNC_CLR` pulsed (R15 read,
/// then written with it set and cleared).
pub(crate) fn before_sync(target: SpiAddr, plan: &Plan) -> Then<Script, Modify> {
    let write = |entry| ti_spi::write(target, entry);
    let arm = Script::new()
        .then(NAME, write(plan.aux_divider(true)))
        .then(NAME, write(capture_register(true)))
        .then(NAME, write(plan.sync_delay_step()));
    Then::new(
        arm,
        Modify::pulse(NAME, target, Reg::Sync as u8, SYNC_CLEAR),
    )
}

/// `lmx1214_sysref_windowing_capture`: the clock positions read, then SYNC delayed to
/// centre the captured edge (R15, its undisclosed bits at libusdr's 0x16).
pub(crate) fn capture(target: SpiAddr) -> Capture {
    let base = Sync::new().with_undisclosed(u5::new(0x16));
    Capture::new(NAME, target, base.into())
}

/// `lmx1214_sysref_windowing_aftersync`: the SYNC path off and the capture disarmed.
pub(crate) fn after_sync(target: SpiAddr, plan: &Plan) -> Script {
    Script::new()
        .then(NAME, ti_spi::write(target, plan.aux_divider(false)))
        .then(NAME, ti_spi::write(target, capture_register(false)))
}
