//! Opening a LimeSDR-mini: libusdr's `limesdr_init` (`device/u3_limesdr/limesdr_ctrl.c`).
//!
//! The reference clock is measured (`limesdr_detect_refclk`), the LMS7002M's reset pulsed,
//! its version word required to be exactly [`VERSION`], its reset programme loaded
//! (`lms7002m_create`: `lms7002m`'s [`Create`]) and streaming stopped
//! (`limesdr_stop_streaming`). `lms7002m_init`, between the reset and the version read,
//! only fills libusdr's state.

use core::time::Duration;

use lms7002m::{self, Create, Reg, ResetPlan, Version, spi_read};
use rsl_lowlevel::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::fpga::{
    self, DETECTOR_0063, DETECTOR_CONTROL, DETECTOR_COUNT, DETECTOR_DONE, DETECTOR_START,
    DETECTOR_STATUS, RX_ENABLE, STREAM_CONTROL, TX_ENABLE,
};
use super::lsop::LsOp;
use super::reference::{measured_hz, nominal_hz};

/// The LMS7002M's `lms7002m_create` arguments on this board: `LMS7_BUS_ADDR` (SPI bus 0),
/// no LDO enables, no shared reference buffer.
pub(crate) const LMS: ResetPlan = ResetPlan {
    target: SpiAddr(0),
    ldo_mask: 0,
    shared_reference: false,
};

/// The only LMS7002M version word `limesdr_init` accepts (`-EIO` otherwise): `VER` 7,
/// `REV` 1, `MASK` 1. `lms7002m_create` alone would accept any mask.
pub(crate) const VERSION: Version = Version::from_raw(0x3841);

/// How many times libusdr reads the detector's status before giving up.
pub(crate) const POLLS: u8 = 10;

/// libusdr's wait after each status read that finds the measurement incomplete.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The writes that start a measurement: stop, clear 0x0063, start.
const START: [LsOp; 3] = [
    LsOp::Write {
        reg: DETECTOR_CONTROL,
        value: 0,
    },
    LsOp::Write {
        reg: DETECTOR_0063,
        value: 0,
    },
    LsOp::Write {
        reg: DETECTOR_CONTROL,
        value: DETECTOR_START,
    },
];

/// What a `LimeSDR` sequence asks its executor to do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Perform a low-level operation and report what it returned.
    Op(LsOp),

    /// Wait at least this long, then report any value.
    Sleep(Duration),
}

/// Why the open stopped. `E` is the executor's error, kept unchanged.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure<E> {
    /// A gateware access or LMS7002M exchange failed, or the LMS7002M's version is wrong.
    Sequence(SequenceFailure<E>),

    /// The LMS7002M's reset pulse failed.
    Reset(E),

    /// The frequency detector did not finish within libusdr's [`POLLS`] polls. libusdr
    /// reads whatever count the registers hold anyway; this port stops.
    DetectorStalled,

    /// The measured reference is no nominal reference (`-EFAULT`).
    UnknownReference {
        /// The measurement, in hertz.
        measured_hz: u64,
    },
}

impl<E> From<SequenceFailure<E>> for Failure<E> {
    fn from(failure: SequenceFailure<E>) -> Self {
        Self::Sequence(failure)
    }
}

/// Where the open is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Writing [`START`]'s entry.
    Start(usize),
    /// Reading the detector's status, the poll numbered from 0.
    Poll(u8),
    /// Waiting after the poll numbered.
    PollWait(u8),
    /// Reading the count.
    Count,
    /// Pulsing the LMS7002M's reset.
    Reset,
    /// Reading the LMS7002M's version.
    Version,
    /// Running `lms7002m_create`.
    Create,
    /// Reading stream control.
    StreamRead,
    /// Writing stream control back with streaming off.
    StreamWrite(u16),
    /// Finished.
    Done,
    /// Stopped by a failure; nothing more is issued.
    Failed,
}

