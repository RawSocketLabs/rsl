//! The chip's bring-up after reset release: libusdr's `lms7002m_create`, which loads a fixed
//! reset programme and checks the chip's version.

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::NAME;
use super::batch::Batch;
use super::chip::Lms7002m;
use super::reg::Reg;
use super::register::Register;
use super::regs::{
    Afe, LdoBypass, LdoBypassLoad, LdoDigital, LdoEnables, LdoEnablesLoad, LmlControl, Mac,
    MimoSiso, SUPPORTED, Version, Xbuf,
};
use super::spi;

/// The board's choices for the chip's power-on state: `lms7002m_create`'s arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetPlan {
    /// The SPI target the chip answers on (`lsaddr`).
    pub target: SpiAddr,

    /// Internal LDO enables (`lms_ldo_mask`): bits 15:0 to [`LdoEnables`], 31:16 to
    /// [`LdoEnablesLoad`], unchanged.
    pub ldo_mask: u32,

    /// Route the TX reference buffer to the RX one (`txrx_clk`): sets
    /// [`Xbuf`]'s second TX output and RX-from-TX input.
    pub shared_reference: bool,
}

impl ResetPlan {
    /// The register writes, in libusdr's order (`reset_regs`).
    fn programme(&self) -> Batch {
        let [ldo0, ldo1, ldo2, ldo3] = self.ldo_mask.to_le_bytes();
        let mut batch = Batch::new(self.target);
        // Every LML bit clear, both banks selected; then every bit set.
        batch.push(LmlControl::new().with_mac(Mac::Both).entry());
        batch.push(LmlControl::running(Mac::Both).entry());
        // "Enable MIMO, this bit only hardwires MAC[1]".
        batch.push(MimoSiso::new().entry());
        batch.push(LdoEnables::from_raw(u16::from_le_bytes([ldo0, ldo1])).entry());
        batch.push(LdoEnablesLoad::from_raw(u16::from_le_bytes([ldo2, ldo3])).entry());
        // No LDO bypassed.
        batch.push(LdoBypassLoad::new().entry());
        batch.push(LdoBypass::new().entry());
        // The SPI-buffer and digital-interface LDOs powered down, their global enable on.
        batch.push(
            LdoDigital::new()
                .with_spi_buf_powered_down(true)
                .with_digi_p2_powered_down(true)
                .with_digi_p1_powered_down(true)
                .with_enabled(true)
                .entry(),
        );
        // Both buffers self-biased and enabled.
        batch.push(
            Xbuf::new()
                .with_rx_self_biased(true)
                .with_tx_self_biased(true)
                .with_tx_second_output(self.shared_reference)
                .with_rx_from_tx_buffer(self.shared_reference)
                .with_enabled(true)
                .entry(),
        );
        // "Disable RF": the whole AFE powered down, its global enable off.
        batch.push(
            Afe::new()
                .with_powered_down(true)
                .with_rx_a_powered_down(true)
                .with_rx_b_powered_down(true)
                .with_tx_a_powered_down(true)
                .with_tx_b_powered_down(true)
                .entry(),
        );
        batch
    }
}

/// `lms7002m_create`: the reset programme, then the version read and check. The chip must
/// be powered and out of reset.
#[derive(Clone, Copy, Debug)]
pub struct Create {
    /// The board's choices.
    plan: ResetPlan,

    /// The reset programme.
    programme: Batch,

    /// Where the sequence is after the programme.
    stage: Stage,
}

/// What [`Create`] does once its programme is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Writing the programme, then reading the version.
    Running,

    /// The version is accepted.
    Created,

    /// A write or the read failed, or the version is refused.
    Failed,
}

impl Create {
    /// Starts the sequence.
    #[must_use]
    pub fn new(plan: ResetPlan) -> Self {
        Self {
            plan,
            programme: plan.programme(),
            stage: Stage::Running,
        }
    }

    /// The chip as `lms7002m_create` leaves it, once the sequence succeeded.
    #[must_use]
    pub const fn chip(&self) -> Option<Lms7002m> {
        match self.stage {
            Stage::Created => Some(Lms7002m::created(self.plan.target)),
            Stage::Running | Stage::Failed => None,
        }
    }

    /// The version read, once the programme is written.
    fn version_read(&self) -> Option<FpgaAccess> {
        (self.stage == Stage::Running).then(|| spi::read(self.plan.target, Reg::Version))
    }
}

