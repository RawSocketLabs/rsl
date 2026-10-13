//! The front-end connector's GPIO core (libusdr's `ipblks/gpio.c`): the M.2 connector's
//! general-purpose pins, which a front-end board uses for its I2C bus, 1PPS, UART, SPI and
//! fast control lines.
//!
//! One FPGA register takes every command; bits 31:30 say which. libusdr reaches the core
//! at register 3 on every board it probes (`gpiobase` in `device/device_fe.c`, hard-coded
//! there with a note to look it up from the board description instead).
//!
//! The commands are register writes, so a run of them is a portable [`GpioSetup`]
//! sequence; with feature `std` the boards' blocking code writes every command through
//! one.

use bnb::{BitEnum, bitfield, u2, u4, u15};
#[cfg(feature = "std")]
use rsl_lowlevel::{Bus, run_sequence};
use rsl_lowlevel::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};

#[cfg(feature = "std")]
use crate::error::Error;

/// The core's register (`gpiobase`).
pub const REG_FRONT_END_GPIO: u32 = 3;

/// The gateware's name in errors.
pub(crate) const FPGA: &str = "FPGA";

/// A pin of the connector's GPIO core, 0 to 14.
pub type Pin = u8;

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
pub enum PinMode {
    /// A plain input (`GPIO_CFG_IN`).
    Input,

    /// A plain output, driven by [`GpioCommand::Drive`] (`GPIO_CFG_OUT`).
    Output,

    /// The pin's first alternate function, such as the external I2C bus, 1PPS, UART or
    /// SPI (`GPIO_CFG_ALT0`).
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

/// One command to the connector's GPIO core: a single register write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpioCommand {
    /// Sets a pin's mode (`gpio_config`); the pin number keeps its low four bits.
    Configure(Pin, PinMode),

    /// Drives the pins in `mask` to the levels in `levels`, one bit per pin
    /// (`gpio_cmd(.., GPIO_OUT, ..)`); bits above pin 14 are dropped, as libusdr masks
    /// them.
    Drive {
        /// Which pins change.
        mask: u16,
        /// Their new levels.
        levels: u16,
    },
}

impl GpioCommand {
    /// The register word that carries this command.
    #[must_use]
    pub fn word(self) -> u32 {
        match self {
            Self::Configure(pin, mode) => ConfigureWrite::new()
                .with_command(Command::Configure)
                .with_pin(u4::new(pin & 0xf))
                .with_mode(mode)
                .to_raw(),
            Self::Drive { mask, levels } => DriveWrite::new()
                .with_command(Command::Drive)
                .with_mask(u15::new(mask & 0x7fff))
                .with_levels(u15::new(levels & 0x7fff))
                .to_raw(),
        }
    }

    /// This command as the register write that performs it.
    #[must_use]
    pub fn access(self) -> FpgaAccess {
        FpgaAccess::Write {
            reg: REG_FRONT_END_GPIO,
            value: self.word(),
        }
    }
}

/// A run of GPIO-core commands, in order: a board's connector setup, as a register-only
/// sequence. It stops at the first failed write.
#[derive(Clone, Copy, Debug)]
pub struct GpioSetup<'a> {
    /// The commands.
    commands: &'a [GpioCommand],
    /// How many have been written.
    done: usize,
}

impl<'a> GpioSetup<'a> {
    /// The setup writing `commands`, first to last.
    #[must_use]
    pub const fn new(commands: &'a [GpioCommand]) -> Self {
        Self { commands, done: 0 }
    }
}

impl Sequence for GpioSetup<'_> {
    fn next(&self) -> Option<SequenceAction> {
        self.commands
            .get(self.done)
            .map(|command| SequenceAction::Fpga(command.access()))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(command) = self.commands.get(self.done) else {
            return Ok(());
        };
        match result {
            Ok(_) => {
                self.done += 1;
                Ok(())
            }
            Err(source) => {
                self.done = self.commands.len();
                Err(SequenceFailure::FpgaTransport {
                    chip: FPGA,
                    access: command.access(),
                    source,
                })
            }
        }
    }
}

/// The connector's GPIO core over a blocking bus, as libusdr's front-end drivers use it.
#[cfg(feature = "std")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FrontEndGpio;

#[cfg(feature = "std")]
impl FrontEndGpio {
    /// Sets pin `pin`'s mode (`gpio_config`).
    pub(crate) fn configure(bus: &mut dyn Bus, pin: Pin, mode: PinMode) -> Result<(), Error> {
        Self::run(bus, &[GpioCommand::Configure(pin, mode)])
    }

    /// Drives the pins in `mask` to the levels in `levels`, one bit per pin
    /// (`gpio_cmd(.., GPIO_OUT, ..)`).
    pub(crate) fn drive(bus: &mut dyn Bus, mask: u16, levels: u16) -> Result<(), Error> {
        Self::run(bus, &[GpioCommand::Drive { mask, levels }])
    }

    /// Writes `commands` in order through their [`GpioSetup`].
    pub(crate) fn run(bus: &mut dyn Bus, commands: &[GpioCommand]) -> Result<(), Error> {
        Ok(run_sequence(bus, &mut GpioSetup::new(commands))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_encode_as_libusdrs_gpio_core_commands() {
        // From libusdr's own probe trace: GPIO2 to ALT0, then GPIO5/6 driven high.
        assert_eq!(
            GpioCommand::Configure(2, PinMode::Alternate0).word(),
            0xc800_0002
        );
        assert_eq!(
            GpioCommand::Drive {
                mask: 0x60,
                levels: 0x60
            }
            .word(),
            0x0030_0060
        );
    }

    #[test]
    fn unused_commands_keep_their_codes() {
        let command = |command| DriveWrite::new().with_command(command).to_raw() >> 30;
        assert_eq!(command(Command::SetHigh), 1);
        assert_eq!(command(Command::SetLow), 2);
        assert_eq!(
            GpioCommand::Configure(0, PinMode::Alternate1).word() & 0b11,
            3
        );
    }

    #[test]
    fn a_setup_writes_each_command_to_the_core_and_stops_at_a_failure() {
        const SETUP: [GpioCommand; 2] = [
            GpioCommand::Configure(0, PinMode::Input),
            GpioCommand::Configure(1, PinMode::Input),
        ];
        let mut setup = GpioSetup::new(&SETUP);
        assert_eq!(
            setup.next(),
            Some(SequenceAction::Fpga(FpgaAccess::Write {
                reg: REG_FRONT_END_GPIO,
                value: 0xc000_0000
            }))
        );
        setup.complete::<()>(Ok(0)).expect("the first write lands");
        let failure = setup.complete(Err(())).expect_err("the second fails");
        assert_eq!(
            failure.access(),
            Some((FPGA, rsl_lowlevel::Access::Write, 3))
        );
        assert_eq!(setup.next(), None, "nothing follows a failure");
    }
}
