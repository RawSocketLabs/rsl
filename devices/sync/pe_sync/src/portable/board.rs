//! The `PE_SYNC` board: where each chip is wired and what libusdr's `pe_sync_configure`
//! asks of it (`device/pe_sync/pe_sync.c`).
//!
//! The clock tree: the LMK05318B, from a 12.8 MHz XO, makes 125 MHz (outputs 0, 1),
//! 150 MHz (2, 3), 10 MHz and 1 Hz; outputs 2 and 3 feed the two LMX2820s. LMX2820 #0's
//! 1 GHz feeds the LMX1204 (clock distribution, multiplied to 4 GHz), LMX2820 #1's 550 MHz
//! the LMX1214 (LO distribution). The two LMK1D1208I fan out the lower clocks.

use hal::{I2cAddr, SpiAddr};
use lmk05318::plan::{OutputFormat, OutputRequest, Xo, XoInput};

use super::lmk1d1208i::{Bank, Config};
use super::lmx1204::plan::{Channel, Request as Lmx1204Request};
use super::lmx1204::reg::{Format, SysrefModeCode};
use super::lmx1214::plan::{Aux, Request as Lmx1214Request};
use super::lmx1214::reg::AuxFormat;
use super::lmx2820::plan::Request as Lmx2820Request;
use super::lmx2820::reg::MashOrder;

/// `LMXGen0`, the first LMX2820: SPI bus 2 (`SPI_LMX2820_0`; 24-bit words at 40 MHz).
pub(crate) const LMX2820_0: SpiAddr = SpiAddr(2);

/// `LMXGen1`, the second LMX2820: SPI bus 3 (`SPI_LMX2820_1`).
pub(crate) const LMX2820_1: SpiAddr = SpiAddr(3);

/// `ClockDistrib`, the LMX1204: SPI bus 4 (`SPI_LMX1204`; 24-bit words at 2 MHz).
pub(crate) const LMX1204: SpiAddr = SpiAddr(4);

/// `LoDistrib`, the LMX1214: SPI bus 5 (`SPI_LMX1214`).
pub(crate) const LMX1214: SpiAddr = SpiAddr(5);

/// The LMK05318B: I²C core 0, bus 0, 0x64 (`I2C_BUS_LMK05318B`).
pub(crate) const LMK05318: I2cAddr = I2cAddr::on_core(0, 0, 0x64);

/// The `LCK` LMK1D1208I fan-out buffer: core 0, bus 1, 0x68 (`I2C_BUS_LMK1D1208I_LCK`).
pub(crate) const LMK1D1208I_LCK: I2cAddr = I2cAddr::on_core(0, 1, 0x68);

/// The `LRF` LMK1D1208I: core 0, bus 1, 0x69 (`I2C_BUS_LMK1D1208I_LRF`).
pub(crate) const LMK1D1208I_LRF: I2cAddr = I2cAddr::on_core(0, 1, 0x69);

/// The LP87524 PMIC: I²C core 1, bus 0, 0x60 (`I2C_BUS_LP87524`).
pub(crate) const LP87524: I2cAddr = I2cAddr::on_core(1, 0, 0x60);

/// The LP87524 register `pe_sync_check_status` reads and logs as `LP87524_OTP`. The
/// vendored YAML documents only `BUCK1_CTRL1` (0x04), so this stays a raw address.
pub(crate) const LP87524_OTP: u8 = 0x01;

/// The LMK05318B's XO: 12.8 MHz CMOS, frequency detector not bypassed
/// (`lmk05318_create(.., 12800000, XO_CMOS, false, ..)`).
pub(crate) const LMK05318_XO: Xo = Xo {
    hz: 12_800_000,
    input: XoInput::Cmos,
    detector_bypass: false,
};

