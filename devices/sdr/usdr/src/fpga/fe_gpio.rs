//! The front-end connector's GPIO core (libusdr's `ipblks/gpio.c`): the M.2 connector's
//! general-purpose pins, which a front-end board uses for its I2C bus, 1PPS, UART and fast
//! control lines.
//!
//! One FPGA register takes every command; bits 31:30 say which. libusdr reaches the core
//! at register 3 on the uSDR (`gpiobase` in `device/device_fe.c`, hard-coded there with a
//! note to look it up from the board description instead).

use bnb::{BitEnum, bitfield, u2, u4, u15};

use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

/// The core's register (`gpiobase`).
const REG_FE_GPIO: u32 = 3;

/// What a write to the core does: bits 31:30 (`GPIO_CMD_OFF`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum Command {
    /// Drive the masked pins to the given levels (`GPIO_OUT`).
    Drive,

    /// Set the masked pins high (`GPIO_SET`).
    SetHigh,

    /// Set the masked pins low (`GPIO_CLR`).
    SetLow,

    /// Configure one pin (`GPIO_CFG_REG`).
    Configure,
}

/// How a pin is used (`gpio_config_vals`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(crate) enum PinMode {
    /// A plain input (`GPIO_CFG_IN`).
    Input,

    /// A plain output, driven by [`FrontEndGpio::drive`] (`GPIO_CFG_OUT`).
    Output,

    /// The pin's first alternate function, such as the external I2C bus, 1PPS or UART
    /// (`GPIO_CFG_ALT0`).
    Alternate0,

    /// The pin's second alternate function (`GPIO_CFG_ALT1`).
    Alternate1,
}

/// A configuration write: one pin's mode.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConfigureWrite {
    /// Always [`Command::Configure`].
    #[bits(30..=31)]
    command: Command,

    /// The pin, 0 to 14 (`GPIO_CFG_IDX_OFF`, 4 bits).
    #[bits(26..=29)]
    pin: u4,

    /// Its new mode (`GPIO_CFG_VAL_MASK`).
    #[bits(0..=1)]
    mode: PinMode,
}

/// A pin-level write: the masked pins take the given levels.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DriveWrite {
    /// Always [`Command::Drive`].
    #[bits(30..=31)]
    command: Command,

    /// Which pins change, one bit per pin (`GPIO_CMD_MASK_OFF`, 15 bits).
    #[bits(15..=29)]
    mask: u15,

    /// Their new levels, one bit per pin.
    #[bits(0..=14)]
    levels: u15,
}

/// The connector GPIO core, as libusdr's front-end drivers use it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FrontEndGpio;

impl FrontEndGpio {
    /// Sets pin `pin`'s mode (`gpio_config`).
    pub(crate) fn configure(bus: &mut dyn Bus, pin: u8, mode: PinMode) -> Result<(), Error> {
        let write = ConfigureWrite::new()
            .with_command(Command::Configure)
            .with_pin(u4::new(pin & 0xf))
            .with_mode(mode);
        Self::write(bus, write.to_raw())
    }

    /// Drives the pins in `mask` to the levels in `levels`, one bit per pin
    /// (`gpio_cmd(.., GPIO_OUT, ..)`); bits above pin 14 are dropped, as libusdr masks them.
    pub(crate) fn drive(bus: &mut dyn Bus, mask: u16, levels: u16) -> Result<(), Error> {
        let write = DriveWrite::new()
            .with_command(Command::Drive)
            .with_mask(u15::new(mask & 0x7fff))
            .with_levels(u15::new(levels & 0x7fff));
        Self::write(bus, write.to_raw())
    }

    /// Writes one command word.
    fn write(bus: &mut dyn Bus, word: u32) -> Result<(), Error> {
        bus.write_regs(REG_FE_GPIO, &[word])
            .writing(FPGA, REG_FE_GPIO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_encode_as_libusdrs_gpio_core_commands() {
        // From libusdr's own probe trace: GPIO2 to ALT0, then GPIO5/6 driven high.
        let configure = ConfigureWrite::new()
            .with_command(Command::Configure)
            .with_pin(u4::new(2))
            .with_mode(PinMode::Alternate0);
        assert_eq!(configure.to_raw(), 0xc800_0002);
        let drive = DriveWrite::new()
            .with_command(Command::Drive)
            .with_mask(u15::new(0x60))
            .with_levels(u15::new(0x60));
        assert_eq!(drive.to_raw(), 0x0030_0060);
    }

    #[test]
    fn unused_commands_keep_their_codes() {
        let command = |command| DriveWrite::new().with_command(command).to_raw() >> 30;
        assert_eq!(command(Command::SetHigh), 1);
        assert_eq!(command(Command::SetLow), 2);
        let mode = ConfigureWrite::new().with_mode(PinMode::Alternate1);
        assert_eq!(mode.to_raw() & 0b11, 3);
    }
}
