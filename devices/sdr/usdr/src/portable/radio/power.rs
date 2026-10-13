//! Power-up and power-down (`usdr_init` and `usdr_dtor` in `device/m2_lm6_1/usdr_ctrl.c`),
//! and each chain's power (`_usdr_pwr_state`).

use core::time::Duration;

use super::super::address::I2cAddr;
use super::super::fpga::{FPGA, Gpo};
use super::super::lms6002d::chip::Create;
use super::super::lms6002d::{Lms6002d, Lna, PowerAmp};
use super::super::power::{PowerRails, RailPlan};
use super::super::script::Script;
use super::super::sequence::{FpgaAccess, SequenceFailure};
use super::super::si5332::sequence::POWER_UP_WRITES;
use super::super::si5332::{ClockSequence, LvpeclOutput, Reference};
use super::run::{Policy, Steps, owns_radio, parts};
use super::state::{Chain, Chains, Radio};

/// The settling wait after a chain's power changes.
const CHAIN_SETTLE: Duration = Duration::from_millis(25);

/// Powers a chain of the LMS6002D on or off, if it is not already (`_usdr_pwr_state`).
///
/// The receiver's new state is recorded once its enables are written, and RXVGA2 follows,
/// then the 25 ms wait; the first failure stops it. The transmitter's state is recorded and
/// the wait taken even when its write fails, as libusdr does; the failure is reported after.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChainPower {
    /// The state.
    pub(crate) radio: Radio,
    /// Which chain.
    chain: Chain,
    /// On or off.
    on: bool,
    /// Parts begun.
    stage: u8,
}

impl ChainPower {
    /// Powers `chain` `on` or off.
    pub(crate) const fn new(radio: Radio, chain: Chain, on: bool) -> Self {
        Self {
            radio,
            chain,
            on,
            stage: 0,
        }
    }
}

impl Steps for ChainPower {
    type Part = Script;

    fn next_part(&mut self, _failed: bool) -> Option<Script> {
        self.stage += 1;
        let radio = &mut self.radio;
        match (self.chain, self.stage) {
            (Chain::Rx, 1) if radio.rx.powered != self.on => {
                Some(radio.lms.set_rx_enabled(self.on))
            }
            (Chain::Rx, 2) => {
                radio.rx.powered = self.on;
                let vga2 = if self.on {
                    radio.lms.enable_rx_vga2()
                } else {
                    radio.lms.disable_rx_vga2()
                };
                Some(vga2.sleep(CHAIN_SETTLE))
            }
            (Chain::Tx, 1) if radio.tx.powered != self.on => {
                Some(radio.lms.set_tx_enabled(self.on))
            }
            (Chain::Tx, 2) => {
                radio.tx.powered = self.on;
                Some(Script::new().sleep(CHAIN_SETTLE))
            }
            _ => None,
        }
    }

    fn policy<E>(&self, _: &SequenceFailure<E>) -> Policy {
        match self.chain {
            Chain::Rx => Policy::Stop,
            Chain::Tx => Policy::Continue,
        }
    }
}

/// Holds the RF transceiver in reset and turns off the LED and booster. Every write is
/// attempted; the first failure is reported.
pub(crate) fn power_down() -> Script {
    Script::new()
        .fpga(FPGA, Gpo::LmsReset.write(0))
        .fpga(FPGA, Gpo::Led.write(0))
        .fpga(FPGA, Gpo::Booster.write(0))
        .every()
}

/// What power-up needs from the board: its wiring and setpoints and what the
/// identification found.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PowerUpPlan {
    /// The rails' setpoints.
    pub(crate) rails: RailPlan,
    /// Where the Si5332 answers.
    pub(crate) clock: I2cAddr,
    /// The Si5332 input the reference arrives on.
    pub(crate) reference: Reference,
    /// The Si5332 output carrying the LMS6002D's PLL reference.
    pub(crate) lvpecl: LvpeclOutput,
    /// Revision 3: the on-board oscillator is started after the clocks are programmed.
    pub(crate) rev3: bool,
    /// On revision 3, the on-board oscillator runs (it stays off for an external
    /// reference).
    pub(crate) oscillator: bool,
    /// The gateware's chains.
    pub(crate) chains: Chains,
    /// The reference's frequency, in Hz.
    pub(crate) reference_hz: u32,
}