/// `limesdr_init`, one [`Action`] at a time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Open {
    /// The step [`Open::next`] names.
    step: Step,

    /// The LMS7002M's part.
    create: Create,

    /// The nominal reference, once recognised.
    reference_hz: Option<u32>,
}

impl Open {
    /// Starts the open; nothing has been accessed yet.
    pub(crate) fn new() -> Self {
        Self {
            step: Step::Start(0),
            create: Create::new(LMS),
            reference_hz: None,
        }
    }

    /// The nominal reference frequency in hertz, once measured and recognised.
    pub(crate) const fn reference_hz(&self) -> Option<u32> {
        self.reference_hz
    }

    /// The action to perform next; `None` once the open finished or failed.
    pub(crate) fn next(&self) -> Option<Action> {
        match self.step {
            Step::PollWait(_) => Some(Action::Sleep(POLL_INTERVAL)),
            Step::Create => self.create.next().map(chip_action),
            step => step.op().map(Action::Op),
        }
    }

    /// Applies the outcome of [`Open::next`]'s action. A failure ends the open; an outcome
    /// with no action pending is ignored. A wait cannot fail.
    ///
    /// # Errors
    ///
    /// The open's [`Failure`].
    pub(crate) fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), Failure<E>> {
        let step = self.step;
        let next = match step {
            Step::Create => self.create_step(result),
            Step::PollWait(poll) if poll + 1 < POLLS => Ok(Step::Poll(poll + 1)),
            Step::PollWait(_) => Err(Failure::DetectorStalled),
            _ => {
                let Some(op) = step.op() else {
                    return Ok(());
                };
                result
                    .map_err(|source| op_failure(op, source))
                    .and_then(|value| self.advance(step, value))
            }
        };
        self.step = *next.as_ref().unwrap_or(&Step::Failed);
        next.map(|_| ())
    }

    /// Runs one `lms7002m_create` exchange; the step after it.
    fn create_step<E>(&mut self, result: Result<u32, E>) -> Result<Step, Failure<E>> {
        self.create.complete(result)?;
        Ok(if self.create.next().is_some() {
            Step::Create
        } else {
            Step::StreamRead
        })
    }

    /// The step after `step`'s operation returned `value`.
    fn advance<E>(&mut self, step: Step, value: u32) -> Result<Step, Failure<E>> {
        Ok(match step {
            Step::Start(index) if index + 1 < START.len() => Step::Start(index + 1),
            Step::Start(_) => Step::Poll(0),
            Step::Poll(_) if value & u32::from(DETECTOR_DONE) != 0 => Step::Count,
            Step::Poll(poll) => Step::PollWait(poll),
            Step::Count => {
                let measured_hz = measured_hz(value);
                let nominal =
                    nominal_hz(measured_hz).ok_or(Failure::UnknownReference { measured_hz })?;
                self.reference_hz = Some(nominal);
                Step::Reset
            }
            Step::Reset => Step::Version,
            Step::Version if value == u32::from(VERSION.to_raw()) => Step::Create,
            Step::Version => {
                return Err(Failure::Sequence(SequenceFailure::ChipId {
                    chip: lms7002m::NAME,
                    expected: VERSION.to_raw().into(),
                    found: value,
                }));
            }
            Step::StreamRead => {
                let [lo, hi, ..] = value.to_le_bytes();
                Step::StreamWrite(u16::from_le_bytes([lo, hi]) & !(RX_ENABLE | TX_ENABLE))
            }
            Step::StreamWrite(_) => Step::Done,
            // Steps without an operation of their own.
            Step::PollWait(_) | Step::Create | Step::Done | Step::Failed => step,
        })
    }
}

