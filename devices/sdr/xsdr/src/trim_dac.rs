//! The XTRX's reference-trim DAC, which steers its reference oscillator: a DAC80501 on
//! revision 5 boards, an MCP4725 on revision 4 (`_xsdr_init_revo`'s end and
//! `xsdr_trim_dac_vctcxo`, `device/m2_lm7_1/xsdr_ctrl.c`).
//!
//! libusdr drives both with its own raw transfers there (`_dac_x0501_set_reg`,
//! `_dac_x0501_get_reg`, `_dac_mcp4725_set_vout`, `_dac_mcp4725_get_vout`), not the
//! `DACx0501` driver the front-end boards use, so this is its own small blocking code: the
//! transfers are wider than one register byte, which the portable sequences' I2C actions
//! cannot express.

use hal::I2cAddr;

use crate::error::{Access, Error};
use crate::lowlevel::Bus;

/// The DAC80501 (`I2C_BUS_DAC80501`: bus 0, `I2C_DEV_DAC80501M_A0_SCL`).
const DAC80501: I2cAddr = I2cAddr::new(0, 0x4b);
/// The MCP4725 (`I2C_BUS_DAC_MCP4725`: bus 0, 0x62).
const MCP4725: I2cAddr = I2cAddr::new(0, 0x62);
/// The DAC80501's name in errors.
const DAC80501_NAME: &str = "DAC80501";
/// The MCP4725's.
const MCP4725_NAME: &str = "MCP4725";

/// The DAC80501's identification register (`DEVID`).
const DEVID: u8 = 1;
/// Its gain register (`GAIN`).
const GAIN: u8 = 4;
/// Its output code (`DAC`).
const CODE: u8 = 8;

/// The IDs `_xsdr_init_revo` takes for a DAC80501.
const DAC80501_IDS: [u16; 3] = [0x0194, 0x0195, 0x2195];
/// The IDs it takes for no DAC80501, so a revision 4 board.
const REVISION_4_IDS: [u16; 2] = [0xbeef, 0xffff];
/// The gain libusdr writes ("TODO TCXO internal / external"): reference halved, buffer
/// gain 2.
const DAC80501_GAIN: u16 = 0x0101;
/// The DAC80501's code at open: `(3.0f / 2 * 1.1f * 65535) / 2.5f` in single precision,
/// 43253.1, truncated.
const DAC80501_MID_RANGE: u16 = 0xa8f5;
/// The MCP4725's code at open: `(1.099f / 2) * 65535`, 36011.5, truncated.
const MCP4725_MID_RANGE: u16 = 0x8cab;

/// Which trim DAC the board has (libusdr's `dac_old_r5`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrimDac {
    /// Revision 5: a DAC80501.
    Dac80501,

    /// Revision 4: an MCP4725.
    Mcp4725,
}

impl TrimDac {
    /// Identifies the DAC and sets it to mid-range, as `_xsdr_init_revo` ends: a DAC80501
    /// gets its gain and code; anything that reads as no DAC80501 is taken for a revision 4
    /// board, whose MCP4725 gets its code and is read back (the result only logged by
    /// libusdr).
    ///
    /// # Errors
    ///
    /// [`Error::ChipId`] for an ID that is neither (libusdr's `-EIO`); [`Error::Bus`].
    pub(crate) fn init(bus: &mut dyn Bus) -> Result<Self, Error> {
        let mut id = [0; 2];
        transfer(bus, DAC80501, DAC80501_NAME, &[DEVID], &mut id)?;
        let id = u16::from_be_bytes(id);
        if DAC80501_IDS.contains(&id) {
            write_dac80501(bus, GAIN, DAC80501_GAIN)?;
            write_dac80501(bus, CODE, DAC80501_MID_RANGE)?;
            return Ok(Self::Dac80501);
        }
        if !REVISION_4_IDS.contains(&id) {
            return Err(Error::ChipId {
                chip: DAC80501_NAME,
                expected: DAC80501_IDS[1].into(),
                found: id.into(),
            });
        }
        Self::Mcp4725.set(bus, MCP4725_MID_RANGE)?;
        // libusdr reads the configuration back and only logs it.
        transfer(bus, MCP4725, MCP4725_NAME, &[], &mut [0; 4])?;
        Ok(Self::Mcp4725)
    }

    /// Sets the DAC's code (`xsdr_trim_dac_vctcxo`): the DAC80501's output register, or the
    /// MCP4725's fast-mode write of the code's top 12 bits.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub(crate) fn set(self, bus: &mut dyn Bus, code: u16) -> Result<(), Error> {
        match self {
            Self::Dac80501 => write_dac80501(bus, CODE, code),
            Self::Mcp4725 => {
                let [high, low] = (code >> 4).to_be_bytes();
                transfer(bus, MCP4725, MCP4725_NAME, &[high & 0x0f, low], &mut [])
            }
        }
    }
}

/// Writes a DAC80501 register, MSB first.
fn write_dac80501(bus: &mut dyn Bus, reg: u8, value: u16) -> Result<(), Error> {
    let [high, low] = value.to_be_bytes();
    transfer(bus, DAC80501, DAC80501_NAME, &[reg, high, low], &mut [])
}

/// One I2C transfer, its failure naming the chip and its first byte (the register, for
/// the DAC80501).
fn transfer(
    bus: &mut dyn Bus,
    dev: I2cAddr,
    chip: &'static str,
    write: &[u8],
    read: &mut [u8],
) -> Result<(), Error> {
    bus.i2c(dev, write, read).map_err(|source| Error::Bus {
        chip,
        access: if read.is_empty() {
            Access::Write
        } else {
            Access::Read
        },
        reg: write.first().copied().unwrap_or(0).into(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mid_range_codes_are_libusdrs_single_precision_arithmetic() {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "libusdr's float-to-int conversions, reproduced"
        )]
        let (dac, mcp) = (
            ((3.0f32 / 2.0 * 1.1 * 65535.0) / 2.5) as u16,
            (1.099f32 / 2.0 * 65535.0) as u16,
        );
        assert_eq!((dac, mcp), (DAC80501_MID_RANGE, MCP4725_MID_RANGE));
    }
}
