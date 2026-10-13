//! Preparing the LimeSDR-mini to stream: libusdr's `limesdr_prepare_streaming`
//! (`device/u3_limesdr/limesdr_ctrl.c`), which its stream initialisation runs before the
//! transport's. A 1 MS/s rate both ways when none is set, then the LMS7002M streamed up
//! on channel A in both directions (`lms7002m_streaming_up`), its bands and antenna
//! switches following the LOs.

use hal::{Sequence, SequenceAction, SequenceFailure};
use lms7002m::Mac;
use lms7002m::ctrl::{Control, ControlError, Operation, StreamFlags};

use super::frontend::LimeFrontend;
use super::rate::SetSampleRate;

/// The rate libusdr sets both ways when none is (`default_rate`).
pub(crate) const DEFAULT_RATE: u32 = 1_000_000;

/// Where the preparation is.
#[derive(Clone, Copy, Debug)]
enum Stage {
    /// Setting the default rate.
    Rate(SetSampleRate),
    /// Streaming up.
    Up(Operation<LimeFrontend>),
}

/// `limesdr_prepare_streaming`, as a sequence.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PrepareStreaming {
    /// Where it is.
    stage: Stage,
    /// An access failed, which ends it.
    failed: bool,
}

impl PrepareStreaming {
    /// Starts preparing `control`'s chip to stream.
    pub(crate) fn new(control: Control) -> Self {
        let stage = if control.clocks().cgen == 0 {
            Stage::Rate(SetSampleRate::new(control, DEFAULT_RATE, DEFAULT_RATE))
        } else {
            Stage::Up(stream_up(control))
        };
        let mut prepare = Self {
            stage,
            failed: false,
        };
        prepare.settle();
        prepare
    }

    /// The control state, as the preparation has left it so far.
    pub(crate) const fn control(&self) -> &Control {
        match &self.stage {
            Stage::Rate(rate) => rate.control(),
            Stage::Up(up) => up.control(),
        }
    }

    /// The streaming up's refusal, once it refused. libusdr ignores the default rate's
    /// result, so its refusal is not one.
    pub(crate) fn refusal(&self) -> Option<ControlError> {
        match &self.stage {
            Stage::Rate(_) => None,
            Stage::Up(up) => up.outcome().and_then(Result::err),
        }
    }

    /// Moves from a finished default rate, whatever its outcome but a failed access, to
    /// streaming up.
    fn settle(&mut self) {
        let Stage::Rate(rate) = self.stage else {
            return;
        };
        if !self.failed && rate.next().is_none() {
            self.stage = Stage::Up(stream_up(*rate.control()));
        }
    }
}

/// Channel A streamed up in both directions with no stream flags, after both directions'
/// run flags are set as `limesdr_prepare_streaming` sets them.
fn stream_up(mut control: Control) -> Operation<LimeFrontend> {
    control.set_running([true, false], [true, false]);
    let channel_a = (Mac::A, StreamFlags::default());
    control.streaming_up(LimeFrontend, channel_a, Some(channel_a))
}

impl Sequence for PrepareStreaming {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        match &self.stage {
            Stage::Rate(rate) => rate.next(),
            Stage::Up(up) => up.next(),
        }
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.next().is_none() {
            return Ok(());
        }
        let completed = match &mut self.stage {
            Stage::Rate(rate) => rate.complete(result),
            Stage::Up(up) => up.complete(result),
        };
        self.failed = completed.is_err();
        self.settle();
        completed
    }
}