impl Step {
    /// The operation this step performs, for the steps that perform one of their own.
    fn op(self) -> Option<LsOp> {
        Some(match self {
            Step::Start(index) => START[index],
            Step::Poll(_) => LsOp::Read {
                reg: DETECTOR_STATUS,
            },
            Step::Count => LsOp::ReadWide {
                reg: DETECTOR_COUNT,
            },
            Step::Reset => LsOp::Lms7Reset,
            Step::Version => spi(version_read()),
            Step::StreamRead => LsOp::Read {
                reg: STREAM_CONTROL,
            },
            Step::StreamWrite(value) => LsOp::Write {
                reg: STREAM_CONTROL,
                value,
            },
            Step::PollWait(_) | Step::Create | Step::Done | Step::Failed => return None,
        })
    }
}

/// The SPI exchange that reads the LMS7002M's version.
fn version_read() -> FpgaAccess {
    spi_read(LMS.target, Reg::Version)
}

/// The operation an LMS7002M exchange is.
fn spi(access: FpgaAccess) -> LsOp {
    match access {
        FpgaAccess::Spi { word, .. } => LsOp::Spi { word },
        FpgaAccess::Read { .. } | FpgaAccess::Write { .. } => {
            unreachable!("invariant: LMS7002M accesses are SPI exchanges")
        }
    }
}

/// The action an `lms7002m` sequence's action is.
fn chip_action(action: SequenceAction) -> Action {
    match action {
        SequenceAction::Fpga(access) => Action::Op(spi(access)),
        SequenceAction::Sleep(duration) => Action::Sleep(duration),
        SequenceAction::Access(_) => unreachable!("invariant: the LMS7002M is on SPI, not I2C"),
    }
}

