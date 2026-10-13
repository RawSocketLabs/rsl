//! The blocking bus: what a transport (USB, `PCIe`, or a simulator) must provide, and the
//! executor that runs a portable sequence over it.

use std::boxed::Box;
use std::error::Error as StdError;
use std::io;
use std::time::Duration;

#[cfg(doc)]
use super::address::{I2C_MAX_READ, I2C_MAX_WRITE};
use super::address::{I2cAddr, SpiAddr};
use super::sequence::{FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure};

/// A failed bus operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BusError {
    /// The operation did not complete in time.
    #[error("bus operation timed out")]
    Timeout,

    /// The device was unplugged or its driver went away.
    #[error("device disconnected")]
    Disconnected,

    /// The bus cannot perform this operation (for example, an I2C transfer over the limits).
    #[error("unsupported bus operation: {0}")]
    Unsupported(&'static str),

    /// An operating-system error from the transport.
    #[error("bus I/O error")]
    Io(#[from] io::Error),

    /// Any other failure. An escape hatch for custom buses whose errors have no variant
    /// here; the drivers' own transports use the specific variants.
    #[error(transparent)]
    Other(Box<dyn StdError + Send + Sync>),
}

/// Register, SPI and I2C access to one board, plus time.
///
/// Implementations perform each call as one transaction and keep the order of calls.
pub trait Bus: Send {
    /// Reads consecutive 32-bit FPGA registers starting at `addr`.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError>;

    /// Writes consecutive 32-bit FPGA registers starting at `addr`.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError>;

    /// Shifts one 32-bit word through the SPI target and returns the word read back.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError>;

    /// Writes `write` to an I2C device, then fills `read`, both in wire order.
    ///
    /// Real transports accept at most [`I2C_MAX_WRITE`] and [`I2C_MAX_READ`] bytes.
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] over those limits or for a core or bus the board lacks;
    /// any transport failure.
    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError>;

    /// Waits for `duration`. Board code never reads the clock itself, so a simulator can
    /// make this virtual.
    fn sleep(&mut self, duration: Duration);

    /// Sets up transport buffers for RX blocks of `block_bytes` (the transport's
    /// `stream_initialize`). The driver has already configured the engine's registers.
    ///
    /// The default supports no streaming, for buses that only control the board.
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] by default; any transport failure.
    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        let _ = block_bytes;
        Err(BusError::Unsupported("RX streaming"))
    }

    /// Waits up to `timeout` for the next filled RX block, lends it to `consume` with the
    /// gateware's two out-of-band words, then hands the buffer back to the DMA engine (the
    /// transport's `recv_dma_wait` and `recv_dma_release`).
    ///
    /// The out-of-band words, as libusdr's stream code reads them: the first holds the lost
    /// packet count in bits 23:0 and the completed-burst mask in bits 63:32; the second the
    /// DMA status in bits 31:0 for `PCIe`, or USB's TX statistics.
    ///
    /// # Errors
    ///
    /// [`BusError::Timeout`] if no block arrives in time; [`BusError::Unsupported`] by
    /// default; any transport failure.
    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        let _ = (timeout, consume);
        Err(BusError::Unsupported("RX streaming"))
    }

    /// Releases the RX DMA buffers (the transport's `stream_deinitialize`).
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] by default; any transport failure.
    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        Err(BusError::Unsupported("RX streaming"))
    }

    /// Sets up transport buffers for TX blocks of up to `block_bytes`, each sample taking
    /// `sample_bytes` across all channels (the transport's `stream_initialize` for the TX
    /// stream, with libusdr's `block_size` and `bits_per_sym` / 8). The driver has already
    /// configured the engine's registers.
    ///
    /// The default supports no TX streaming.
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] by default; any transport failure.
    fn tx_stream_open(&mut self, block_bytes: u32, sample_bytes: u32) -> Result<(), BusError> {
        let _ = (block_bytes, sample_bytes);
        Err(BusError::Unsupported("TX streaming"))
    }

    /// Waits up to `timeout` for a free TX buffer and lends it to `fill` (the transport's
    /// `send_dma_get`), with the four status words the transport has for this buffer, if
    /// any; `fill` writes the block from the buffer's start and returns its length in
    /// bytes (see [`TxFill`]). Then commits the block (`send_dma_commit`), to play at
    /// `time` in samples or, with `None`, as soon as it reaches the gateware.
    ///
    /// Who tells the gateware the block is there depends on the transport, as in
    /// libusdr, and the result says which: [`TxCommit::Registers`] leaves it to the
    /// driver, through the TX core's registers (libusdr's `soft_tx_commit`, which the
    /// `PCIe` transport's commit calls); [`TxCommit::Framed`] means the transport framed
    /// the block with its length and `time` and queued it (USB), and the driver writes
    /// nothing more.
    ///
    /// # Errors
    ///
    /// [`BusError::Timeout`] if no buffer frees in time; [`BusError::Unsupported`] by
    /// default; any transport failure.
    fn tx_stream_send(
        &mut self,
        timeout: Duration,
        time: Option<u64>,
        fill: &mut TxFill<'_>,
    ) -> Result<TxCommit, BusError> {
        let _ = (timeout, time, fill);
        Err(BusError::Unsupported("TX streaming"))
    }

    /// Releases the TX buffers (the transport's `stream_deinitialize`).
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] by default; any transport failure.
    fn tx_stream_close(&mut self) -> Result<(), BusError> {
        Err(BusError::Unsupported("TX streaming"))
    }
}

/// What [`Bus::tx_stream_send`] lends a TX buffer to: it takes the buffer and the status
/// words the transport has for it, if any, writes the block from the buffer's start and
/// returns its length in bytes. A transport that frames the block itself uses the length;
/// one whose gateware learns it from the driver's registers may only check it against
/// the buffer.
pub type TxFill<'a> = dyn FnMut(&mut [u8], Option<[u32; 4]>) -> usize + 'a;

/// Who tells the gateware a block [`Bus::tx_stream_send`] committed is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum TxCommit {
    /// The driver, through the TX core's registers: the transport only handed the
    /// buffer over (libusdr's `PCIe` commit, which calls back `soft_tx_commit`).
    Registers,

    /// The transport, which framed the block with its length and time and queued it
    /// (libusdr's USB commit); the driver writes nothing more.
    Framed,
}

/// Runs a portable register sequence to its end over `bus`, one action at a time: the
/// blocking executor, so the sequence's operations reach the bus exactly as written.
///
/// # Errors
///
/// The sequence's failure, with the bus error and the access it struck.
pub fn run_sequence(
    bus: &mut dyn Bus,
    sequence: &mut impl Sequence,
) -> Result<(), SequenceFailure<BusError>> {
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

#[cfg(test)]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use super::super::sequence::Access;
    use super::*;

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
    fn a_failed_fpga_read_keeps_the_bus_error_and_access_and_stops() {
        let mut bus = Recorder {
            fail: Some(19),
            ..Recorder::default()
        };
        let failure = run_sequence(&mut bus, &mut Echo::default()).unwrap_err();
        assert_eq!(failure.access(), Some(("FPGA", Access::Read, 19)));
        assert!(matches!(
            failure,
            SequenceFailure::FpgaTransport {
                source: BusError::Disconnected,
                ..
            }
        ));
        assert_eq!(bus.calls, [Call::Read(19)], "nothing follows the failure");
    }
}
