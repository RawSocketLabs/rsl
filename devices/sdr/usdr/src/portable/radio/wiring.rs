//! Where the uSDR wires the chips its RF sequences reach, and the setpoints they write.

use super::super::address::I2cAddr;
use super::super::lp8758::BuckVoltage;
use super::super::si5332::LvpeclOutput;

/// The LP8758 PMIC: FPGA I2C bus 0, the LP8758-E0's address 0x60 (SNVSAC6B;
/// `I2C_DEV_PMIC_FPGA` in libusdr).
pub(crate) const PMIC: I2cAddr = I2cAddr::new(0, 0x60);

/// The Si5332 clock generator: FPGA I2C bus 0, address 0x6A (`I2C_DEV_CLKGEN` in libusdr).
pub(crate) const CLOCK: I2cAddr = I2cAddr::new(0, 0x6a);

/// The mixer LO is the Si5332 VCO divided by this (`si_vco_div`, set in `usdr_init`).
pub(crate) const MIXER_LO_DIVIDER: u8 = 8;

/// LMS6002D digital I/O rail (PMIC buck 3) at normal sample rates: 1.8 V
/// (`LMS6_VIO_NORM`).
pub(crate) const LMS_VIO_NORMAL: BuckVoltage = BuckVoltage::from_millivolts(1800);

/// LMS6002D digital I/O rail at ADC rates of 62 MS/s and above: 1.925 V
/// (`LMS6_VIO_BOOST`), "to get stable samplerates over 60Msps" in libusdr's words.
pub(crate) const LMS_VIO_BOOST: BuckVoltage = BuckVoltage::from_millivolts(1925);

/// Which of the Si5332's outputs 0 and 1 carries the LMS6002D's PLL reference (LVPECL);
/// the other is the RX sample clock. Revision 3 swapped them.
pub(crate) const fn lvpecl(rev3: bool) -> LvpeclOutput {
    if rev3 {
        LvpeclOutput::Out0
    } else {
        LvpeclOutput::Out1
    }
}
