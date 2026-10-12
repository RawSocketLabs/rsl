//! The LMK05318B network synchronizer (`hw/lmk05318/lmk05318.c`): I2C core 0, bus 0,
//! address 0x64 (`I2C_BUS_LMK05318B` in `pe_sync.c`), with a 16-bit register pointer
//! (`lmk05318_reg_wr` sends `reg >> 8, reg`).
//!
//! The pinned `hw/lmk05318/lmk05318.yaml` documents fields but no reset values, so every
//! register resets to 0 except those below. Assumptions, each a sim choice:
//! - No reset or sync side effects: `DEV_CTL` (0x0c) `RESET_SW` and `SYNC_SW` are stored
//!   like any other bit. The interrupt flags (`INT_FLAG0`/`INT_FLAG1`, 0x13/0x14) are
//!   plain storage, so after libusdr clears them they report no loss of lock or signal.
//! - DPLL reference validation (`REFVALSTAT`) is not modelled: `pe_sync.c` leaves the DPLL
//!   disabled, so `lmk05318_wait_dpll_ref_stat` returns without reading it.
//! - `BAW_LOCKDET_PPM_MAX_BY1` (0x50) is plain storage. The YAML calls bit 7 `BAW_LOCK`
//!   "BAW Lock Detect Status", yet the C writes it (1 on the no-DPLL path, `lmk05318.c`
//!   `no_dpll_regs`; 0 on the DPLL path), and the YAML marks no field's mode. Stored, it
//!   reads back the 1 the no-DPLL configuration wrote, which `lmk05318_wait_apll1_lock`
//!   requires; whether hardware reports a live status there is open.

use super::i2c::{I2cRegisters, Reset};

/// The registers whose power-on contents are not 0, with their read-only masks. Read-only is
/// a sim choice for all of them: the YAML gives no access modes.
const RESET: [Reset; 6] = [
    // Device identity, read as one 4-byte burst from 0x0000. `lmk05318_create` refuses
    // anything else (`-ENODEV`); with the transport's packing (first byte most significant)
    // its `dummy[3..0]` check of 10 0b 35 42 is these bytes in address order.
    (0x0000, 0x10, 0xff),
    (0x0001, 0x0b, 0xff),
    (0x0002, 0x35, 0xff),
    (0x0003, 0x42, 0xff),
    // `PLL1_CALSTAT1` and `PLL2_CALSTAT1` (YAML page "Status"; the C never writes them): bit
    // 5 `PLLn_VM_INSIDE` reads in range, which the APLL1 and APLL2 lock waits require.
    (0x0165, 0x20, 0xff),
    (0x016f, 0x20, 0xff),
];

/// A healthy LMK05318B whose APLLs report lock.
///
/// Assumption: lock is reported from power-on, not modelled as acquired by calibration;
/// libusdr's lock waits time out by wall clock, and a reported lock ends each on its first
/// poll.
pub(crate) fn lmk05318() -> I2cRegisters {
    I2cRegisters::new(2, &RESET)
}
