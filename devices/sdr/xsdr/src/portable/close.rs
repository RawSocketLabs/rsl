//! Closing an XSDR: libusdr's `xsdr_dtor` (`device/m2_lm7_1/xsdr_ctrl.c`) on a board
//! without streams or an LMS8001.
//!
//! The LMS7002M's streaming is brought down (`xsdr_rfic_streaming_down` for both
//! directions, `lms7002m_streaming_down` in `lms7002m_ctrl.c`), the chip destroyed
//! (`lms7002m_destroy`), its power and LDOs and the LED switched off, and the TX MMCM
//! powered down. The chip operations come from `lms7002m`; the order, and which
//! failures end the close, are the family's.

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};
use lms7002m::{Batch, Lms7002m, Mac, RxMode, RxPath, Tsp, TxMode, TxPath};

use super::fpga::{self, Gpo, TX_MMCM_POWER_DOWN};
use super::outcome::outcome;

/// How many of [`Close`]'s chip batches, from the first, may fail without ending the close:
/// `lms7002m_streaming_down` ignores the results of its bank selection, processor and
/// front-end writes and returns only `lms7002m_afe_enable`'s.
const IGNORED: usize = 5;

/// What `xsdr_dtor` writes after the chip: LMS7002M power control, external LDOs and LED
/// off, then the TX MMCM down.
const BOARD: [FpgaAccess; 4] = [
    Gpo::LmsPower.write(0),
    Gpo::LmsLdo.write(0),
    Gpo::Led.write(0),
    TX_MMCM_POWER_DOWN,
];

/// The directions a streaming down brings down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Down {
    /// RX alone (`RFIC_LMS7_RX`), as an RX stream's destroy does.
    Rx,

    /// TX alone (`RFIC_LMS7_TX`), as a TX stream's destroy does.
    Tx,

    /// Both (`RFIC_LMS7_RX | RFIC_LMS7_TX`), as `xsdr_dtor` does.
    Both,
}

/// `lms7002m_streaming_down`'s chip writes for `down`, from the chip's cached state: both
/// banks selected, the RX processor and front ends off unless [`Down::Tx`], the TX
/// processor and front ends off unless [`Down::Rx`] (`None` where skipped), then the AFE
/// on for the channels still running (`running`, RX then TX, as the run flags stand
/// before the streaming down; the directions brought down count as stopped).
pub(crate) fn streaming_down(
    chip: &mut Lms7002m,
    down: Down,
    running: ([bool; 2], [bool; 2]),
) -> [Option<Batch>; 6] {
    let select = chip.mac_set(Mac::Both);
    let both_banks = "invariant: both banks are selected just before";
    let (mut rx_running, mut tx_running) = running;
    let (mut rx_tsp, mut rfe, mut tx_tsp, mut trf) = (None, None, None, None);
    if down != Down::Tx {
        rx_tsp = Some(chip.tsp_enable(Tsp::Rx, false));
        rfe = Some(
            chip.rfe_path(RxPath::None, RxMode::Disable)
                .expect(both_banks),
        );
        rx_running = [false; 2];
    }
    if down != Down::Rx {
        tx_tsp = Some(chip.tsp_enable(Tsp::Tx, false));
        trf = Some(
            chip.trf_path(TxPath::Mute, TxMode::Disable)
                .expect(both_banks),
        );
        tx_running = [false; 2];
    }
    let afe = chip.afe_enable(rx_running, tx_running);
    [Some(select), rx_tsp, rfe, tx_tsp, trf, Some(afe)]
}

/// `xsdr_dtor` on an XSDR, from the chip's cached state.
///
/// A failed chip write abandons the rest of its batch (`lms7002m_spi_post`); in the first
/// [`IGNORED`] batches the close then carries on, as libusdr's streaming-down does, without
/// reporting it. Any other failure ends the close, as `xsdr_dtor` chains its steps on
/// success.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Close {
    /// The chip's writes in order: streaming-down's bank selection, RX processor off, RX
    /// front ends off, TX processor off, TX front ends off and AFE off, then destroy's.
    chip: [Batch; 7],

    /// The [`BOARD`] writes completed.
    board: usize,

    /// A step failed; nothing more is issued.
    failed: bool,
}

