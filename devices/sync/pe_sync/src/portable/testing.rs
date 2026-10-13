//! A test executor for the portable sequences: runs one to its end against scripted
//! answers and records what it did.

use std::vec::Vec;

use rsl_lowlevel::{FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure};

/// One action a sequence took, in the trace's terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exchange {
    /// An SPI word shifted out.
    Spi(u32),
    /// A wait, in microseconds.
    Sleep(u128),
    /// A gateware register read.
    Read(u32),
    /// A gateware register write.
    Write(u32, u32),
    /// An I²C register read: the device's 7-bit address and the register.
    I2cRead(u8, u8),
    /// An I²C register write: the device, the register and the byte.
    I2cWrite(u8, u8, u8),
}

impl From<SequenceAction> for Exchange {
    fn from(action: SequenceAction) -> Self {
        match action {
            SequenceAction::Fpga(FpgaAccess::Spi { word, .. }) => Self::Spi(word),
            SequenceAction::Fpga(FpgaAccess::Read { reg }) => Self::Read(reg),
            SequenceAction::Fpga(FpgaAccess::Write { reg, value }) => Self::Write(reg, value),
            SequenceAction::Sleep(wait) => Self::Sleep(wait.as_micros()),
            SequenceAction::Access(RegisterAccess::Read { dev, reg }) => {
                Self::I2cRead(dev.addr(), reg)
            }
            SequenceAction::Access(RegisterAccess::Write { dev, reg, value }) => {
                Self::I2cWrite(dev.addr(), reg, value)
            }
        }
    }
}

/// Runs `sequence` to its end, answering each action with `answer`: what it did, and how
/// it ended.
pub(crate) fn drive(
    sequence: &mut impl Sequence,
    mut answer: impl FnMut(&SequenceAction) -> Result<u32, ()>,
) -> (Vec<Exchange>, Result<(), SequenceFailure<()>>) {
    let mut done = Vec::new();
    while let Some(action) = sequence.next() {
        done.push(Exchange::from(action));
        if let Err(failure) = sequence.complete(answer(&action)) {
            return (done, Err(failure));
        }
    }
    (done, Ok(()))
}
