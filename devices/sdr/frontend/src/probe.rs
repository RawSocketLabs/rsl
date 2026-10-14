//! The probe itself (`device_fe_probe`): which boards to try, in which order, what to do
//! when one fails, and the hand-over to the driver for the bring-ups it owns.

use hal::Bus;

use crate::board::{Board, Connector};
use crate::error::Error;
use crate::exm2pe::Exm2pe;
use crate::pciefe::Pciefe;
use crate::selection::FrontEndSelection;
use crate::{fe4ch, fe1005000, simplesync, supersync, xmass};

/// A board that answered the probe, brought up as far as this crate goes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Detected {
    /// The `PCIe` front end, up with the selection's options (`board_ext_pciefe_init`).
    Pciefe(Pciefe),

    /// The M.2-to-`PCIe` breakout, up with the selection's options (`board_exm2pe_init`).
    Exm2pe(Exm2pe),

    /// XMASS: its control expander kept the direction mask. The rest of
    /// `board_xmass_init` (power, the LMK05318, the sync pulse) is the driver's; this
    /// crate's [`xmass`](crate::xmass) functions drive the expander for it.
    Xmass,

    /// `SuperSync`: its LMK5C33216 identified itself. Its register load is the driver's.
    SuperSync,

    /// `SimpleSync`: its LMK05318 identified itself, with the board's clock tree and GPS
    /// powered. The rest of `lmk05318_create` and the board's bring-up are the driver's.
    SimpleSync,

    /// fe1005000: its XRA1405 expander kept its configuration. Its bring-up
    /// (`tune_rf_path`) is not ported.
    Fe1005000,

    /// fe4ch4007200: its TMP114 identified itself. Its expander test and bring-up are not
    /// ported.
    Fe4ch4007200,
}

impl Detected {
    /// Which board this is.
    #[must_use]
    pub const fn board(&self) -> Board {
        match self {
            Self::Pciefe(_) => Board::Pciefe,
            Self::Exm2pe(_) => Board::Exm2pe,
            Self::Xmass => Board::Xmass,
            Self::SuperSync => Board::SuperSync,
            Self::SimpleSync => Board::SimpleSync,
            Self::Fe1005000 => Board::Fe1005000,
            Self::Fe4ch4007200 => Board::Fe4ch4007200,
        }
    }

    /// Looks for `board` as libusdr's init function does, as far as this crate brings it
    /// up; `None` when it is absent. `selection` carries a hint's options, if any.
    fn detect(
        board: Board,
        bus: &mut dyn Bus,
        selection: &FrontEndSelection,
    ) -> Result<Option<Self>, Error> {
        Ok(match board {
            Board::Pciefe => {
                let options = match selection {
                    FrontEndSelection::Pciefe(options) => Some(options),
                    _ => None,
                };
                Pciefe::init(bus, options)?.map(Self::Pciefe)
            }
            Board::Xmass => xmass::detect(bus)?.then_some(Self::Xmass),
            Board::SuperSync => supersync::detect(bus)?.then_some(Self::SuperSync),
            Board::SimpleSync => simplesync::detect(bus)?.then_some(Self::SimpleSync),
            Board::Exm2pe => {
                let options = match selection {
                    FrontEndSelection::Exm2pe(options) => Some(options),
                    _ => None,
                };
                Some(Self::Exm2pe(Exm2pe::init(bus, options)?))
            }
            Board::Fe1005000 => fe1005000::detect(bus)?.then_some(Self::Fe1005000),
            Board::Fe4ch4007200 => fe4ch::detect(bus)?.then_some(Self::Fe4ch4007200),
        })
    }
}

/// How a driver's bring-up of a [`Detected`] board failed.
#[derive(Debug)]
pub enum Failure<E> {
    /// The board did not come up, as a libusdr init function returning an error: the
    /// automatic probe reports it to [`BringUp::skipped`] and tries the next board; a
    /// hinted probe fails with it.
    Board(E),

    /// Stop probing and fail with this error, whatever the selection: the driver cannot
    /// continue libusdr's bring-up (it is not ported), so carrying on would leave
    /// libusdr's path.
    Stop(E),
}

impl<E> Failure<E> {
    /// The error, whichever failure it is.
    fn into_inner(self) -> E {
        match self {
            Self::Board(error) | Self::Stop(error) => error,
        }
    }
}

/// What a driver adds to the probe: the bring-ups it owns, in its own error type.
pub trait BringUp {
    /// The front end the driver keeps.
    type FrontEnd;

    /// The driver's error; this crate's failures convert into it.
    type Error: From<Error>;