impl Sequence for Create {
    fn next(&self) -> Option<SequenceAction> {
        if self.stage == Stage::Failed {
            return None;
        }
        self.programme
            .next()
            .or_else(|| self.version_read().map(SequenceAction::Fpga))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.stage == Stage::Failed {
            return Ok(());
        }
        if self.programme.next().is_some() {
            return self
                .programme
                .complete(result)
                .inspect_err(|_| self.stage = Stage::Failed);
        }
        let Some(access) = self.version_read() else {
            return Ok(());
        };
        self.stage = Stage::Failed;
        let word = result.map_err(|source| SequenceFailure::FpgaTransport {
            chip: NAME,
            access,
            source,
        })?;
        let version = Version::from_raw(spi::value(word));
        if !version.is_supported() {
            return Err(SequenceFailure::ChipId {
                chip: NAME,
                expected: SUPPORTED.id().into(),
                found: version.id().into(),
            });
        }
        self.stage = Stage::Created;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::Access;

    use super::*;

    /// A plan with every option on, so each argument reaches a register: the XTRX's LDO
    /// mask (`_xsdr_pwren_revo` passes 0x01B10D15) and a shared reference.
    const PLAN: ResetPlan = ResetPlan {
        target: SpiAddr(0),
        ldo_mask: 0x01b1_0d15,
        shared_reference: true,
    };

    /// Runs `create`, answering the version read with `version`; returns the words sent.
    fn run(create: &mut Create, version: u32) -> ([u32; 11], Result<(), SequenceFailure<()>>) {
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
    fn the_reset_programme_matches_the_generated_macros() {
        // Each word from `reset_regs` in lms7002m.c, expanded by hand with
        // gen/def_lms7002m.h's field offsets.
        let (words, outcome) = run(&mut Create::new(PLAN), 0x3840);
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            words,
            [
                0x8020_0003, // LML_0x0020(0, .., 0, LMS7_CH_AB)
                0x8020_ffff, // LML_0x0020(1, .., 1, LMS7_CH_AB)
                0x802e_0000, // LML_0x002E(0)
                0x8092_0d15, // the mask's low half
                0x8093_01b1, // the mask's high half
                0x8095_0000,
                0x8096_0000,
                0x80a6_000f, // PD_LDO_SPIBUF, _DIGIp2, _DIGIp1 and EN_G_LDOP
                0x8085_0199, // SLFB both, EN_OUT2/EN_TBUFIN for txrx_clk, EN_G_XBUF
                0x8082_003e, // PD_AFE and the four converters
                0x002f_0000, // the version read comes last
            ]
        );
    }

    #[test]
    fn any_metal_mask_is_accepted_but_not_another_revision() {
        let mut accepted = Create::new(PLAN);
        assert_eq!(run(&mut accepted, 0x387f).1, Ok(()), "MASK is not checked");
        assert_eq!(
            accepted.chip().map(|chip| chip.mac()),
            Some(Mac::Both),
            "create leaves both banks selected"
        );

        let mut refused = Create::new(PLAN);
        assert_eq!(
            run(&mut refused, 0x3880).1,
            Err(SequenceFailure::ChipId {
                chip: NAME,
                expected: 0xe1,
                found: 0xe2,
            }),
            "REV 2 is refused"
        );
        assert_eq!(refused.next(), None, "nothing follows the refusal");
        assert_eq!(refused.chip(), None, "a refused chip is not created");
    }

    #[test]
    fn a_failed_write_names_its_register_and_stops() {
        let mut create = Create::new(PLAN);
        create.complete::<()>(Ok(0)).unwrap();
        let failure = create.complete(Err(())).unwrap_err();
        assert_eq!(
            failure.access(),
            Some((NAME, Access::Write, 0x0020)),
            "the second LML_0x0020 write"
        );
        assert_eq!(create.next(), None);
        assert_eq!(create.chip(), None);
    }

    #[test]
    fn a_failed_version_read_names_the_version_register() {
        let mut create = Create::new(PLAN);
        for _ in 0..10 {
            create.complete::<()>(Ok(0)).unwrap();
        }
        let failure = create.complete(Err(())).unwrap_err();
        assert_eq!(failure.access(), Some((NAME, Access::Read, 0x002f)));
        assert_eq!(create.next(), None);
    }
}
