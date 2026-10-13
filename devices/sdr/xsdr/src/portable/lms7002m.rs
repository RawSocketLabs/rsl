//! The LMS7002M RF transceiver's bring-up after reset release: libusdr's `lms7002m_create`
//! (`hw/lms7002m/lms7002m.c`), which loads a fixed reset programme and checks the chip's
//! version.
//!
//! SPI word format (`gen/def_lms7002m.h`, `MAKE_LMS7002M_REG_WR`/`_RD`): bit 31 selects a
//! write, bits 30:16 carry the 15-bit register address, bits 15:0 the data; a read returns
//! the register in bits 15:0. Field names and positions below are the generated header's.
//!
//! Chip-level code: when a second LMS7002M driver (the `LimeSDR`'s) needs it, it moves to a shared
//! `devices/sdr/chips/lms7002m` crate (see `PORTING.md`).

use rsl_lowlevel::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::outcome::outcome;

/// The chip's name in errors.
pub(crate) const NAME: &str = "LMS7002M";

/// SPI bit 31: the word writes its register.
const WRITE: u32 = 1 << 31;

/// The registers the bring-up touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reg {
    /// LML resets, channel enables and the `MAC` channel select (bits 1:0). `LML_0x0020`.
    LmlControl = 0x0020,

    /// `MIMO_SISO` (bit 15): 1 powers channel B's digital part down. `LML_0x002E`.
    MimoSiso = 0x002e,

    /// Chip version (`VER`, bits 15:11), revision (`REV`, 10:6) and metal mask (`MASK`,
    /// 5:0); read only. `LML_0x002F`.
    Version = 0x002f,

    /// AFE: DAC current, ADC interleaving, muxes and power-downs (`PD_AFE` bit 5,
    /// `PD_RX_AFE1`/`2` bits 4:3, `PD_TX_AFE1`/`2` bits 2:1, `EN_G_AFE` bit 0).
    /// `AFE_0x0082`.
    Afe = 0x0082,

    /// The XBUF input buffers' self-biasing (`SLFB_XBUF_RX`/`TX`, bits 8:7), bypasses
    /// (6:5), TX buffer second output (`EN_OUT2_XBUF_TX`, bit 4), RX buffer input from the
    /// TX buffer (`EN_TBUFIN_XBUF_RX`, bit 3), power-downs (2:1) and enable (`EN_G_XBUF`,
    /// bit 0). `XBUF_0x0085`.
    Xbuf = 0x0085,

    /// Internal LDO enables, low half of the board's LDO mask. `LDO_0x0092`.
    LdoEnable0 = 0x0092,

    /// Internal LDO enables, high half of the board's LDO mask. `LDO_0x0093`.
    LdoEnable1 = 0x0093,

    /// LDO bypasses and load-impedance enables, part one. `LDO_0x0095`.
    LdoBypass0 = 0x0095,

    /// LDO bypasses, part two. `LDO_0x0096`.
    LdoBypass1 = 0x0096,

    /// SPI-buffer and digital-interface LDOs: speed-up, bypass, load impedance,
    /// power-downs (`PD_LDO_SPIBUF`, `_DIGIp2`, `_DIGIp1`, bits 3:1) and `EN_G_LDOP` (bit 0).
    /// `LDO_0x00A6`.
    LdoDigital = 0x00a6,
}

/// [`Reg::LmlControl`]'s `MAC` value selecting both channels (`LMS7_CH_AB`).
const MAC_BOTH: u16 = 0b11;

/// What [`Reg::Version`] reports: chip version `VER` (bits 15:11), revision `REV` (10:6) and
/// metal mask `MASK` (5:0). The reset programme's writes stay `(Reg, u16)` for now
/// (`PORTING.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Version(u16);

/// `VER` 7, `REV` 1: the only chip `lms7002m_create` accepts (`-ENODEV` otherwise), any
/// `MASK`.
const SUPPORTED: Version = Version(7 << 11 | 1 << 6);

impl Version {
    /// The register from an SPI read's word; `lms7002m_spi_rd` keeps bits 15:0.
    const fn from_word(word: u32) -> Self {
        let [lo, hi, ..] = word.to_le_bytes();
        Self(u16::from_le_bytes([lo, hi]))
    }

    /// `VER` (bits 15:11).
    const fn chip(self) -> u16 {
        self.0 >> 11
    }

    /// `REV` (bits 10:6).
    const fn revision(self) -> u16 {
        self.0 >> 6 & 0x1f
    }

    /// `VER:REV` (bits 15:6), the identity errors report.
    const fn id(self) -> u16 {
        self.0 >> 6
    }

    /// Whether `lms7002m_create` accepts this chip.
    const fn is_supported(self) -> bool {
        self.chip() == SUPPORTED.chip() && self.revision() == SUPPORTED.revision()
    }
}

/// The board's choices for the chip's power-on state (`lms7002m_create`'s arguments).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ResetPlan {
    /// The SPI target the chip answers on.
    pub(crate) target: SpiAddr,

    /// Internal LDO enables: bits 15:0 to [`Reg::LdoEnable0`], 31:16 to
    /// [`Reg::LdoEnable1`] (`lms_ldo_mask`).
    pub(crate) ldo_mask: u32,

    /// Route the TX reference buffer to the RX one (`txrx_clk`): sets `EN_OUT2_XBUF_TX`
    /// and `EN_TBUFIN_XBUF_RX`.
    pub(crate) shared_reference: bool,
}