/// The LMK05318B's outputs (`lmk_freq`, `lmk05318_port_request` with either APLL): 125 MHz
/// LVDS on 0 and 1; 150 MHz LVDS on 2 and 3; 156.25 MHz requested but off on 4 and 5; 10 MHz
/// and 1 Hz CMOS (P in phase, N inverted) on 6 and 7.
pub(crate) const LMK05318_OUTPUTS: [OutputRequest; 8] = [
    OutputRequest::new(0, 125_000_000, OutputFormat::Lvds),
    OutputRequest::new(1, 125_000_000, OutputFormat::Lvds),
    OutputRequest::new(2, LMX2820_REFERENCE_HZ, OutputFormat::Lvds),
    OutputRequest::new(3, LMX2820_REFERENCE_HZ, OutputFormat::Lvds),
    OutputRequest::new(4, 156_250_000, OutputFormat::Off),
    OutputRequest::new(5, 156_250_000, OutputFormat::Off),
    OutputRequest::new(6, 10_000_000, OutputFormat::LvcmosPN),
    OutputRequest::new(7, 1, OutputFormat::LvcmosPN),
];

/// LMK05318B outputs 2 and 3, the LMX2820s' references, in Hz.
const LMX2820_REFERENCE_HZ: u32 = 150_000_000;

/// LMX2820 #0 (`lmx0_freq`): 1 GHz on both outputs, second-order modulator, no forced
/// multiplier; SYSREF off.
pub(crate) const LMX2820_0_TUNE: Lmx2820Request = Lmx2820Request {
    reference_hz: LMX2820_REFERENCE_HZ as u64,
    mash: MashOrder::Second,
    multiplier: None,
    out_a_hz: LMX2820_0_OUT_HZ,
    out_b_hz: LMX2820_0_OUT_HZ,
};

/// LMX2820 #0's outputs, in Hz; output B feeds the LMX1204.
const LMX2820_0_OUT_HZ: u64 = 1_000_000_000;

/// LMX2820 #1 (`lmx1_freq`): 550 MHz on both outputs; otherwise as #0.
pub(crate) const LMX2820_1_TUNE: Lmx2820Request = Lmx2820Request {
    out_a_hz: LMX2820_1_OUT_HZ,
    out_b_hz: LMX2820_1_OUT_HZ,
    ..LMX2820_0_TUNE
};

/// LMX2820 #1's outputs, in Hz; they feed the LMX1214.
const LMX2820_1_OUT_HZ: u64 = 550_000_000;

/// The LMX1214: LMX2820 #1's 550 MHz buffered to all four outputs and, in LVDS, to the
/// auxiliary output.
pub(crate) const LMX1214_SETUP: Lmx1214Request = Lmx1214Request {
    input_hz: LMX2820_1_OUT_HZ,
    output_hz: LMX2820_1_OUT_HZ,
    outputs: [true; 4],
    aux: Aux {
        enabled: true,
        hz: LMX2820_1_OUT_HZ,
        format: AuxFormat::Lvds,
    },
};

/// Every LMX1204 channel output on.
const ALL: Channel = Channel {
    enabled: true,
    clock: true,
    sysref: true,
};

/// The LMX1204: LMX2820 #0's output B (1 GHz) multiplied by 4; continuous SYSREF at
/// 3.125 MHz, every clock and SYSREF output on; a 125 MHz logic clock and logic SYSREF in
/// LVDS. Its SYSREF request input is set to LMX2820 #0's planned 25 MHz SYSREF output,
/// though that synthesizer's SYSREF stays off; only repeater mode would use it.
pub(crate) const LMX1204_SETUP: Lmx1204Request = Lmx1204Request {
    clkin_hz: LMX2820_0_OUT_HZ,
    sysref_request_hz: 25_000_000,
    filter: false,
    sysref_mode: SysrefModeCode::Continuous,
    sysref: true,
    channels: [ALL; 5],
    clkout_hz: LMX2820_0_OUT_HZ * 4,
    sysref_out_hz: 3_125_000,
    logic_hz: 125_000_000,
    logic_clock_format: Format::Lvds,
    logic_sysref_format: Format::Lvds,
};

/// Both LMK1D1208Is (`lmk1d_cfg`): IN0 on and IN1 off, both banks from IN0 unmuted, all
/// eight outputs on in standard LVDS.
pub(crate) const LMK1D1208I_SETUP: Config = Config {
    inputs: [true, false],
    banks: [Bank {
        from_in0: true,
        muted: false,
    }; 2],
    outputs: 0xff,
    boosted: 0,
};
