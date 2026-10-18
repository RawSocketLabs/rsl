//! The `m2_lm6_1` board: its state, how its chips are wired, and its supply setpoints.
//! The sequences that drive it are `impl Board` blocks in sibling modules.
//!
//! The crate docs describe the board as a whole: its chips, signal paths, clock tree and
//! power-up order. This module holds the facts the sequences need about this one board:
//! which chip sits where (each chip's `usdr()` constructor) and the voltages the board
//! runs its rails at. A chip module knows what its registers mean; only `board/` decides
//! what to write to them and in what order.

use std::fmt;

use super::stream::StreamGroup;
use crate::chips::lp8758::{BuckControl, BuckVoltage, Config, Lp8758};
use crate::chips::register::run_sequence;
use crate::chips::si5332::{LvpeclOutput, Reference, Si5332};
use crate::chips::tmp114::Tmp114;
use crate::chips::tps6381x::{OutputVoltage, Tps6381x};
use crate::error::Error;
use crate::fpga::IqImbalance;
use crate::frontend::FrontEnd;
use crate::lowlevel::Bus;
use crate::portable::power::RailPlan;
use crate::portable::radio::{OwnsRadio, Radio, Run, Steps, wiring};
use crate::portable::script::Script;
use crate::reference::ReferenceClock;
#[cfg(all(feature = "mio", target_os = "linux"))]
use crate::transport::usb::BoardState;

/// A powered uSDR board and the bus it is reached through.
///
/// A `Board` exists only between power-up ([`Identified::power_up`]) and power-down
/// ([`Board::power_down`]), so holding one means the rails, clocks and LMS6002D are up.
/// Before power-up, [`Identified`] owns the bus instead.
///
/// [`Identified`]: super::power::Identified
/// [`Identified::power_up`]: super::power::Identified::power_up
pub(crate) struct Board {
    /// The hardware seam; chip drivers borrow it per call.
    pub(super) bus: Box<dyn Bus>,

    /// The RF state: the LMS6002D's cache (its registers are not read back), each chain's
    /// settings, the rates and the gateware's chains. The board's RF sequences are portable
    /// machines that take a copy and give it back (`portable::radio`).
    pub(super) radio: Radio,

    /// The DSP chains' digital corrections, which only the blocking board's calibration
    /// and stream creates use.
    pub(super) corrections: Corrections,

    /// The reference every clock derives from; its frequency feeds each sample-clock plan
    /// and synthesizer tune (libusdr's `fref`), as the radio state's copy.
    pub(super) reference: ReferenceClock,

    /// The stream group, while one exists.
    pub(super) streams: Option<StreamGroup>,

    /// The front end on the M.2 connector, once the probe brought one up (libusdr's
    /// `fe`).
    pub(super) front_end: Option<FrontEnd>,
}

/// The DSP chains' digital corrections as libusdr keeps them (its `rx_corr` and
/// `tx_corr`): the blocking board's, and a native handle's after a hand-over.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Corrections {
    /// The RX IQ imbalance correction, which stream creation writes again.
    pub(crate) rx: IqImbalance,

    /// The TX IQ imbalance correction, which TX stream creation writes again.
    pub(crate) tx: IqImbalance,

    /// The LO-leakage calibration's DC offsets, I then Q (libusdr's `tx_corr.dc_i` and
    /// `dc_q`): where its searches last moved each, kept from one calibration to the next.
    pub(crate) tx_lo_dc: [i16; 2],
}

impl fmt::Debug for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Board")
            .field("radio", &self.radio)
            .field("corrections", &self.corrections)
            .field("streams", &self.streams)
            .field("front_end", &self.front_end)
            .finish_non_exhaustive()
    }
}

impl Board {
    /// The LP8758 PMIC: four step-down converters, among them the FPGA I/O bank rail
    /// ([`Self::VGPIO`]) and the LMS6002D's I/O rail (`wiring::LMS_VIO_NORMAL`).
    pub(super) const PMIC: Lp8758 = Lp8758::usdr();

