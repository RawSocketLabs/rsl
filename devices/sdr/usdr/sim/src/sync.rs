//! The FPGA's stream sync unit: register 14 with bit 31 set, its mode (`ST_*`) in bits
//! 19:16, as libusdr's `sfetrx4_stream_sync` writes it. Writes with bit 31 clear are the TX
//! core's commands (see `tx_stream.rs`) and leave the mode alone.
//!
//! Assumed, from the C that writes it (no gateware source is vendored):
//! - Bit 31 selects the sync unit. Evidence: every `sfetrx4_stream_sync` write sets it;
//!   `sfe_tx4_ctl` and `sfe_tx4_upd`, handed the same register as the TX stream's "sync
//!   base", never do (their fields end at bit 11); and libusdr's clients start TX, then
//!   sync (`usdr_dm_create`, the JSON controller), so sync writes, whose low bits read as
//!   the TX core's stop command, cannot be TX commands.
//! - What releases the streams. Free-run releases them at once. The activity modes
//!   release them once that activity runs: RX's DMA and front end (`ST_ADCACT`), the TX
//!   core started (`ST_DACACT`), either (`ST_ADCORDACACT`), or both (`ST_ADCANDDACACT`,
//!   and `ST_DELAYADCDAC`, libusdr's `"all"` for RX with TX, whose delay is not
//!   modelled). Stop, 1PPS (`ST_ONEPPS`; the sim raises no pulse) and generated SYSREF
//!   hold them.

/// The register the sync unit shares with the TX core's commands.
pub(crate) const REG_SYNC: u32 = 14;
/// Bit 31 of a [`REG_SYNC`] write selects the sync unit.
pub(crate) const SYNC_SELECT: u32 = 1 << 31;

/// The sync unit's mode.
#[derive(Debug, Default)]
pub(crate) struct SyncUnit {
    /// The last mode written (`ST_*`); 0, stop, from reset.
    mode: u32,
}

impl SyncUnit {
    /// Follows a register write.
    pub(crate) fn on_write(&mut self, addr: u32, value: u32) {
        if addr == REG_SYNC && value & SYNC_SELECT != 0 {
            self.mode = (value >> 16) & 0xf;
        }
    }

    /// Whether the streams may run, with RX running if `rx` and the TX core started if
    /// `tx`.
    pub(crate) const fn releases(&self, rx: bool, tx: bool) -> bool {
        match self.mode {
            // `ST_FREERUN`.
            7 => true,
            // `ST_ADCACT`.
            2 => rx,
            // `ST_DACACT`.
            3 => tx,
            // `ST_ADCANDDACACT`, `ST_DELAYADCDAC`.
            4 | 6 => rx && tx,
            // `ST_ADCORDACACT`.
            5 => rx || tx,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sync unit after a `sfetrx4_stream_sync` write of `mode`.
    fn synced(mode: u32) -> SyncUnit {
        let mut sync = SyncUnit::default();
        sync.on_write(REG_SYNC, SYNC_SELECT | mode << 16);
        sync
    }

    #[test]
    fn activity_modes_wait_for_their_activity() {
        assert!(synced(7).releases(false, false), "free-run");
        assert!(!synced(2).releases(false, true) && synced(2).releases(true, false));
        assert!(!synced(3).releases(true, false) && synced(3).releases(false, true));
        assert!(!synced(6).releases(true, false) && synced(6).releases(true, true));
        assert!(synced(5).releases(false, true) && !synced(5).releases(false, false));
    }

    #[test]
    fn stop_and_1pps_hold_the_streams() {
        assert!(!SyncUnit::default().releases(true, true), "reset is stop");
        assert!(!synced(1).releases(true, true), "no 1PPS pulse is raised");
        assert!(!synced(9).releases(true, true), "nor a generated SYSREF");
    }

    #[test]
    fn tx_commands_leave_the_mode_alone() {
        let mut sync = synced(7);
        // `sfe_tx4_ctl`: stop, reset buffers, start with lane B muted.
        for command in [0, 0x80, 0x103] {
            sync.on_write(REG_SYNC, command);
        }
        assert!(sync.releases(false, false), "still free-running");
    }
}
