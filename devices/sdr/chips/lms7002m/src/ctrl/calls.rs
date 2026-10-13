//! A queue of chip operations run one after another, each built from the chip's cache only
//! when its turn comes: several `lms7002m_*` calls chained with libusdr's
//! `res = res ? res : ...`, so a failure ends the queue with the cache as libusdr leaves
//! it (later calls never built).

use bnb::u3;
use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure};

use crate::batch::Batch;
use crate::chip::{
    LmlConfig, LmlMap, Lms7002m, LpfParams, NoChannel, RbbMode, RbbPath, RxGainStage, RxMode,
    Synthesizer, TbbMode, TbbPath, Tsp, TxGainStage, TxMode, TxPath,
};
use crate::regs::{Mac, RxPath};

/// The most calls one queue holds: `lms7002m_samplerate`'s tail with both channels running
/// in both directions.
const CAPACITY: usize = 32;

/// One chip operation, as the control code calls it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ChipCall {
    /// [`Lms7002m::mac_set`].
    MacSet(Mac),
    /// [`Lms7002m::afe_enable`].
    AfeEnable([bool; 2], [bool; 2]),
    /// [`Lms7002m::tsp_enable`].
    TspEnable(Tsp, bool),
    /// [`Lms7002m::tsp_int_dec`].
    TspIntDec(Tsp, u32),
    /// [`Lms7002m::rxtsp_dc_corr`].
    RxtspDcCorr(bool, u3),
    /// [`Lms7002m::tsp_cmix`].
    TspCmix(Tsp, i32),
    /// [`Lms7002m::limelight_configure`].
    LimelightConfigure(LmlConfig),
    /// [`Lms7002m::limelight_switch_rx_mode`].
    LimelightSwitchRxMode(LmlConfig),
    /// [`Lms7002m::limelight_map`].
    LimelightMap([bool; 2], [LmlMap; 2]),
    /// [`Lms7002m::rfe_path`].
    RfePath(RxPath, RxMode),
    /// [`Lms7002m::trf_path`].
    TrfPath(TxPath, TxMode),
    /// [`Lms7002m::rfe_gain`].
    RfeGain(RxGainStage, i32),
    /// [`Lms7002m::trf_gain`].
    TrfGain(TxGainStage, i32),
    /// [`Lms7002m::rbb_path`].
    RbbPath(RbbPath, RbbMode),
    /// [`Lms7002m::rbb_lpf`].
    RbbLpf(LpfParams),
    /// [`Lms7002m::rbb_pga`].
    RbbPga(i32),
    /// [`Lms7002m::tbb_path`].
    TbbPath(TbbPath, TbbMode),
    /// [`Lms7002m::tbb_lpf`].
    TbbLpf(LpfParams),
    /// [`Lms7002m::tbb_gain`].
    TbbGain(i8),
    /// [`Lms7002m::sxx_disable`].
    SxxDisable(Synthesizer),
}

impl ChipCall {
    /// The call's writes, built from (and updating) `chip`'s cache.
    fn build(self, chip: &mut Lms7002m) -> Result<Batch, NoChannel> {
        Ok(match self {
            Self::MacSet(mac) => chip.mac_set(mac),
            Self::AfeEnable(rx, tx) => chip.afe_enable(rx, tx),
            Self::TspEnable(tsp, enable) => chip.tsp_enable(tsp, enable),
            Self::TspIntDec(tsp, order) => chip.tsp_int_dec(tsp, order)?,
            Self::RxtspDcCorr(bypass, window) => chip.rxtsp_dc_corr(bypass, window)?,
            Self::TspCmix(tsp, frequency) => chip.tsp_cmix(tsp, frequency)?,
            Self::LimelightConfigure(config) => chip.limelight_configure(config),
            Self::LimelightSwitchRxMode(config) => chip.limelight_switch_rx_mode(config),
            Self::LimelightMap(siso, maps) => chip.limelight_map(siso, maps),
            Self::RfePath(path, mode) => chip.rfe_path(path, mode)?,
            Self::TrfPath(path, mode) => chip.trf_path(path, mode)?,
            Self::RfeGain(stage, gain) => chip.rfe_gain(stage, gain).0,
            Self::TrfGain(stage, gain) => chip.trf_gain(stage, gain)?.0,
            Self::RbbPath(path, mode) => chip.rbb_path(path, mode),
            Self::RbbLpf(params) => chip.rbb_lpf(params),
            Self::RbbPga(gain) => chip.rbb_pga(gain),
            Self::TbbPath(path, mode) => chip.tbb_path(path, mode),
            Self::TbbLpf(params) => chip.tbb_lpf(params),
            Self::TbbGain(gain) => chip.tbb_gain(gain),
            Self::SxxDisable(synthesizer) => chip.sxx_disable(synthesizer),
        })
    }
}

/// The queue. Its owner keeps the chip and passes it to [`Calls::start`] and
/// [`Calls::complete`], so the calls see the cache as earlier ones left it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Calls {
    /// The calls; the first `len` are used.
    queue: [Option<ChipCall>; CAPACITY],

    /// How many it holds.
    len: usize,

    /// The next call to build.
    at: usize,

    /// The current call's writes.
    batch: Option<Batch>,

    /// A call found no bank selected (libusdr's `-EINVAL`), ending the queue.
    refused: bool,

    /// A write failed, ending the queue.
    failed: bool,
}

impl Calls {
    /// An empty queue.
    pub(crate) const fn new() -> Self {
        Self {
            queue: [None; CAPACITY],
            len: 0,
            at: 0,
            batch: None,
            refused: false,
            failed: false,
        }
    }

    /// Appends a call.
    ///
    /// # Panics
    ///
    /// Past the capacity: every queue the control code builds is bounded.
    pub(crate) fn push(&mut self, call: ChipCall) -> &mut Self {
        assert!(self.len < CAPACITY, "invariant: queues fit their capacity");
        self.queue[self.len] = Some(call);
        self.len += 1;
        self
    }

    /// Builds calls until one has a write pending or the queue ends; call it once the
    /// queue is complete, before [`Sequence`]-style use.
    pub(crate) fn start(&mut self, chip: &mut Lms7002m) {
        while !self.refused && self.batch.is_none_or(|batch| batch.next().is_none()) {
            let Some(call) = self.queue[..self.len].get(self.at).copied().flatten() else {
                self.batch = None;
                return;
            };
            self.at += 1;
            match call.build(chip) {
                Ok(batch) => self.batch = Some(batch),
                Err(NoChannel) => self.refused = true,
            }
        }
    }

    /// Whether a call found no bank selected.
    pub(crate) const fn refused(&self) -> bool {
        self.refused
    }

    /// The next action.
    pub(crate) fn next(&self) -> Option<SequenceAction> {
        if self.failed || self.refused {
            return None;
        }
        self.batch.as_ref().and_then(Sequence::next)
    }

    /// Applies the outcome of [`Self::next`]'s action, then builds what follows.
    pub(crate) fn complete<E>(
        &mut self,
        chip: &mut Lms7002m,
        result: Result<u32, E>,
    ) -> Result<(), SequenceFailure<E>> {
        if self.next().is_none() {
            return Ok(());
        }
        if let Some(batch) = &mut self.batch {
            batch.complete(result).inspect_err(|_| self.failed = true)?;
        }
        self.start(chip);
        Ok(())
    }
}