parts! {
    /// A power-up part.
    #[derive(Clone, Copy, Debug)]
    pub(crate) enum PowerUpPart {
        /// The PMIC and boost converter.
        Rails(PowerRails),
        /// The clock generator's identification and programming.
        Program(ClockSequence<POWER_UP_WRITES>),
        /// Its state wait.
        Active(ClockSequence<0>),
        /// The LMS6002D's power-up configuration.
        Create(Create),
        /// GPO writes, reads and waits.
        Script(Script),
    }
}

/// Where power-up is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Not started.
    Start,
    /// The rails.
    Rails,
    /// The wait after them.
    RailsSettle,
    /// The clock generator programmed.
    Program,
    /// libusdr's discarded read of FPGA register 0xC, kept so traces line up.
    Probe,
    /// The clock generator's state wait.
    Active,
    /// Revision 3: the oscillator started.
    Oscillator,
    /// The wait after the clocks.
    ClocksSettle,
    /// The RF section powered and the LMS6002D released from reset.
    Release,
    /// Its ID read, for libusdr's log.
    Identify,
    /// The wait before its configuration.
    IdentifySettle,
    /// Its power-up configuration.
    Create,
    /// The idle configuration.
    Configure,
    /// After a failure: the LED, reset and booster turned back off.
    Cleanup,
    /// Finished.
    Done,
}

/// Brings the board up from reset, after identification and the thermal policy: rails,
/// clocks, then the RF transceiver and its idle configuration (both chains off, wideband
/// RX and TX paths, external mixer off, FPGA DC correction on).
///
/// Failures after the clocks are programmed turn the LED, booster and RF chip back off,
/// over the same span as libusdr's `fail:` path. On revision 3 the oscillator is enabled
/// only after programming, so the Si5332 may report no input clock: libusdr discards
/// `si5332_init`'s result there; only that missing clock is tolerated, a bus failure or an
/// absent Si5332 still fails, a deliberate divergence.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PowerUp {
    /// The plan.
    plan: PowerUpPlan,
    /// Where it is.
    stage: Stage,
    /// The clocks failed.
    clocks_failed: bool,
    /// The LMS6002D's cache, once created.
    lms: Option<Lms6002d>,
    /// The powered board's state, once configured.
    radio: Option<Radio>,
}

impl PowerUp {
    /// Power-up with `plan`.
    pub(crate) const fn new(plan: PowerUpPlan) -> Self {
        Self {
            plan,
            stage: Stage::Start,
            clocks_failed: false,
            lms: None,
            radio: None,
        }
    }

    /// The powered board's state, once power-up has finished without a failure.
    pub(crate) const fn radio(&self) -> Option<Radio> {
        self.radio
    }

    /// The wait part of `millis` milliseconds.
    fn sleep(millis: u64) -> PowerUpPart {
        PowerUpPart::Script(Script::new().sleep(Duration::from_millis(millis)))
    }

    /// The writes turning the RF section back off, whose failures libusdr discards.
    fn cleanup(&mut self) -> PowerUpPart {
        self.stage = Stage::Cleanup;
        PowerUpPart::Script(
            Script::new()
                .fpga(FPGA, Gpo::Led.write(0))
                .fpga(FPGA, Gpo::LmsReset.write(0))
                .fpga(FPGA, Gpo::Booster.write(0))
                .every(),
        )
    }
}

impl Steps for PowerUp {
    type Part = PowerUpPart;