    /// Continues libusdr's init of a board that answered.
    ///
    /// # Errors
    ///
    /// [`Failure::Board`] when the board does not come up; [`Failure::Stop`] when the
    /// driver cannot bring it up at all.
    fn bring_up(
        &mut self,
        bus: &mut dyn Bus,
        detected: Detected,
    ) -> Result<Self::FrontEnd, Failure<Self::Error>>;

    /// The automatic probe skips `board`, which failed with `error` (libusdr logs it).
    fn skipped(&mut self, board: Board, error: &Self::Error) {
        let _ = (board, error);
    }
}

impl Connector {
    /// Looks for a front end on this connector as `selection` says and brings up the first
    /// that answers (`device_fe_probe`), handing each board that answers to `driver`.
    ///
    /// Automatically, every board the connector admits is tried in libusdr's order; one
    /// that fails is reported to [`BringUp::skipped`] and the next is tried. With a hint,
    /// only the named board is tried and its failure is the probe's. A board the connector
    /// does not admit is skipped without a bus access.
    ///
    /// `Ok(None)` when nothing came up. With a hint (or [`FrontEndSelection::None`]) that
    /// is libusdr's `-ENODEV`, which the uSDR ignores and the XSDR and `LSDr` fail their
    /// open on; automatically it is libusdr's success without a front end.
    ///
    /// # Errors
    ///
    /// The hinted board's failure, or a [`Failure::Stop`] from `driver`.
    pub fn probe<B: BringUp>(
        self,
        bus: &mut dyn Bus,
        selection: &FrontEndSelection,
        driver: &mut B,
    ) -> Result<Option<B::FrontEnd>, B::Error> {
        if let Some(board) = selection.hinted() {
            return self
                .try_board(bus, board, selection, driver)
                .map_err(Failure::into_inner);
        }
        if *selection == FrontEndSelection::None {
            return Ok(None);
        }
        for board in Board::ALL {
            match self.try_board(bus, board, selection, driver) {
                Ok(Some(front_end)) => return Ok(Some(front_end)),
                Ok(None) => {}
                Err(Failure::Board(error)) => driver.skipped(board, &error),
                Err(Failure::Stop(error)) => return Err(error),
            }
        }
        Ok(None)
    }

