//! TI LMK1D1208I LVDS fan-out buffer: libusdr's `hw/lmk1d1208i/lmk1d1208i.c`
//! (`lmk1d1208i_create`) as `PE_SYNC` uses it, for its two fan-out buffers.
//!
//! Two inputs, two banks of four outputs; each bank takes either input or is muted. Byte
//! registers behind an 8-bit I²C register pointer; fields from the pinned
//! `hw/lmk1d1208i/lmk1d1208i.yaml`.
//!
//! Only `PE_SYNC` uses the chip in libusdr, so it lives in this driver until a second one
//! needs it (`devices/AGENTS.md`); it knows no board.

use bnb::{bitfield, u2, u4};
use rsl_lowlevel::{
    I2cAddr, Register, RegisterAccess, Sequence, SequenceAction, SequenceFailure, i2c_byte,
};

use super::outcome::outcome;

/// The chip's name in errors.
pub(crate) const NAME: &str = "LMK1D1208I";

/// Register addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// See [`OutputEnables`]. `R0`.
    OutputEnables = 0x00,
    /// See [`OutputSwings`]. `R1`.
    OutputSwings = 0x01,
    /// See [`Inputs`]. `R2`.
    Inputs = 0x02,
    /// See [`Identity`]. `R5`.
    Identity = 0x05,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// Each output on (bit n for output n) or high-impedance. `OUTn_EN`, `R0`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputEnables {
    /// Bit n: output n on.
    #[bits(0..=7)]
    outputs: u8,
}
impl Register for OutputEnables {
    type Map = Reg;
    const ADDR: Reg = Reg::OutputEnables;
}

/// Each output's swing: boosted LVDS (500 mV, bit n for output n) or standard (350 mV).
/// `OUTn_AMP_SEL`, `R1`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputSwings {
    /// Bit n: output n boosted.
    #[bits(0..=7)]
    boosted: u8,
}
impl Register for OutputSwings {
    type Map = Reg;
    const ADDR: Reg = Reg::OutputSwings;
}

/// The inputs and the banks' sources. `R2`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Inputs {
    /// Reserved, written 1 (`R2_RESERVED_0`, `R2_RESERVED_1`, bits 7:6): "Writing a
    /// different value than 1 will affect device functionality".
    #[bits(6..=7)]
    reserved: u2,
    /// Bank 1 takes IN0 rather than IN1. `BANK1_IN_SEL`, bit 5.
    #[bits(5..=5)]
    bank1_from_in0: bool,
    /// Bank 0 takes IN0 rather than IN1. `BANK0_IN_SEL`, bit 4.
    #[bits(4..=4)]
    bank0_from_in0: bool,
    /// Bank 1's outputs held low. `BANK1_MUTE`, bit 3.
    #[bits(3..=3)]
    bank1_muted: bool,
    /// Bank 0's outputs held low. `BANK0_MUTE`, bit 2.
    #[bits(2..=2)]
    bank0_muted: bool,
    /// IN1 on. `IN1_EN`, bit 1.
    #[bits(1..=1)]
    in1: bool,
    /// IN0 on. `IN0_EN`, bit 0.
    #[bits(0..=0)]
    in0: bool,
}
impl Register for Inputs {
    type Map = Reg;
    const ADDR: Reg = Reg::Inputs;
}

/// The chip's identity; read only. `R5`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    /// The silicon revision; 2 at reset. `REV_ID`, bits 7:4.
    #[bits(4..=7)]
    revision: u4,
    /// The device; 0 at reset. `DEV_ID`, bits 3:0.
    #[bits(0..=3)]
    device: u4,
}

impl Identity {
    /// The identity `lmk1d1208i_create` expects: revision 2, device 0.
    const EXPECTED: Self = Self::new().with_revision(u4::new(2));

    /// Whether `lmk1d1208i_create` accepts it. Its check, `rev_id != 0x2 && dev_id !=
    /// 0x0`, refuses only a chip wrong in both; that is kept.
    fn accepted(self) -> bool {
        self.revision() == Self::EXPECTED.revision() || self.device() == Self::EXPECTED.device()
    }
}

