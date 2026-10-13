//! General-purpose outputs (one 8-bit latch per bank) and inputs (byte-addressed words).
//!
//! The FPGA drives the board's control lines: resets, RF switches, enables, the LED.
//! Each is a GPO bank the host sets with one register write. The GPI words report what
//! the gateware knows about itself and the board, including the revision the driver
//! branches on.
//!
//! Source: `device/generic_usdr/generic_regs.h` and `dev_gpo_set`/`dev_gpi_get32` in
//! `device/m2_lm6_1/usdr_ctrl.c`.

use bnb::bitfield;

use crate::error::{BusContext, Error};
use crate::identity::Identity;
use crate::lowlevel::Bus;
use crate::portable::sequence::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};

pub(crate) use crate::portable::fpga::{FPGA, Gpo};

/// First FPGA register of the general-purpose inputs.
const REG_GPI_BASE: u32 = 16;

impl Gpo {
    /// Latches `value` into this bank.
    pub(crate) fn set(self, bus: &mut dyn Bus, value: u8) -> Result<(), Error> {
        let FpgaAccess::Write { reg, value } = self.write(value) else {
            unreachable!("invariant: a GPO latch is a register write");
        };
        bus.write_regs(reg, &[value]).writing(FPGA, reg)
    }
}

/// General-purpose input words, by byte offset (`IGPI_*` in `generic_regs.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gpi {
    /// Xilinx `USR_ACCESS2`: the gateware build ID, stamped into the bitstream when it is
    /// built. libusdr only logs it.
    UsrAccess2 = 0,

    /// Hardware ID; see [`Hwid`].
    Hwid = 12,
    /// The word libusdr's `/dm/sdr/0/clkmeas` reads back as the clock measurement. Its
    /// header names offset 24 `IGPI_USBC`; the vendored source does not document what the
    /// gateware reports there.
    ClockMeasurement = 24,
}

impl Gpi {
    /// The FPGA register holding this input's 32-bit word.
    pub(crate) const fn register(self) -> u32 {
        REG_GPI_BASE + self as u32 / 4
    }

    /// Reads the 32-bit input word holding this input.
    pub(crate) fn read(self, bus: &mut dyn Bus) -> Result<u32, Error> {
        let reg = self.register();
        let mut word = [0];
        bus.read_regs(reg, &mut word).reading(FPGA, reg)?;
        Ok(word[0])
    }
}

/// The hardware ID word.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hwid {
    /// The TX chain's stream core is the extended one (`CORE_EXFETX_DMA32_R0_2`), which
    /// libusdr's TX stream create tests as `hwid & (1 << (24 + 2))`.
    #[bits(26..=26)]
    has_extended_tx_core: bool,

    /// The board has an RX chain.
    #[bits(25..=25)]
    has_rx: bool,

    /// The board has a TX chain.
    #[bits(24..=24)]
    has_tx: bool,

    /// Board revision.
    #[bits(8..=15)]
    revision: u8,
}

/// The gateware identity as a sequence: the build word, then the hardware ID, as
/// `dev_m2_lm6_1_sdr_revision_get` reads them. The blocking board and the native USB handle
/// both run it.
#[derive(Debug, Default)]
pub(crate) struct IdentityRead {
    /// The gateware build word, once read.
    build: Option<u32>,
    /// The hardware ID word, once read.
    hwid: Option<u32>,
}

impl IdentityRead {
    /// The identity, once both words are read.
    pub(crate) fn identity(&self) -> Option<Identity> {
        let (gateware_build, raw) = (self.build?, self.hwid?);
        let hwid = Hwid::from_raw(raw);
        Some(Identity {
            revision: hwid.revision(),
            has_rx: hwid.has_rx(),
            has_tx: hwid.has_tx(),
            gateware_build,
            hwid: raw,
        })
    }
}

impl Sequence for IdentityRead {
    fn next(&self) -> Option<SequenceAction> {
        let input = match (self.build, self.hwid) {
            (None, _) => Gpi::UsrAccess2,
            (Some(_), None) => Gpi::Hwid,
            (Some(_), Some(_)) => return None,
        };
        Some(SequenceAction::Fpga(FpgaAccess::Read {
            reg: input.register(),
        }))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(SequenceAction::Fpga(access)) = self.next() else {
            return Ok(());
        };
        let word = result.map_err(|source| SequenceFailure::FpgaTransport {
            chip: FPGA,
            access,
            source,
        })?;
        if self.build.is_none() {
            self.build = Some(word);
        } else {
            self.hwid = Some(word);
        }
        Ok(())
    }
}