    /// The TMP114 temperature sensor, the only temperature the driver reads. The thermal
    /// policy acts on it before power-up, so it must answer from reset.
    pub(super) const TEMP: Tmp114 = Tmp114::usdr();

    /// The Si5332 clock generator: the LMS6002D's PLL reference, the RX and TX sample
    /// clocks, and the external mixer's LO. See the crate docs for its output wiring.
    pub(super) const CLOCK: Si5332 = Si5332::usdr();

    /// The TPS63811 buck-boost converter, programmed to [`Self::BOOST_VOLTAGE`] before
    /// the clocks start.
    pub(super) const BOOST: Tps6381x = Tps6381x::usdr();

    /// FPGA GPIO bank voltage (PMIC buck 1): 1.8 V, which libusdr chose "to be compatible
    /// with xSDR", another Wavelet Lab board.
    pub(super) const VGPIO: BuckVoltage = BuckVoltage::from_millivolts(1800);

    /// Boost converter output, 3.45 V, the same whichever level the TPS63811's VSEL pin
    /// sits at.
    pub(super) const BOOST_VOLTAGE: OutputVoltage = OutputVoltage::from_millivolts(3450);

    /// The rails power-up brings up, in `usdr_init`'s order. The PMIC configuration is
    /// libusdr's `lp8758_ss(0)`: spread spectrum off; the same byte also sets the 105 °C
    /// die-temperature warning and drops the EN-pin pull-downs. Then channels 1 and 3 are
    /// set, all four run in forced PWM, and the boost converter starts in forced PWM.
    pub(super) const RAILS: RailPlan = RailPlan {
        pmic: Self::PMIC.dev(),
        config: Config::new().with_warn_at_105c(true),
        gpio: Self::VGPIO,
        lms_io: wiring::LMS_VIO_NORMAL,
        control: BuckControl::new()
            .with_enabled(true)
            .with_discharge_when_off(true)
            .with_forced_pwm(true),
        boost: Self::BOOST.dev(),
        boost_forced_pwm: true,
        boost_voltage: Self::BOOST_VOLTAGE,
    };
}

impl Board {
    /// What a native USB handle carries on from this board, and the bus to hand over.
    #[cfg(all(feature = "mio", target_os = "linux"))]
    pub(crate) fn hand_over(&mut self) -> (BoardState, &mut dyn Bus) {
        let state = BoardState {
            rev3: self.radio.rev3,
            chains: self.radio.chains,
            radio: Some(self.radio),
            corrections: self.corrections,
        };
        (state, self.bus.as_mut())
    }

    /// The Si5332 input `reference` selects on a board of this revision (`si5332_init`'s
    /// `ext_in2`).
    pub(crate) const fn si5332_reference(reference: ReferenceClock, rev3: bool) -> Reference {
        if reference.uses_input2(rev3) {
            Reference::Input2
        } else {
            Reference::Oscillator
        }
    }

    /// Which of the Si5332's outputs 0 and 1 carries the LMS6002D's PLL reference (LVPECL);
    /// the other is the RX sample clock. Revision 3 swapped them.
    pub(super) const fn lvpecl(rev3: bool) -> LvpeclOutput {
        wiring::lvpecl(rev3)
    }

    /// Runs `script` over the bus.
    pub(super) fn run_script(&mut self, mut script: Script) -> Result<(), Error> {
        run_sequence(self.bus.as_mut(), &mut script)
    }

    /// Runs a composite RF operation over the bus with the board's RF state, and takes the
    /// state back, failed or not: its first failure, or the fault that ended it.
    pub(super) fn run_radio<S: Steps + OwnsRadio>(&mut self, steps: S) -> Result<(), Error> {
        let mut run = Run::new(steps);
        let ran = run_sequence(self.bus.as_mut(), &mut run);
        self.radio = run.radio();
        run.fault().map_or(ran, |fault| Err(fault.into()))
    }
}
