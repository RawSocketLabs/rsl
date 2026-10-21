//! The LMX1204's SYSREF delay reload and windowing as `hw/lmx1204/lmx1204.c` runs them:
//! `lmx1204_reload_sysrefout_ch_delay`, `lmx1204_sysref_windowing_beforesync`, `_capture`
//! and `_aftersync`, each on the plan the solver left.
//!
//! The windowing centres SYSREFREQ between two CLKIN edges: arm the capture
//! ([`before_sync`]), let a SYSREFREQ edge arrive, read where it fell and delay it to the
//! middle ([`capture`]), then disarm ([`after_sync`]). No libusdr device calls these;
//! `PE_SYNC` has no control for them (`PORTING.md`).

use hal::SpiAddr;

use super::plan::Plan;
use super::reg::{Capture as CaptureControl, Reg};
use super::sequence::NAME;
use crate::portable::chain::Then;
use crate::portable::script::Script;
use crate::portable::ti_spi::{self, Modify, Register};
use crate::portable::ti_sync::Capture;

/// `lmx1204_reload_sysrefout_ch_delay`: R22 down to R17 rewritten with the plan's
/// delays ([`Plan::with_delays`]).
pub(crate) fn reload_delays(target: SpiAddr, plan: &Plan) -> Script {
    let mut script = Script::new();
    for entry in plan.delay_registers() {
        script = script.then(NAME, ti_spi::write(target, entry));
    }
    script
}

/// `SYSREFREQ_CLR`'s bit in R15.
const SYSREF_REQUEST_CLEAR: u16 = 1;

/// `lmx1204_sysref_windowing_beforesync`: the dividers' synchronization on, the
/// clock-position capture armed on the SYSREFREQ pins and the delay step set for CLKIN;
/// then `SYSREFREQ_CLR` pulsed (R15 read, then written with it set and cleared).
pub(crate) fn before_sync(target: SpiAddr, plan: &Plan) -> Then<Script, Modify> {
    let write = |entry| ti_spi::write(target, entry);
    let arm = Script::new()
        .then(NAME, write(plan.logic_divider(true)))
        .then(NAME, write(capture_control(true)))
        .then(NAME, write(plan.delay_step()));
    Then::new(
        arm,
        Modify::pulse(NAME, target, Reg::Sysref as u8, SYSREF_REQUEST_CLEAR),
    )
}

/// `lmx1204_sysref_windowing_capture`: the clock positions read, then SYSREFREQ delayed
/// to centre the captured edge (R15 as the plan sets it).
pub(crate) fn capture(target: SpiAddr, plan: &Plan) -> Capture {
    let (_, base) = plan.sysref(0);
    Capture::new(NAME, target, base)
}

/// `lmx1204_sysref_windowing_aftersync`: the dividers' synchronization off and the
/// capture disarmed.
pub(crate) fn after_sync(target: SpiAddr, plan: &Plan) -> Script {
    Script::new()
        .then(NAME, ti_spi::write(target, plan.logic_divider(false)))
        .then(NAME, ti_spi::write(target, capture_control(false)))
}

/// R14 with the capture armed or not, SYSREFREQ from its pins.
fn capture_control(enabled: bool) -> (u8, u16) {
    CaptureControl::new()
        .with_enabled(enabled)
        .with_request_pins(true)
        .entry()
}