    /// Tries one board: `None` when the connector does not admit it or it is absent.
    fn try_board<B: BringUp>(
        self,
        bus: &mut dyn Bus,
        board: Board,
        selection: &FrontEndSelection,
        driver: &mut B,
    ) -> Result<Option<B::FrontEnd>, Failure<B::Error>> {
        if !board.admits(self) {
            return Ok(None);
        }
        let detected = Detected::detect(board, bus, selection)
            .map_err(|error| Failure::Board(error.into()))?;
        detected
            .map(|detected| driver.bring_up(bus, detected))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use std::boxed::Box;
    use std::time::Duration;
    use std::vec::Vec;

    use hal::{BusError, I2cAddr, SpiAddr};

    use super::*;
    use crate::FrontEndKind;

    /// An empty connector: idle-high I2C and SPI, every access recorded. I2C core 1 is
    /// refused, as a one-core device's transport does.
    #[derive(Default)]
    struct EmptyConnector {
        /// I2C devices addressed, in order.
        i2c: Vec<I2cAddr>,
        /// Register writes and SPI exchanges.
        other: usize,
    }

    impl Bus for EmptyConnector {
        fn read_regs(&mut self, _: u32, out: &mut [u32]) -> Result<(), BusError> {
            out.fill(0);
            Ok(())
        }

        fn write_regs(&mut self, _: u32, _: &[u32]) -> Result<(), BusError> {
            self.other += 1;
            Ok(())
        }

        fn spi32(&mut self, _: SpiAddr, _: u32) -> Result<u32, BusError> {
            self.other += 1;
            Ok(u32::MAX)
        }

        fn i2c(&mut self, dev: I2cAddr, _: &[u8], read: &mut [u8]) -> Result<(), BusError> {
            if dev.core() != 0 {
                return Err(BusError::Unsupported("one I2C core"));
            }
            self.i2c.push(dev);
            read.fill(0xff);
            Ok(())
        }

        fn sleep(&mut self, _: Duration) {}
    }

    /// A driver that keeps the kinds this crate brings up whole and stops on the rest,
    /// recording what it was offered and what was skipped.
    #[derive(Default)]
    struct Recorder {
        offered: Vec<Board>,
        skipped: Vec<Board>,
    }

    impl BringUp for Recorder {
        type FrontEnd = FrontEndKind;
        type Error = Box<Error>;

        fn bring_up(
            &mut self,
            _: &mut dyn Bus,
            detected: Detected,
        ) -> Result<FrontEndKind, Failure<Box<Error>>> {
            self.offered.push(detected.board());
            match detected {
                Detected::Exm2pe(board) => Ok(FrontEndKind::Exm2pe {
                    dac: board.has_dac(),
                }),
                Detected::Pciefe(_) => Ok(FrontEndKind::Pciefe),
                other => Err(Failure::Stop(Box::new(Error::Board {
                    board: other.board().name(),
                    reason: "not ported",
                }))),
            }
        }

        fn skipped(&mut self, board: Board, _: &Box<Error>) {
            self.skipped.push(board);
        }
    }

    #[test]
    fn an_empty_key_a_e_connector_skips_the_pciefe_and_settles_on_a_breakout_without_a_dac() {
        let mut bus = EmptyConnector::default();
        let mut driver = Recorder::default();
        let found = Connector::M2KeyAE
            .probe(&mut bus, &FrontEndSelection::Auto, &mut driver)
            .expect("the probe succeeds");
        assert_eq!(found, Some(FrontEndKind::Exm2pe { dac: false }));
        assert_eq!(driver.skipped, [Board::Pciefe], "its DAC is not recognised");
        assert_eq!(driver.offered, [Board::Exm2pe]);
    }

    #[test]
    fn an_empty_lsdr_connector_finds_nothing_and_touches_only_its_two_boards() {
        let mut bus = EmptyConnector::default();
        let mut driver = Recorder::default();
        let found = Connector::Lsdr
            .probe(&mut bus, &FrontEndSelection::Auto, &mut driver)
            .expect("the probe succeeds");
        assert_eq!(found, None);
        assert_eq!(
            driver.skipped,
            [Board::Fe4ch4007200],
            "this bus refuses I2C core 1"
        );
        assert!(bus.i2c.is_empty(), "no core-0 I2C device is addressed");
        // fe1005000's 7 pin writes and 7 select-and-exchange pairs, fe4ch4007200's 3 pins.
        assert_eq!(bus.other, 7 + 14 + 3);
    }

    #[test]
    fn a_hint_tries_only_its_board_and_an_unadmitted_one_touches_nothing() {
        let mut bus = EmptyConnector::default();
        let mut driver = Recorder::default();
        let found = Connector::Lsdr
            .probe(&mut bus, &FrontEndSelection::Xmass, &mut driver)
            .expect("an unadmitted hint is not an error");
        assert_eq!(found, None);
        assert_eq!((bus.i2c.len(), bus.other), (0, 0));

        let found = Connector::M2KeyAE
            .probe(&mut bus, &FrontEndSelection::SuperSync, &mut driver)
            .expect("an absent hinted board is not an error");
        assert_eq!(found, None);
        assert_eq!(bus.i2c, [I2cAddr::new(1, 0x64)], "only SuperSync's ID read");
    }

    #[test]
    fn a_hinted_board_that_fails_fails_the_probe() {
        let mut bus = EmptyConnector::default();
        let failure = Connector::M2KeyAE
            .probe(
                &mut bus,
                &FrontEndSelection::Pciefe(crate::PciefeOptions::default()),
                &mut Recorder::default(),
            )
            .expect_err("the pciefe's DAC is not recognised");
        assert!(matches!(
            *failure,
            Error::Board {
                board: "pciefe",
                ..
            }
        ));
    }

    #[test]
    fn a_stopping_driver_ends_the_automatic_probe_at_the_board_it_cannot_bring_up() {
        /// Answers XMASS's direction test, so XMASS is detected.
        #[derive(Default)]
        struct WithXmass(EmptyConnector);

        impl Bus for WithXmass {
            fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
                self.0.read_regs(addr, out)
            }
            fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
                self.0.write_regs(addr, values)
            }
            fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
                self.0.spi32(target, word)
            }
            fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
                self.0.i2c(dev, write, read)?;
                if dev.addr() == 0x14 && write == [0x06] {
                    read.copy_from_slice(&0xe000_u16.to_le_bytes());
                }
                Ok(())
            }
            fn sleep(&mut self, duration: Duration) {
                self.0.sleep(duration);
            }
        }

        let mut bus = WithXmass::default();
        let mut driver = Recorder::default();
        let failure = Connector::M2KeyAE
            .probe(&mut bus, &FrontEndSelection::Auto, &mut driver)
            .expect_err("the driver cannot bring XMASS up");
        assert!(matches!(*failure, Error::Board { board: "xmass", .. }));
        assert_eq!(driver.offered, [Board::Xmass], "no later board is tried");
    }
}