    fn next_part(&mut self, failed: bool) -> Option<PowerUpPart> {
        let plan = self.plan;
        if failed
            && matches!(
                self.stage,
                Stage::Program | Stage::Active | Stage::Oscillator
            )
        {
            self.clocks_failed = true;
        }
        let (stage, part) = match self.stage {
            Stage::Start => (
                Stage::Rails,
                PowerUpPart::Rails(PowerRails::new(plan.rails)),
            ),
            Stage::Rails => (Stage::RailsSettle, Self::sleep(10)),
            Stage::RailsSettle => (
                Stage::Program,
                PowerUpPart::Program(ClockSequence::program(
                    plan.clock,
                    1,
                    plan.reference,
                    plan.lvpecl,
                )),
            ),
            Stage::Program if !failed => (
                Stage::Probe,
                PowerUpPart::Script(Script::new().fpga(FPGA, FpgaAccess::Read { reg: 0xc })),
            ),
            Stage::Probe => (
                Stage::Active,
                PowerUpPart::Active(ClockSequence::wait_active(plan.clock)),
            ),
            Stage::Program | Stage::Active if plan.rev3 => (
                Stage::Oscillator,
                PowerUpPart::Script(
                    Script::new()
                        .fpga(FPGA, Gpo::EnableOscillator.write(u8::from(plan.oscillator)))
                        .sleep(Duration::from_millis(1))
                        .every(),
                ),
            ),
            Stage::Program | Stage::Active | Stage::Oscillator => {
                (Stage::ClocksSettle, Self::sleep(10))
            }
            Stage::ClocksSettle if self.clocks_failed => return Some(self.cleanup()),
            Stage::ClocksSettle => (
                Stage::Release,
                PowerUpPart::Script(
                    Script::new()
                        .fpga(FPGA, Gpo::Booster.write(1))
                        .fpga(FPGA, Gpo::Led.write(1))
                        .fpga(FPGA, Gpo::LmsReset.write(1)),
                ),
            ),
            Stage::Release | Stage::Identify | Stage::IdentifySettle | Stage::Create if failed => {
                return Some(self.cleanup());
            }
            Stage::Release => (
                Stage::Identify,
                PowerUpPart::Script(Lms6002d::read_id(Lms6002d::USDR_TARGET)),
            ),
            Stage::Identify => (Stage::IdentifySettle, Self::sleep(1)),
            Stage::IdentifySettle => {
                let (lms, create) = Lms6002d::create(Lms6002d::USDR_TARGET);
                self.lms = Some(lms);
                (Stage::Create, PowerUpPart::Create(create))
            }
            Stage::Create => {
                let mut lms = self.lms.expect("invariant: created in the stage before");
                let configure = lms
                    .set_tx_enabled(false)
                    .extend(&lms.set_rx_enabled(false))
                    .fpga(FPGA, Gpo::RxMixerEnable.write(0))
                    // `usdr_set_rx_port_switch` for LNAW: switch 0, mixer off.
                    .fpga(FPGA, Gpo::RxSwitch.write(0))
                    .fpga(FPGA, Gpo::RxMixerEnable.write(0))
                    .fpga(FPGA, Gpo::TxSwitch.write(1))
                    .extend(&lms.select_lna(Lna::Lna1))
                    .extend(&lms.select_pa(PowerAmp::Pa1))
                    .fpga(FPGA, Gpo::DcCorrection.write(1));
                self.lms = Some(lms);
                (Stage::Configure, PowerUpPart::Script(configure))
            }
            Stage::Configure => {
                if !failed {
                    let lms = self.lms.expect("invariant: created before configuring");
                    self.radio = Some(Radio::new(lms, plan.rev3, plan.chains, plan.reference_hz));
                }
                self.stage = Stage::Done;
                return None;
            }
            Stage::Cleanup | Stage::Done => return None,
        };
        self.stage = stage;
        Some(part)
    }

    fn policy<E>(&self, failure: &SequenceFailure<E>) -> Policy {
        match self.stage {
            // Revision 3's clocks may find no input before the oscillator runs.
            Stage::Active
                if self.plan.rev3 && matches!(failure, SequenceFailure::InputMissing { .. }) =>
            {
                Policy::Ignore
            }
            Stage::Program
            | Stage::Active
            | Stage::Oscillator
            | Stage::Release
            | Stage::Identify
            | Stage::Create => Policy::Continue,
            Stage::Probe | Stage::Cleanup => Policy::Ignore,
            _ => Policy::Stop,
        }
    }
}

owns_radio!(ChainPower);
