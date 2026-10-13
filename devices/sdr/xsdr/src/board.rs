//! The XSDR's wiring and setpoints, as the pinned libusdr's `_xsdr_init_revx` and
//! `xsdr_pwren` choose them (`device/m2_lm7_1/xsdr_ctrl.c`).

use rsl_lowlevel::{I2cAddr, SpiAddr};
use rsl_lp8758::{Buck, BuckControl, BuckVoltage};

use crate::portable::lms7002m::ResetPlan;
use crate::portable::supplies::SupplyPlan;

/// The FPGA-side LP8758: `I2C_BUS_LP8758_FPGA`, bus 0, `I2C_DEV_PMIC_FPGA` 0x60.
const PMIC: I2cAddr = I2cAddr::new(0, 0x60);

/// The XSDR's supplies.
///
/// Rails, as libusdr requests them: channel 1 at 1.48 V ("TODO check if we need this
/// rail"), channel 2 at 1.28 V ("LMS Vcore boost to 1.25V"), channel 3 at 1.85 V. libusdr's
/// `s_get_vout` turns its 1280 mV into code 0x86, which SNVSAC6B's table makes 1.285 V; the
/// driver writes that same code and names it for what it selects. The other two requests
/// fall where libusdr's codes and the datasheet's agree.
///
/// Every channel is enabled in forced PWM: `lp8758_vout_ctrl(.., 1, 1)`, byte 0x8A.
pub(crate) const XSDR_SUPPLIES: SupplyPlan = SupplyPlan {
    pmic: PMIC,
    rails: [
        (Buck::B1, BuckVoltage::from_millivolts(1480)),
        (Buck::B2, BuckVoltage::from_millivolts(1285)),
        (Buck::B3, BuckVoltage::from_millivolts(1850)),
    ],
    control: BuckControl::new()
        .with_enabled(true)
        .with_discharge_when_off(true)
        .with_forced_pwm(true),
};

/// The XSDR's LMS7002M: `SPI_LMS7` (bus 0); `xsdr_pwren` passes `new_rev` boards an LDO
/// mask of 0 and `txrx_clk` = 1 to `lms7002m_create`.
pub(crate) const XSDR_LMS: ResetPlan = ResetPlan {
    target: SpiAddr(0),
    ldo_mask: 0,
    shared_reference: true,
};
