//! The DC-offset calibration engines (datasheet `DC_REG`/`DC_CAL`/`DC_OP` and their
//! counterparts): the top-level LPF tuning engine (0x00-0x03), the TX LPF's (0x30-0x33),
//! the RX LPF's (0x50-0x53) and RXVGA2's (0x60-0x63). The four share one map, so each
//! register here is indexed by [`DcBlock`].
//!
//! An engine nulls the DC offset of one stage, chosen by its channel address: libusdr
//! starts it, polls until it finishes, and reads back the value it settled on. A value of
//! 0x1F is saturated, and libusdr restarts from 0.

use bnb::{BitEnum, bitfield, u3, u6};

use super::spi::BlockReg;
use crate::chips::register::IndexedRegister;

/// Calibration-engine register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Top-level `DC_REG`; see [`DcValue`].
    LpfValue = 0x00,

    /// Top-level `DC_CAL`; see [`DcStatus`].
    LpfStatus = 0x01,

    /// Top-level `DC_OP`; see [`DcControl`].
    LpfControl = 0x03,

    /// TX LPF `DC_REG_VAL`.
    TxLpfValue = 0x30,

    /// TX LPF `DC_REG` (status).
    TxLpfStatus = 0x31,

    /// TX LPF `DC_CALIB`.
    TxLpfControl = 0x33,

    /// RX LPF `DC_REG_VAL`.
    RxLpfValue = 0x50,

    /// RX LPF `DC_REG` (status).
    RxLpfStatus = 0x51,

    /// RX LPF `DC_CALIB`.
    RxLpfControl = 0x53,

    /// RXVGA2 `DC_REG_VAL`.
    RxVga2Value = 0x60,

    /// RXVGA2 `DC_REG` (status).
    RxVga2Status = 0x61,

    /// RXVGA2 `DC_CALIB`.
    RxVga2Control = 0x63,
}

impl BlockReg for Reg {}

/// Which calibration engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DcBlock {
    /// The top-level LPF tuning engine (channel 0 only).
    Lpf,

    /// The TX LPF's engine (channels 0 and 1: I and Q).
    TxLpf,

    /// The RX LPF's engine (channels 0 and 1: I and Q).
    RxLpf,

    /// RXVGA2's engine (channel 0 the reference, 1 and 2 VGA2A I and Q, 3 and 4 VGA2B I
    /// and Q, by libusdr's use).
    RxVga2,
}

/// The engine's value for the selected channel: its result once calibrated, or a start
/// value written before. Datasheet `DC_REG`/`DC_REG_VAL`, bits 5:0 (the YAML gives 7:0
/// for the LPF and VGA2 blocks; libusdr writes and reads at most five bits).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DcValue {
    /// The value. Bits 5:0.
    #[bits(0..=5)]
    value: u6,
}

/// The engine's state. Datasheet `DC_CAL` (top level) or `DC_REG` (others), read-only.
///
/// The top-level register also holds the RC time constant the LPF tuning measured
/// (`RC_LPF`, bits 7:5), which the other blocks lack, so it has no field here; libusdr only
/// logs it.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DcStatus {
    /// Lock pattern: locked unless 000 or 111. `LOCK`, bits 4:2.
    #[bits(2..=4)]
    lock: u3,

    /// The calibration is still running. `CLBR_DONE`/`CALIBR_DONE`, bit 1 (1 means in
    /// progress, despite the name).
    #[bits(1..=1)]
    running: bool,

    /// The selected channel's comparator says count up. `UD`, bit 0.
    #[bits(0..=0)]
    count_up: bool,
}

/// Starts or loads the selected channel. Datasheet `DC_OP`/`DC_CALIB`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DcControl {
    /// Start a calibration of the selected channel. `START_CLBR`/`START`, bit 5.
    #[bits(5..=5)]
    start: bool,

    /// Load the value from `DC_CNTVAL` into the selected channel. `LOAD`, bit 4.
    #[bits(4..=4)]
    load: bool,

    /// Release the engines from reset; clear resets them all. `SRESET`, bit 3 (active
    /// low); reset 1.
    #[bits(3..=3)]
    released: bool,

    /// The channel. `ADDR`, bits 2:0.
    #[bits(0..=2)]
    channel: u3,
}

/// Implements [`IndexedRegister`] over [`DcBlock`] for a register every engine has.
macro_rules! per_block {
    ($ty:ty, $lpf:ident, $tx:ident, $rx:ident, $vga2:ident) => {
        impl IndexedRegister for $ty {
            type Map = Reg;
            type Index = DcBlock;

            fn addr(block: DcBlock) -> Reg {
                match block {
                    DcBlock::Lpf => Reg::$lpf,
                    DcBlock::TxLpf => Reg::$tx,
                    DcBlock::RxLpf => Reg::$rx,
                    DcBlock::RxVga2 => Reg::$vga2,
                }
            }
        }
    };
}
per_block!(DcValue, LpfValue, TxLpfValue, RxLpfValue, RxVga2Value);
per_block!(DcStatus, LpfStatus, TxLpfStatus, RxLpfStatus, RxVga2Status);
per_block!(
    DcControl,
    LpfControl,
    TxLpfControl,
    RxLpfControl,
    RxVga2Control
);
