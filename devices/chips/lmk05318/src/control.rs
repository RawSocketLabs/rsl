//! Device control: reset, output synchronization, start-up mode, the latched loss-of-lock
//! and loss-of-signal flags, the interrupt output and the DPLL's general control.

use super::reg::{Lmk05318Register, Reg};
use bnb::bitfield;

/// Device-wide control. `DEV_CTL`, 0x0C.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceControl {
    /// Software reset of every function, while set. `RESET_SW`, bit 7.
    #[bits(7..=7)]
    reset: bool,

    /// Output synchronization (SYNC), asserted while set. `SYNC_SW`, bit 6.
    #[bits(6..=6)]
    sync: bool,

    /// Synchronize the outputs automatically once the DPLL locks. `SYNC_AUTO_DPLL`, bit 5.
    #[bits(5..=5)]
    sync_after_dpll_lock: bool,

    /// Synchronize the outputs automatically once the PLLs lock. `SYNC_AUTO_APLL`, bit 4.
    #[bits(4..=4)]
    sync_after_apll_lock: bool,

    /// Mute every output during a SYNC event. `SYNC_MUTE`, bit 3.
    #[bits(3..=3)]
    mute_during_sync: bool,

    /// Cascaded start-up: APLL1 held at a centre value while APLL2 locks, then locks
    /// itself. `PLLSTRTMODE`, bit 1.
    #[bits(1..=1)]
    cascaded_startup: bool,

    /// Start the PLLs and outputs automatically after a device reset. `AUTOSTRT`, bit 0.
    #[bits(0..=0)]
    autostart: bool,
}
impl Lmk05318Register for DeviceControl {
    const ADDR: Reg = Reg::DeviceControl;
}

/// The latched XO and APLL loss flags; each sets on an edge of its source and clears when
/// written 0. `INT_FLAG0`, 0x13.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LossFlags {
    /// The XO frequency detector lost its source. `LOS_FDET_XO_INTR`, bit 4.
    #[bits(4..=4)]
    xo_frequency_lost: bool,

    /// APLL2 lost lock. `LOL_PLL2_INTR`, bit 3.
    #[bits(3..=3)]
    apll2_unlocked: bool,

    /// APLL1 lost lock. `LOL_PLL1_INTR`, bit 2.
    #[bits(2..=2)]
    apll1_unlocked: bool,

    /// The XO was lost. `LOS_XO_INTR`, bit 0.
    #[bits(0..=0)]
    xo_lost: bool,
}
impl Lmk05318Register for LossFlags {
    const ADDR: Reg = Reg::LossFlags;
}

/// The latched DPLL flags; each sets on an edge of its source and clears when written 0.
/// `INT_FLAG1`, 0x14.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpllFlags {
    /// The DPLL lost phase lock. `LOPL_DPLL_INTR`, bit 7.
    #[bits(7..=7)]
    phase_lock_lost: bool,

    /// The DPLL lost frequency lock. `LOFL_DPLL_INTR`, bit 6.
    #[bits(6..=6)]
    frequency_lock_lost: bool,

    /// The tuning-word history updated. `HIST_INTR`, bit 5.
    #[bits(5..=5)]
    history_updated: bool,

    /// The DPLL entered holdover. `HLDOVR_INTR`, bit 4.
    #[bits(4..=4)]
    holdover: bool,

    /// The DPLL switched reference. `REFSWITCH_INTR`, bit 3.
    #[bits(3..=3)]
    reference_switched: bool,

    /// The active reference lost clocks. `LOR_MISSCLK_INTR`, bit 2.
    #[bits(2..=2)]
    reference_missing_clock: bool,

    /// The active reference's frequency went out of range. `LOR_FREQ_INTR`, bit 1.
    #[bits(1..=1)]
    reference_frequency_lost: bool,

    /// The active reference's amplitude went out of range. `LOR_AMP_INTR`, bit 0.
    #[bits(0..=0)]
    reference_amplitude_lost: bool,
}
impl Lmk05318Register for DpllFlags {
    const ADDR: Reg = Reg::DpllFlags;
}

/// The interrupt output. `INTCTL`, 0x15.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterruptControl {
    /// Raise the interrupt only when every unmasked flag is set, rather than any.
    /// `INT_AND_OR`, bit 1.
    #[bits(1..=1)]
    all_flags: bool,

    /// Interrupt output enabled. `INT_EN`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}
impl Lmk05318Register for InterruptControl {
    const ADDR: Reg = Reg::InterruptControl;
}

/// The DPLL's general control. `DPLL_GEN_CTL`, 0xFC.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpllControl {
    /// Zero-delay synchronization. `DPLL_ZDM_SYNC_EN`, bit 7.
    #[bits(7..=7)]
    zero_delay_sync: bool,

    /// No feedback-divider reset in zero-delay mode. `DPLL_ZDM_NDIV_RST_DIS`, bit 6.
    #[bits(6..=6)]
    zero_delay_keeps_divider: bool,

    /// The switchover timer. `DPLL_SWITCHOVER_1`, bit 5.
    #[bits(5..=5)]
    switchover_timer: bool,

    /// Fast lock, always. `DPLL_FASTLOCK_ALWAYS`, bit 4.
    #[bits(4..=4)]
    fast_lock: bool,

    /// Frequency lock detection. `DPLL_LOCKDET_PPM_EN`, bit 3.
    #[bits(3..=3)]
    frequency_lock_detect: bool,

    /// In holdover without tuning-word history, hold the last control value rather than
    /// run free. `DPLL_HLDOVR_MODE`, bit 2.
    #[bits(2..=2)]
    holdover_holds: bool,

    /// The DPLL is on. `DPLL_LOOP_EN`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}
impl Lmk05318Register for DpllControl {
    const ADDR: Reg = Reg::DpllControl;
}
