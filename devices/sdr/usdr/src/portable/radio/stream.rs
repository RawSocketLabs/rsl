//! The RF chains' part of a stream's create and destroy: libusdr's
//! `usdr_rfic_streaming_up` and `_down`, and the RX create's DC calibration
//! (`usdr_device_m2_lm6_1_create_stream`). The stream engine's part is
//! `portable::stream`'s.

use core::time::Duration;

use super::super::lms6002d::{PllTune, RxDcCalibration, TuneFault};
use super::super::script::Script;
use super::super::sequence::SequenceFailure;
use super::power::ChainPower;
use super::run::{Policy, Run, Steps, owns_radio, parts};
use super::state::{Chain, NcoChannels, Radio};
use super::tune::Tune;

/// The frequency an RX stream tunes to if none was set, in Hz.
const DEFAULT_RX_HZ: u32 = 320_000_000;

/// The wait after a chain wakes for a stream.
const WAKE: Duration = Duration::from_millis(1);

parts! {
    /// A stream's RF part.
    #[derive(Clone, Copy, Debug)]
    pub(crate) enum StreamPart {
        /// The chain's power.
        Power(Run<ChainPower>),
        /// The default RX tuning.
        Tune(Run<Tune>),
        /// The RX DC calibration.
        Calibration(RxDcCalibration),
        /// The TX PLL's retune.
        Pll(PllTune),
        /// A wait.
        Script(Script),
    }
}

/// Where a stream's RF part is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Not started.
    Start,
    /// The chain powered.
    Power,
    /// The wake-up wait.
    Wake,
    /// RX: the default tuning.
    Tune,
    /// RX: the DC calibration.
    Calibration,
    /// RX: the TX PLL tuned back to its LO.
    Retune,
    /// Finished.
    Done,
}

/// The RF chains' part of a stream create (`usdr_rfic_streaming_up`, then for RX the DC
/// calibration). Creating the RX stream powers the receiver, tunes it to 320 MHz if no
/// frequency was set (as libusdr, leaving the recorded frequency unset), and calibrates its
/// DC offsets; the calibration borrows the TX PLL, which, with a TX frequency set, is tuned
/// back to it, with any failure ignored as libusdr ignores it: the LO itself, not the
/// lowest lock a low-LO fallback found. Creating the TX stream powers the transmitter;
/// libusdr takes its wake-up wait even when powering fails.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StreamUp {
    /// The state.
    pub(crate) radio: Radio,
    /// Which chain's stream.
    chain: Chain,
    /// The chain was not running: the wake-up wait follows its power.
    waking: bool,
    /// Where it is.
    stage: Stage,
    /// The fault that ended it.
    fault: Option<TuneFault>,
}

impl StreamUp {
    /// The RF part of creating `chain`'s stream.
    pub(crate) const fn new(radio: Radio, chain: Chain) -> Self {
        Self {
            radio,
            chain,
            waking: false,
            stage: Stage::Start,
            fault: None,
        }
    }
}

impl Steps for StreamUp {
    type Part = StreamPart;

    fn next_part(&mut self, _failed: bool) -> Option<StreamPart> {
        let radio = &mut self.radio;
        loop {
            let (stage, part) = match (self.stage, self.chain) {
                (Stage::Start, chain) => {
                    let running = match chain {
                        Chain::Rx => &mut radio.rx_running,
                        Chain::Tx => &mut radio.tx_running,
                    };
                    self.waking = !*running;
                    *running = true;
                    let power = ChainPower::new(*radio, chain, true);
                    (Stage::Power, Some(StreamPart::Power(Run::new(power))))
                }
                (Stage::Power, _) => (
                    Stage::Wake,
                    self.waking
                        .then(|| StreamPart::Script(Script::new().sleep(WAKE))),
                ),
                (Stage::Wake, Chain::Rx) => {
                    let tune = (radio.rx.lo_hz == 0).then(|| {
                        let lo = radio.rx.plan.set_lo(NcoChannels::Both, DEFAULT_RX_HZ);
                        StreamPart::Tune(Run::new(Tune::new(*radio, Chain::Rx, lo)))
                    });
                    (Stage::Tune, tune)
                }
                (Stage::Tune, _) => {
                    let vga2 = (radio.rx.vga2a, radio.rx.vga2b);
                    let calibration = RxDcCalibration::new(radio.lms, radio.reference_hz, vga2);
                    (
                        Stage::Calibration,
                        Some(StreamPart::Calibration(calibration)),
                    )
                }
                (Stage::Calibration, _) => {
                    let lo = radio.tx.lo_hz;
                    let retune = (lo != 0).then(|| StreamPart::Pll(radio.tune_pll(Chain::Tx, lo)));
                    (Stage::Retune, retune)
                }
                // The transmitter's power failure is reported after the wait, and ends it.
                (Stage::Wake, Chain::Tx) | (Stage::Retune | Stage::Done, _) => (Stage::Done, None),
            };
            self.stage = stage;
            if part.is_some() || stage == Stage::Done {
                return part;
            }
        }
    }

    fn absorb(&mut self, part: &StreamPart) {
        match part {
            StreamPart::Power(run) => self.radio = run.steps().radio,
            StreamPart::Tune(run) => {
                self.radio = run.steps().radio;
                self.fault = run.fault();
                if self.fault.is_none() && !run.reported() {
                    // As libusdr, the default tuning leaves the frequency unset.
                    self.radio.rx.lo_hz = 0;
                }
            }
            StreamPart::Calibration(calibration) => {
                self.radio.lms = *calibration.chip();
                self.fault = calibration.fault();
            }
            StreamPart::Pll(tune) => self.radio.lms = *tune.chip(),
            StreamPart::Script(_) => {}
        }
        if self.fault.is_some() {
            self.stage = Stage::Done;
        }
    }

    fn policy<E>(&self, _: &SequenceFailure<E>) -> Policy {
        match (self.stage, self.chain) {
            (Stage::Power, Chain::Tx) => Policy::Continue,
            (Stage::Retune, _) => Policy::Ignore,
            _ => Policy::Stop,
        }
    }

    fn fault(&self) -> Option<TuneFault> {
        self.fault
    }
}

/// The RF chains' part of a stream destroy (`usdr_rfic_streaming_down`): the chain no
/// longer runs, and is powered down.
pub(crate) fn stream_down(mut radio: Radio, chain: Chain) -> Run<ChainPower> {
    match chain {
        Chain::Rx => radio.rx_running = false,
        Chain::Tx => radio.tx_running = false,
    }
    Run::new(ChainPower::new(radio, chain, false))
}

owns_radio!(StreamUp);
