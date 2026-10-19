//! The four-channel 400–7200 MHz front end's identification (libusdr's `fe4ch4007200`,
//! `device/ext_fe_ch4_400_7200/ext_fe_ch4_400_7200.c`), built for the sSDR and dSDR.
//!
//! libusdr applies no compatibility filter, so its probe tries this board on every
//! device, last. The board's chips sit on FPGA I2C core 1 (buses 0 and 1): a TMP114
//! sensor and four TCA6424A expanders. libusdr reads the sensor's ID and each expander's
//! direction registers, and takes a wrong ID for an absent board. A device with one I2C
//! core refuses the first of those transfers in its transport (the uSDR's do, with
//! `-EINVAL`), which libusdr reports as a failed board.
//!
//! What follows an identified sensor (the expanders' test vectors, their outputs, the
//! DAC and the channel set-up) is not ported: no device this crate serves can reach it.
//! Every one declares a single I2C core (`DNLL_I2C_COUNT` 1 on the uSDR, the XSDR's
//! `rev_generic` and the `LSDr`); only the sSDR (`rev_advanced`, not ported) declares two.
//! A sensor that answers anyway stops the probe ([`Error::NotPorted`]).

use hal::{Bus, I2cAddr};

use crate::chips::tmp114::Tmp114;
use crate::error::{BusContext, Error};
use crate::gpio::{FrontEndGpio, GpioCommand, PinMode};

/// The connector setup: the third I2C bus's clock, then the fourth's data and clock, to
/// the core's second alternate function (`GPIO_I2C3_SCL`, `GPIO_I2C4_SDA`,
/// `GPIO_I2C4_SCL`).
const SETUP: [GpioCommand; 3] = [
    GpioCommand::Configure(6, PinMode::Alternate1),
    GpioCommand::Configure(1, PinMode::Alternate1),
    GpioCommand::Configure(5, PinMode::Alternate1),
];

/// The temperature sensor U69, a TMP114NB on core 1, bus 0 (`I2C_TEMP_U69`).
const SENSOR: I2cAddr = I2cAddr::on_core(1, 0, 0x4e);

/// The four expanders: U114 and U110 on bus 0, U300 and U301 on bus 1, each at the
/// TCA6424A's low and high address (`I2C_TCA6424AR_U114`, `_U110`, `_U300`, `_U301`).
const EXPANDERS: [I2cAddr; 4] = [
    I2cAddr::on_core(1, 0, 0x22),
    I2cAddr::on_core(1, 0, 0x23),
    I2cAddr::on_core(1, 1, 0x22),
    I2cAddr::on_core(1, 1, 0x23),
];

/// The TMP114's `Device_ID` register (0x0B), 16 bits, MSB first.
const SENSOR_ID_REG: u8 = 0x0b;

/// The TCA6424A's first direction register (`TCA6424_CFG0`).
const EXPANDER_CONFIG0: u8 = 12;

/// The TCA6424A's auto-increment flag, set in the register byte of a multi-register
/// access (`tca6424a_reg16_get`'s `0x80 | reg`).
const AUTO_INCREMENT: u8 = 0x80;

/// Looks for the board (`ext_fe_ch4_400_7200_init` up to its sensor check): configures
/// the connector's pins, reads the sensor's ID and every expander's first two direction
/// registers, then compares the ID.
///
/// # Errors
///
/// Any bus failure, a device refusing I2C core 1 included.
pub(crate) fn detect(bus: &mut dyn Bus) -> Result<bool, Error> {
    FrontEndGpio::run(bus, &SETUP)?;
    let mut id = [0; 2];
    bus.i2c(SENSOR, &[SENSOR_ID_REG], &mut id)
        .reading("TMP114", SENSOR_ID_REG)?;
    // libusdr logs these; nothing else reads them.
    for expander in EXPANDERS {
        bus.i2c(expander, &[AUTO_INCREMENT | EXPANDER_CONFIG0], &mut [0; 2])
            .reading("TCA6424A", EXPANDER_CONFIG0)?;
    }
    Ok(u16::from_be_bytes(id) == Tmp114::DEVICE_ID)
}