impl Close {
    /// Plans the close of the chip in state `chip`.
    pub(crate) fn new(mut chip: Lms7002m) -> Self {
        let down = streaming_down(&mut chip, Down::Both, ([false; 2], [false; 2]))
            .map(|batch| batch.expect("invariant: both directions come down"));
        let [select, rx_tsp, rfe, tx_tsp, trf, afe] = down;
        let destroy = chip.destroy();
        Self {
            chip: [select, rx_tsp, rfe, tx_tsp, trf, afe, destroy],
            board: 0,
            failed: false,
        }
    }

    /// The chip batch with a write pending, and its index.
    fn pending(&mut self) -> Option<(usize, &mut Batch)> {
        self.chip
            .iter_mut()
            .enumerate()
            .find(|(_, batch)| batch.next().is_some())
    }
}

impl Sequence for Close {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.chip.iter().find_map(Sequence::next).or_else(|| {
            BOARD
                .get(self.board)
                .map(|&access| SequenceAction::Fpga(access))
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.failed {
            return Ok(());
        }
        if let Some((index, batch)) = self.pending() {
            return match batch.complete(result) {
                Err(_) if index < IGNORED => Ok(()),
                outcome => outcome.inspect_err(|_| self.failed = true),
            };
        }
        let Some(&access) = BOARD.get(self.board) else {
            return Ok(());
        };
        outcome(SequenceAction::Fpga(access), result, fpga::NAME)
            .inspect_err(|_| self.failed = true)?;
        self.board += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::{Access, SpiAddr};

    use super::*;

    /// Runs `close`, failing the exchanges whose (zero-based) positions `failing` lists;
    /// returns the words or register writes issued and the outcome.
    fn run(close: &mut Close, failing: &[usize]) -> (usize, Result<(), SequenceFailure<()>>) {
        let mut issued = 0;
        while close.next().is_some() {
            let result = if failing.contains(&issued) {
                Err(())
            } else {
                Ok(0)
            };
            issued += 1;
            if let Err(failure) = close.complete(result) {
                return (issued, Err(failure));
            }
        }
        (issued, Ok(()))
    }

    #[test]
    fn a_never_created_chip_gets_its_banks_selected_first() {
        let mut close = Close::new(Lms7002m::new(SpiAddr(0)));
        assert!(
            matches!(
                close.next(),
                Some(SequenceAction::Fpga(FpgaAccess::Spi {
                    word: 0x8020_0003,
                    ..
                }))
            ),
            "libusdr's zeroed cache selects no bank, so MAC = both is written"
        );
        let (issued, outcome) = run(&mut close, &[]);
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            issued,
            27 + 5 + BOARD.len(),
            "streaming down, destroy, board"
        );
    }

    #[test]
    fn a_failed_streaming_down_write_skips_its_batch_and_carries_on() {
        // Exchange 3 is the first RX front-end write (after one bank selection and the two
        // RX processor words); its batch of eleven is abandoned.
        let (issued, outcome) = run(&mut Close::new(Lms7002m::new(SpiAddr(0))), &[3]);
        assert_eq!(outcome, Ok(()), "libusdr ignores it");
        assert_eq!(issued, 27 - 10 + 5 + BOARD.len());
    }

    #[test]
    fn a_failed_afe_write_ends_the_close_before_destroy_and_power_off() {
        let mut close = Close::new(Lms7002m::new(SpiAddr(0)));
        let (issued, outcome) = run(&mut close, &[26]);
        assert_eq!(issued, 27);
        assert_eq!(
            outcome.unwrap_err().access(),
            Some(("LMS7002M", Access::Write, 0x0082))
        );
        assert_eq!(close.next(), None, "the board stays powered, as in libusdr");
    }

    #[test]
    fn a_failed_board_write_ends_the_close() {
        let mut close = Close::new(Lms7002m::new(SpiAddr(0)));
        let (issued, outcome) = run(&mut close, &[27 + 5]);
        assert_eq!(issued, 27 + 5 + 1);
        assert_eq!(
            outcome.unwrap_err().access(),
            Some(("FPGA", Access::Write, 0)),
            "the LMS7002M power control"
        );
        assert_eq!(close.next(), None);
    }
}
