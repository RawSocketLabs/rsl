//! The XSDR's wiring and setpoints, as the pinned libusdr's `_xsdr_init_revx` and
//! `xsdr_pwren` choose them (`device/m2_lm7_1/xsdr_ctrl.c`).

use rsl_lms7002m::ResetPlan;
use rsl_lowlevel::{I2cAddr, SpiAddr};
use rsl_lp8758::{Buck, BuckControl, BuckVoltage};

use crate::portable::supplies::SupplyPlan;

/// The FPGA-side LP8758: `I2C_BUS_LP8758_FPGA`, bus 0, `I2C_DEV_PMIC_FPGA` 0x60.
const PMIC: I2cAddr = I2cAddr::new(0, 0x60);

/// The XSDR's supplies.
///
/// Rails, as libusdr requests them: channel 1 at 1.48 V ("TODO check if we need this
/// rail"), channel 2 at 1.28 V ("LMS Vcore boost to 1.25V"), channel 3 at 1.85 V. The
/// driver writes the codes libusdr's `s_get_vout` produces and names each for the voltage
/// SNVSAC6B's table gives that code: 1480 mV is code 0xA1, 1.48 V; 1280 mV becomes 0x86,
/// 1.285 V (libusdr sits one code high there); 1850 mV becomes 0xB3, 1.84 V (20 mV steps,
/// rounded down).
///
/// Every channel is enabled in forced PWM: `lp8758_vout_ctrl(.., 1, 1)`, byte 0x8A.
pub(crate) const XSDR_SUPPLIES: SupplyPlan = SupplyPlan {
    pmic: PMIC,
    rails: [
        (Buck::B1, BuckVoltage::from_millivolts(1480)),
        // libusdr requests 1280 mV.
        (Buck::B2, BuckVoltage::from_millivolts(1285)),
        // libusdr requests 1850 mV.
        (Buck::B3, BuckVoltage::from_millivolts(1840)),
    ],
    control: BuckControl::new()
        .with_enabled(true)
        .with_discharge_when_off(true)
        .with_forced_pwm(true),
};

/// The LMS7002M's reference: the internal 26 MHz clock `_xsdr_init_revx` selects
/// (`XSDR_INT_REFCLK`, `lms7002m_init`'s `fref`).
pub(crate) const REFERENCE_HZ: u32 = 26_000_000;

/// The XSDR's LMS7002M: `SPI_LMS7` (bus 0); `xsdr_pwren` passes `new_rev` boards an LDO
/// mask of 0 and `txrx_clk` = 1 to `lms7002m_create`.
pub(crate) const XSDR_LMS: ResetPlan = ResetPlan {
    target: SpiAddr(0),
    ldo_mask: 0,
    shared_reference: true,
};