impl ResetPlan {
    /// The register writes, in libusdr's order (`reset_regs`).
    fn programme(&self) -> [(Reg, u16); 10] {
        let [ldo0, ldo1, ldo2, ldo3] = self.ldo_mask.to_le_bytes();
        let xbuf_shared = if self.shared_reference {
            1 << 4 | 1 << 3
        } else {
            0
        };
        [
            // All LML/FIFO resets asserted, channels off, MAC = both.
            (Reg::LmlControl, MAC_BOTH),
            // Every reset released, every channel enabled, MAC = both.
            (Reg::LmlControl, 0xfffc | MAC_BOTH),
            // MIMO: channel B's digital part stays powered.
            (Reg::MimoSiso, 0),
            (Reg::LdoEnable0, u16::from_le_bytes([ldo0, ldo1])),
            (Reg::LdoEnable1, u16::from_le_bytes([ldo2, ldo3])),
            // No LDO bypassed.
            (Reg::LdoBypass0, 0),
            (Reg::LdoBypass1, 0),
            // SPI-buffer and digital-interface LDOs powered down, their global enable on.
            (Reg::LdoDigital, 0b1111),
            // Both XBUFs self-biased and enabled.
            (Reg::Xbuf, 1 << 8 | 1 << 7 | xbuf_shared | 1),
            // "Disable RF": the whole AFE powered down, its global enable off.
            (Reg::Afe, 0b11_1110),
        ]
    }
}

/// `lms7002m_create`: the reset programme, then the version read and check.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lms7002mCreate {
    /// The board's choices.
    plan: ResetPlan,

    /// Exchanges completed: the programme's words, then the version read.
    done: usize,

    /// An exchange failed or the version was wrong; nothing more is issued.
    failed: bool,
}

impl Lms7002mCreate {
    /// Starts the sequence; the chip must be powered and out of reset.
    pub(crate) const fn new(plan: ResetPlan) -> Self {
        Self {
            plan,
            done: 0,
            failed: false,
        }
    }

    /// The SPI exchange to perform next.
    fn access(&self) -> Option<FpgaAccess> {
        if self.failed {
            return None;
        }
        let programme = self.plan.programme();
        let (reg, write, data) = match programme.get(self.done) {
            Some(&(reg, value)) => (reg, true, value),
            None if self.done == programme.len() => (Reg::Version, false, 0),
            None => return None,
        };
        let addr = reg as u32;
        Some(FpgaAccess::Spi {
            target: self.plan.target,
            word: if write { WRITE } else { 0 } | addr << 16 | u32::from(data),
            reg: addr,
            write,
        })
    }
}

impl Sequence for Lms7002mCreate {
    fn next(&self) -> Option<SequenceAction> {
        self.access().map(SequenceAction::Fpga)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(access) = self.access() else {
            return Ok(());
        };
        let word = outcome(SequenceAction::Fpga(access), result, NAME)
            .inspect_err(|_| self.failed = true)?;
        self.done += 1;
        if matches!(access, FpgaAccess::Spi { write: false, .. }) {
            let version = Version::from_word(word);
            if !version.is_supported() {
                self.failed = true;
                return Err(SequenceFailure::ChipId {
                    chip: NAME,
                    expected: SUPPORTED.id().into(),
                    found: version.id().into(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::Access;

    use super::*;

    /// A plan with every option on, so each argument reaches a register.
    const PLAN: ResetPlan = ResetPlan {
        target: SpiAddr(0),
        ldo_mask: 0x01b1_0d15,
        shared_reference: true,
    };

    /// Runs `create`, answering the version read with `version`; returns the words sent.
    fn run(
        create: &mut Lms7002mCreate,
        version: u32,
    ) -> ([u32; 11], Result<(), SequenceFailure<()>>) {
        let mut words = [0; 11];
        let mut sent = 0;
        while let Some(SequenceAction::Fpga(FpgaAccess::Spi { word, write, .. })) = create.next() {
            words[sent] = word;
            sent += 1;
            if let Err(failure) = create.complete::<()>(Ok(if write { 0 } else { version })) {
                return (words, Err(failure));
            }
        }
        (words, Ok(()))
    }

    #[test]
    fn the_ldo_mask_and_reference_sharing_reach_their_registers() {
        // Expected words from the generated macros, field by field (`def_lms7002m.h`).
        let (words, outcome) = run(&mut Lms7002mCreate::new(PLAN), 0x3840);
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            words[3], 0x8092_0d15,
            "LDO_0x0092 takes the mask's low half"
        );
        assert_eq!(
            words[4], 0x8093_01b1,
            "LDO_0x0093 takes the mask's high half"
        );
        assert_eq!(
            words[8], 0x8085_0199,
            "EN_OUT2_XBUF_TX and EN_TBUFIN_XBUF_RX set"
        );
        assert_eq!(words[10], 0x002f_0000, "the version read comes last");
    }

    #[test]
    fn any_metal_mask_is_accepted_but_not_another_revision() {
        let (_, outcome) = run(&mut Lms7002mCreate::new(PLAN), 0x387f);
        assert_eq!(outcome, Ok(()), "MASK is not checked");

        let mut create = Lms7002mCreate::new(PLAN);
        let (_, outcome) = run(&mut create, 0x3880);
        assert_eq!(
            outcome,
            Err(SequenceFailure::ChipId {
                chip: NAME,
                expected: 0xe1,
                found: 0xe2,
            }),
            "REV 2 is refused"
        );
        assert_eq!(create.next(), None, "nothing follows the refusal");
    }

    #[test]
    fn a_failed_write_names_its_register_and_stops() {
        let mut create = Lms7002mCreate::new(PLAN);
        create.complete::<()>(Ok(0)).unwrap();
        let failure = create.complete(Err(())).unwrap_err();
        assert_eq!(
            failure.access(),
            Some((NAME, Access::Write, 0x0020)),
            "the second LML_0x0020 write"
        );
        assert_eq!(create.next(), None);
    }
}