/// The failure of `op`, which the executor reported as `source`.
fn op_failure<E>(op: LsOp, source: E) -> Failure<E> {
    match op {
        LsOp::Lms7Reset => Failure::Reset(source),
        LsOp::Spi { .. } | LsOp::Write { .. } | LsOp::Read { .. } | LsOp::ReadWide { .. } => {
            SequenceFailure::FpgaTransport {
                chip: if matches!(op, LsOp::Spi { .. }) {
                    lms7002m::NAME
                } else {
                    fpga::NAME
                },
                access: op
                    .fpga_access()
                    .expect("invariant: every operation but the reset is an access"),
                source,
            }
            .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::Access;

    use super::*;

    /// A board that answers as libusdr expects: the detector done at the `done_at`th poll
    /// (`None`: never), a 40 MHz count, version 0x3841, stream control `stream`.
    struct Board {
        done_at: Option<u8>,
        polls: u8,
        version: u32,
        stream: u16,
        sleeps: u32,
        stream_written: Option<u16>,
    }

    impl Board {
        fn healthy() -> Self {
            Self {
                done_at: Some(0),
                polls: 0,
                version: 0x3841,
                stream: 0xfff3,
                sleeps: 0,
                stream_written: None,
            }
        }

        /// The reply to `action`.
        fn answer(&mut self, action: Action) -> u32 {
            match action {
                Action::Sleep(_) => {
                    self.sleeps += 1;
                    0
                }
                Action::Op(LsOp::Read {
                    reg: DETECTOR_STATUS,
                }) => {
                    let done = self.done_at == Some(self.polls);
                    self.polls += 1;
                    if done { DETECTOR_DONE.into() } else { 0 }
                }
                Action::Op(LsOp::ReadWide { .. }) => 6_710_884,
                Action::Op(LsOp::Spi { word }) if word >> 31 == 0 => self.version,
                Action::Op(LsOp::Read {
                    reg: STREAM_CONTROL,
                }) => self.stream.into(),
                Action::Op(LsOp::Write {
                    reg: STREAM_CONTROL,
                    value,
                }) => {
                    self.stream_written = Some(value);
                    0
                }
                Action::Op(_) => 0,
            }
        }
    }

    /// Runs `open` on `board` to its end.
    fn run(open: &mut Open, board: &mut Board) -> Result<usize, Failure<()>> {
        let mut actions = 0;
        while let Some(action) = open.next() {
            actions += 1;
            let reply = board.answer(action);
            open.complete::<()>(Ok(reply))?;
        }
        Ok(actions)
    }

    #[test]
    fn a_healthy_board_opens_with_streaming_stopped_and_nothing_else_cleared() {
        let (mut open, mut board) = (Open::new(), Board::healthy());
        // Three starts, one poll, the count, the reset, the version, create's eleven, the
        // stream read and write.
        assert_eq!(run(&mut open, &mut board), Ok(3 + 1 + 1 + 1 + 1 + 11 + 2));
        assert_eq!(open.reference_hz(), Some(40_000_000));
        assert_eq!(
            board.stream_written,
            Some(0xfff0),
            "only RX_EN and TX_EN are cleared"
        );
    }

    #[test]
    fn each_incomplete_poll_is_followed_by_a_wait() {
        let mut board = Board {
            done_at: Some(3),
            ..Board::healthy()
        };
        assert!(run(&mut Open::new(), &mut board).is_ok());
        assert_eq!((board.polls, board.sleeps), (4, 3));
    }

    #[test]
    fn a_detector_that_never_finishes_stops_after_ten_polls_and_waits() {
        let mut board = Board {
            done_at: None,
            ..Board::healthy()
        };
        let mut open = Open::new();
        assert_eq!(run(&mut open, &mut board), Err(Failure::DetectorStalled));
        assert_eq!((board.polls, board.sleeps), (POLLS, POLLS.into()));
        assert_eq!(open.next(), None, "nothing follows");
    }

    #[test]
    fn only_version_word_0x3841_is_accepted() {
        let mut board = Board {
            version: 0x3840,
            ..Board::healthy()
        };
        let mut open = Open::new();
        assert_eq!(
            run(&mut open, &mut board),
            Err(Failure::Sequence(SequenceFailure::ChipId {
                chip: "LMS7002M",
                expected: 0x3841,
                found: 0x3840,
            })),
            "mask 0 is a supported chip to lms7002m_create, not to this board"
        );
        assert_eq!(open.next(), None);
    }

    #[test]
    fn failures_name_what_they_struck() {
        let mut open = Open::new();
        let failure = open.complete(Err(())).unwrap_err();
        assert_eq!(
            failure,
            Failure::Sequence(SequenceFailure::FpgaTransport {
                chip: "FPGA",
                access: FpgaAccess::Write {
                    reg: u32::from(DETECTOR_CONTROL),
                    value: 0
                },
                source: (),
            })
        );
        assert_eq!(open.next(), None, "nothing follows a failure");

        let mut open = run_until(Action::Op(LsOp::Lms7Reset));
        assert_eq!(open.complete(Err(())), Err(Failure::Reset(())));

        let mut open = run_until(Action::Op(spi(version_read())));
        let Err(Failure::Sequence(failure)) = open.complete(Err(())) else {
            panic!("a failed version read is an access failure");
        };
        assert_eq!(failure.access(), Some(("LMS7002M", Access::Read, 0x002f)));
    }

    /// An open on a healthy board, run until `action` is next.
    fn run_until(action: Action) -> Open {
        let (mut open, mut board) = (Open::new(), Board::healthy());
        while let Some(next) = open.next().filter(|&next| next != action) {
            let reply = board.answer(next);
            open.complete::<()>(Ok(reply)).expect("a healthy board");
        }
        assert_eq!(open.next(), Some(action), "the open reaches it");
        open
    }

    #[test]
    fn a_wait_reported_as_failed_still_counts_as_waited() {
        let mut open = Open::new();
        let mut board = Board {
            done_at: Some(1),
            ..Board::healthy()
        };
        while let Some(action) = open.next() {
            let reply = board.answer(action);
            let result = if matches!(action, Action::Sleep(_)) {
                Err(())
            } else {
                Ok(reply)
            };
            open.complete(result).expect("a wait cannot fail");
        }
        assert_eq!(open.reference_hz(), Some(40_000_000), "the open finished");
    }
}
