//! Register access for the blocking drivers. The typed-register traits live in the
//! portable core ([`Register`], [`IndexedRegister`]), shared with its sans-I/O machines.

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};
pub(crate) use crate::portable::register::{IndexedRegister, Register};
use crate::portable::sequence::{FpgaAccess, RegisterAccess, Sequence, SequenceAction};

/// Runs a portable register sequence to its end over `bus`, one action at a time: the
/// blocking executor, so the sequence's operations reach the bus exactly as written.
pub(crate) fn run_sequence(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    while let Some(action) = sequence.next() {
        let result = match action {
            SequenceAction::Access(RegisterAccess::Read { dev, reg }) => {
                let mut byte = [0];
                bus.i2c(dev, &[reg], &mut byte).map(|()| byte[0].into())
            }
            SequenceAction::Access(RegisterAccess::Write { dev, reg, value }) => {
                bus.i2c(dev, &[reg, value], &mut []).map(|()| value.into())
            }
            SequenceAction::Fpga(FpgaAccess::Read { reg }) => {
                let mut word = [0];
                bus.read_regs(reg.into(), &mut word).map(|()| word[0])
            }
            SequenceAction::Fpga(FpgaAccess::Write { reg, value }) => {
                bus.write_regs(reg.into(), &[value]).map(|()| value)
            }
            SequenceAction::Fpga(FpgaAccess::Spi { target, word, .. }) => bus.spi32(target, word),
            SequenceAction::Sleep(duration) => {
                bus.sleep(duration);
                Ok(0)
            }
        };
        sequence.complete(result)?;
    }
    Ok(())
}

/// A chip whose byte-wide registers sit behind an I2C device with an auto-incrementing
/// register pointer. Visible only to chip modules, so board code reaches the registers
/// through the driver's methods.
pub(in crate::chips) trait I2cChip {
    /// The chip's name in errors.
    const NAME: &'static str;

    /// Where the chip answers.
    fn addr(&self) -> I2cAddr;

    /// This chip's registers on `bus`, for the accesses of one operation.
    fn on<'a>(&self, bus: &'a mut dyn Bus) -> I2cWriter<'a> {
        I2cWriter {
            dev: self.addr(),
            chip: Self::NAME,
            bus,
        }
    }
}

/// An [`I2cChip`]'s registers bound to a bus for one operation; it writes. Reads run in
/// portable sequences.
pub(in crate::chips) struct I2cWriter<'a> {
    /// Where the chip answers.
    dev: I2cAddr,

    /// Names the chip in errors.
    chip: &'static str,

    /// The bus the chip is on.
    bus: &'a mut dyn Bus,
}

impl I2cWriter<'_> {
    /// Writes a typed register.
    pub(in crate::chips) fn write<R: Register>(&mut self, value: R) -> Result<(), Error> {
        self.write_raw(R::ADDR, value.into())
    }

    /// Writes one copy of a repeated register.
    pub(in crate::chips) fn write_to<R: IndexedRegister>(
        &mut self,
        index: R::Index,
        value: R,
    ) -> Result<(), Error> {
        self.write_raw(R::addr(index), value.into())
    }

    /// Writes a register as a byte.
    pub(in crate::chips) fn write_raw(
        &mut self,
        reg: impl Into<u8>,
        value: u8,
    ) -> Result<(), Error> {
        let reg = reg.into();
        self.bus
            .i2c(self.dev, &[reg, value], &mut [])
            .writing(self.chip, reg)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::error::Access;
    use crate::lowlevel::{BusError, SpiAddr};
    use crate::portable::sequence::{FpgaAccess, SequenceFailure};

    /// What reached the bus, in order.
    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        Read(u32),
        Write(u32, Vec<u32>),
        Spi(u32, u32),
        Sleep(Duration),
    }

    /// Answers reads with `reg + 100` and SPI with the word inverted; fails reads of `fail`.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<Call>,
        fail: Option<u32>,
    }

    impl Bus for Recorder {
        fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
            self.calls.push(Call::Read(addr));
            if self.fail == Some(addr) {
                return Err(BusError::Disconnected);
            }
            out.fill(addr + 100);
            Ok(())
        }

        fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
            self.calls.push(Call::Write(addr, values.to_vec()));
            Ok(())
        }

        fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
            self.calls.push(Call::Spi(target.0, word));
            Ok(!word)
        }

        fn i2c(&mut self, _: I2cAddr, _: &[u8], _: &mut [u8]) -> Result<(), BusError> {
            unreachable!("this sequence makes no I2C access")
        }

        fn sleep(&mut self, duration: Duration) {
            self.calls.push(Call::Sleep(duration));
        }
    }

    /// Reads register 19, writes it plus one to register 0, shifts that through SPI, then
    /// waits; keeps every value reported to it.
    #[derive(Default)]
    struct Echo {
        step: usize,
        seen: Vec<u32>,
    }

    impl Sequence for Echo {
        fn next(&self) -> Option<SequenceAction> {
            let last = self.seen.last().copied().unwrap_or(0);
            Some(match self.step {
                0 => SequenceAction::Fpga(FpgaAccess::Read { reg: 19 }),
                1 => SequenceAction::Fpga(FpgaAccess::Write {
                    reg: 0,
                    value: last + 1,
                }),
                2 => SequenceAction::Fpga(FpgaAccess::Spi {
                    target: SpiAddr(0),
                    word: last,
                    reg: 4,
                    write: true,
                }),
                3 => SequenceAction::Sleep(Duration::from_micros(10)),
                _ => return None,
            })
        }

        fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
            let Some(SequenceAction::Fpga(access)) = self.next() else {
                self.step += 1;
                return Ok(());
            };
            let value = result.map_err(|source| SequenceFailure::FpgaTransport {
                chip: "FPGA",
                access,
                source,
            })?;
            self.seen.push(value);
            self.step += 1;
            Ok(())
        }
    }

    #[test]
    fn fpga_actions_reach_the_bus_as_written_and_report_their_words() {
        let mut bus = Recorder::default();
        let mut echo = Echo::default();
        run_sequence(&mut bus, &mut echo).unwrap();
        assert_eq!(
            bus.calls,
            [
                Call::Read(19),
                Call::Write(0, vec![120]),
                Call::Spi(0, 120),
                Call::Sleep(Duration::from_micros(10)),
            ]
        );
        assert_eq!(
            echo.seen,
            [119, 120, !120],
            "read word, written value, SPI reply"
        );
    }

    #[test]
    fn a_failed_fpga_read_names_the_gateware_register_and_stops() {
        let mut bus = Recorder {
            fail: Some(19),
            ..Recorder::default()
        };
        let error = run_sequence(&mut bus, &mut Echo::default()).unwrap_err();
        assert!(matches!(
            error,
            Error::Bus {
                chip: "FPGA",
                access: Access::Read,
                reg: 19,
                source: BusError::Disconnected,
            }
        ));
        assert_eq!(bus.calls, [Call::Read(19)], "nothing follows the failure");
    }

    #[test]
    fn a_failed_spi_exchange_names_the_chip_register_and_direction() {
        let failure = SequenceFailure::FpgaTransport {
            chip: "LMS6002D",
            access: FpgaAccess::Spi {
                target: SpiAddr(0),
                word: 0x8412,
                reg: 4,
                write: true,
            },
            source: BusError::Timeout,
        };
        assert!(matches!(
            Error::from(failure),
            Error::Bus {
                chip: "LMS6002D",
                access: Access::Write,
                reg: 4,
                source: BusError::Timeout,
            }
        ));
    }
}