/// One bank of four outputs (`lmk1d1208i_config_t`'s `bank`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bank {
    /// It takes IN0 (`LMK1D1208I_IN0`) rather than IN1.
    pub(crate) from_in0: bool,
    /// Its outputs held low.
    pub(crate) muted: bool,
}

/// The configuration `lmk1d1208i_create` writes (`lmk1d1208i_config_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    /// Which inputs are on, IN0 first.
    pub(crate) inputs: [bool; 2],
    /// Banks 0 and 1.
    pub(crate) banks: [Bank; 2],
    /// Which outputs are on: bit n for output n.
    pub(crate) outputs: u8,
    /// Which outputs have the boosted swing: bit n for output n.
    pub(crate) boosted: u8,
}

impl Config {
    /// The three writes, in `lmk1d1208i_create`'s order.
    fn writes(self) -> [(Reg, u8); 3] {
        let [in0, in1] = self.inputs;
        let [bank0, bank1] = self.banks;
        [
            OutputEnables::new().with_outputs(self.outputs).entry(),
            OutputSwings::new().with_boosted(self.boosted).entry(),
            Inputs::new()
                .with_reserved(u2::new(3))
                .with_bank1_from_in0(bank1.from_in0)
                .with_bank0_from_in0(bank0.from_in0)
                .with_bank1_muted(bank1.muted)
                .with_bank0_muted(bank0.muted)
                .with_in1(in1)
                .with_in0(in0)
                .entry(),
        ]
    }
}

/// `lmk1d1208i_create`: the identity read and check, then the configuration's three
/// writes; the first failure ends it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Create {
    /// Where the chip answers.
    dev: I2cAddr,
    /// The writes.
    writes: [(Reg, u8); 3],
    /// Actions completed: the read, then each write.
    done: usize,
    /// An action failed or the identity was refused.
    failed: bool,
}

impl Create {
    /// Starts the sequence for the chip at `dev`.
    pub(crate) fn new(dev: I2cAddr, config: Config) -> Self {
        Self {
            dev,
            writes: config.writes(),
            done: 0,
            failed: false,
        }
    }
}

impl Sequence for Create {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        let access = match self.done {
            0 => RegisterAccess::read(self.dev, Reg::Identity),
            done => RegisterAccess::write(self.dev, *self.writes.get(done - 1)?),
        };
        Some(SequenceAction::Access(access))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let reported = i2c_byte(outcome(action, result, NAME).inspect_err(|_| self.failed = true));
        let byte = reported?;
        if self.done == 0 {
            let identity = Identity::from_raw(byte);
            if !identity.accepted() {
                self.failed = true;
                return Err(SequenceFailure::ChipId {
                    chip: NAME,
                    expected: Identity::EXPECTED.to_raw().into(),
                    found: byte.into(),
                });
            }
        }
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::testing::{Exchange, drive};

    /// Every output on in standard LVDS from IN0, IN1 off: `PE_SYNC`'s configuration.
    const ALL_FROM_IN0: Config = Config {
        inputs: [true, false],
        banks: [Bank {
            from_in0: true,
            muted: false,
        }; 2],
        outputs: 0xff,
        boosted: 0,
    };

    /// The fan-out buffer at 0x68.
    const DEV: I2cAddr = I2cAddr::new(1, 0x68);

    #[test]
    fn create_checks_the_identity_then_writes_its_configuration() {
        // Golden session.trace lines 1488-1491.
        let (done, outcome) = drive(&mut Create::new(DEV, ALL_FROM_IN0), |_| Ok(0x20));
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done,
            [
                Exchange::I2cRead(0x68, 0x05),
                Exchange::I2cWrite(0x68, 0x00, 0xff),
                Exchange::I2cWrite(0x68, 0x01, 0x00),
                Exchange::I2cWrite(0x68, 0x02, 0xf1),
            ]
        );
    }

    #[test]
    fn only_a_chip_wrong_in_revision_and_device_both_is_refused() {
        for (identity, accepted) in [(0x20, true), (0x30, true), (0x21, true), (0x31, false)] {
            let (done, outcome) = drive(&mut Create::new(DEV, ALL_FROM_IN0), |_| Ok(identity));
            assert_eq!(outcome.is_ok(), accepted, "identity {identity:#04x}");
            assert_eq!(done.len(), if accepted { 4 } else { 1 });
        }
    }
}
