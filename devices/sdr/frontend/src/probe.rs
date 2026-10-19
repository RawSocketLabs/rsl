//! The probe itself (`device_fe_probe`): which boards to try, in which order, what to do
//! when one fails, and the board that came up.

use std::boxed::Box;

use deps::tracing::{debug, info, warn};
use hal::Bus;

use crate::board::{Board, Connector, FrontEndKind};
use crate::error::Error;
use crate::exm2pe::Exm2pe;
use crate::fe4ch;
use crate::fe1005000::Fe1005000;
use crate::pciefe::Pciefe;
use crate::selection::FrontEndSelection;
use crate::simplesync::SimpleSync;
use crate::supersync::SuperSync;
use crate::xmass::Xmass;

/// A front-end board that came up, with the state libusdr keeps for it (`struct dev_fe`'s
/// union).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrontEnd {
    /// The `PCIe` front end (`board_ext_pciefe_init`).
    Pciefe(Pciefe),

    /// The M.2-to-`PCIe` breakout (`board_exm2pe_init`).
    Exm2pe(Exm2pe),

    /// XMASS (`board_xmass_init`); boxed, its clock synchronizer's state is large.
    Xmass(Box<Xmass>),

    /// `SuperSync` (`board_ext_supersync_init`).
    SuperSync(SuperSync),

    /// `SimpleSync` (`board_ext_simplesync_init`); boxed as XMASS.
    SimpleSync(Box<SimpleSync>),

    /// The 100–5000 MHz front end (`ext_fe_100_5000_init`).
    Fe1005000(Fe1005000),
}

impl FrontEnd {
    /// Which board this is.
    #[must_use]
    pub const fn board(&self) -> Board {
        match self {
            Self::Pciefe(_) => Board::Pciefe,
            Self::Exm2pe(_) => Board::Exm2pe,
            Self::Xmass(_) => Board::Xmass,
            Self::SuperSync(_) => Board::SuperSync,
            Self::SimpleSync(_) => Board::SimpleSync,
            Self::Fe1005000(_) => Board::Fe1005000,
        }
    }

    /// What a device reports for this board.
    #[must_use]
    pub const fn kind(&self) -> FrontEndKind {
        match self {
            Self::Pciefe(_) => FrontEndKind::Pciefe,
            Self::Exm2pe(board) => FrontEndKind::Exm2pe {
                dac: board.has_dac(),
            },
            Self::Xmass(_) => FrontEndKind::Xmass,
            Self::SuperSync(_) => FrontEndKind::SuperSync,
            Self::SimpleSync(_) => FrontEndKind::SimpleSync,
            Self::Fe1005000(_) => FrontEndKind::Fe1005000,
        }
    }

    /// Looks for `board` and brings it up as libusdr's init function does; `None` when it
    /// is absent. `selection` carries a hint's options, if any.
    fn init(
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
            Board::Xmass => Xmass::init(bus)?.map(|board| Self::Xmass(Box::new(board))),
            Board::SuperSync => SuperSync::init(bus)?.map(Self::SuperSync),
            Board::SimpleSync => {
                SimpleSync::init(bus)?.map(|board| Self::SimpleSync(Box::new(board)))
            }
            Board::Exm2pe => {
                let options = match selection {
                    FrontEndSelection::Exm2pe(options) => Some(options),
                    _ => None,
                };
                Some(Self::Exm2pe(Exm2pe::init(bus, options)?))
            }
            Board::Fe1005000 => Fe1005000::init(bus)?.map(Self::Fe1005000),
            Board::Fe4ch4007200 => {
                if fe4ch::detect(bus)? {
                    return Err(Error::NotPorted(board));
                }
                None
            }
        })
    }
}

impl Connector {
    /// Looks for a front end on this connector as `selection` says and brings up the first
    /// that answers (`device_fe_probe`), logging the events libusdr logs (in this crate's
    /// words and levels: a skipped board at WARN, where libusdr's `Unable to initialize`
    /// is an ERROR).
    ///
    /// Automatically, every board the connector admits is tried in libusdr's order; one
    /// that fails is logged and skipped, and the next is tried. With a hint, only the named
    /// board is tried and its failure is the probe's. A board the connector does not admit
    /// is skipped without a bus access.
    ///
    /// `Ok(None)` when nothing came up. With a hint (or [`FrontEndSelection::None`]) that
    /// is libusdr's `-ENODEV`, which the uSDR ignores and the XSDR and `LSDr` fail their
    /// open on; automatically it is libusdr's success without a front end.
    ///
    /// # Errors
    ///
    /// The hinted board's failure; [`Error::NotPorted`] when fe4ch4007200 answers, hinted
    /// or not (libusdr would bring it up; no device this crate serves can reach it, its
    /// chips sit on an I2C core only the sSDR has).
    pub fn probe(
        self,
        bus: &mut dyn Bus,
        selection: &FrontEndSelection,
    ) -> Result<Option<FrontEnd>, Error> {
        self.probe_reporting(bus, selection, &mut |board, error| {
            warn!(board = board.name(), %error, "front end failed to come up; skipped");
        })
    }

    /// [`Self::probe`], handing each board the automatic probe skips, with its failure, to
    /// `skipped` (which logs it).
    fn probe_reporting(
        self,
        bus: &mut dyn Bus,
        selection: &FrontEndSelection,
        skipped: &mut dyn FnMut(Board, &Error),
    ) -> Result<Option<FrontEnd>, Error> {
        let found = if let Some(board) = selection.hinted() {
            let found = self.try_board(bus, board, selection)?;
            if found.is_none() {
                warn!(hint = board.name(), connector = ?self, "no front end found with the hint");
            }
            found
        } else if *selection == FrontEndSelection::None {
            debug!("front-end probe skipped");
            None
        } else {
            self.first_up(bus, selection, skipped)?
        };
        if let Some(front_end) = &found {
            info!(board = front_end.board().name(), "front end detected");
        }
        Ok(found)
    }

    /// The automatic probe: the first board in libusdr's order that comes up.
    ///
    /// # Errors
    ///
    /// [`Error::NotPorted`] for a board that answers but is not brought up: carrying on
    /// would leave libusdr's path.
    fn first_up(
        self,
        bus: &mut dyn Bus,
        selection: &FrontEndSelection,
        skipped: &mut dyn FnMut(Board, &Error),
    ) -> Result<Option<FrontEnd>, Error> {
        for board in Board::ALL {
            match self.try_board(bus, board, selection) {
                Ok(Some(front_end)) => return Ok(Some(front_end)),
                Ok(None) => {}
                Err(error @ Error::NotPorted(_)) => return Err(error),
                Err(error) => skipped(board, &error),
            }
        }
        debug!("no front end detected");
        Ok(None)
    }

    /// Tries one board: `None` when the connector does not admit it or it is absent.
    fn try_board(
        self,
        bus: &mut dyn Bus,
        board: Board,
        selection: &FrontEndSelection,
    ) -> Result<Option<FrontEnd>, Error> {
        if !board.admits(self) {
            return Ok(None);
        }
        FrontEnd::init(board, bus, selection)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use std::vec::Vec;

    use hal::{BusError, I2cAddr, SpiAddr};

    use super::*;

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

    /// Probes `connector` on `bus` as `selection` says, collecting the boards skipped.
    fn probe(
        connector: Connector,
        bus: &mut dyn Bus,
        selection: &FrontEndSelection,
    ) -> (Result<Option<FrontEnd>, Error>, Vec<Board>) {
        let mut skipped = Vec::new();
        let found = connector.probe_reporting(bus, selection, &mut |board, _| skipped.push(board));
        (found, skipped)
    }

    #[test]
    fn an_empty_key_a_e_connector_skips_the_pciefe_and_settles_on_a_breakout_without_a_dac() {
        let mut bus = EmptyConnector::default();
        let (found, skipped) = probe(Connector::M2KeyAE, &mut bus, &FrontEndSelection::Auto);
        let found = found.expect("the probe succeeds");
        assert_eq!(
            found.as_ref().map(FrontEnd::kind),
            Some(FrontEndKind::Exm2pe { dac: false })
        );
        assert_eq!(skipped, [Board::Pciefe], "its DAC is not recognised");
    }

    #[test]
    fn an_empty_lsdr_connector_finds_nothing_and_touches_only_its_two_boards() {
        let mut bus = EmptyConnector::default();
        let (found, skipped) = probe(Connector::Lsdr, &mut bus, &FrontEndSelection::Auto);
        assert_eq!(found.expect("the probe succeeds"), None);
        assert_eq!(
            skipped,
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
        let (found, _) = probe(Connector::Lsdr, &mut bus, &FrontEndSelection::Xmass);
        assert_eq!(found.expect("an unadmitted hint is not an error"), None);
        assert_eq!((bus.i2c.len(), bus.other), (0, 0));

        let (found, _) = probe(Connector::M2KeyAE, &mut bus, &FrontEndSelection::SuperSync);
        assert_eq!(found.expect("an absent hinted board is not an error"), None);
        assert_eq!(bus.i2c, [I2cAddr::new(1, 0x64)], "only SuperSync's ID read");
    }

    #[test]
    fn a_hinted_board_that_fails_fails_the_probe() {
        let mut bus = EmptyConnector::default();
        let (found, skipped) = probe(
            Connector::M2KeyAE,
            &mut bus,
            &FrontEndSelection::Pciefe(crate::PciefeOptions::default()),
        );
        let failure = found.expect_err("the pciefe's DAC is not recognised");
        assert!(matches!(
            failure,
            Error::Board {
                board: "pciefe",
                ..
            }
        ));
        assert_eq!(skipped, [], "a hinted failure is not skipped");
    }

    #[test]
    fn naming_no_board_probes_nothing() {
        let mut bus = EmptyConnector::default();
        let (found, _) = probe(Connector::M2KeyAE, &mut bus, &FrontEndSelection::None);
        assert_eq!(found.expect("not an error"), None);
        assert_eq!((bus.i2c.len(), bus.other), (0, 0));
    }

    #[test]
    fn a_fe4ch4007200_that_answers_stops_the_automatic_probe() {
        /// Answers on I2C core 1 with fe4ch4007200's sensor ID, as a transport with a second
        /// core and the board attached would.
        struct WithFe4ch;

        impl Bus for WithFe4ch {
            fn read_regs(&mut self, _: u32, out: &mut [u32]) -> Result<(), BusError> {
                out.fill(0);
                Ok(())
            }
            fn write_regs(&mut self, _: u32, _: &[u32]) -> Result<(), BusError> {
                Ok(())
            }
            fn spi32(&mut self, _: SpiAddr, _: u32) -> Result<u32, BusError> {
                Ok(u32::MAX)
            }
            fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
                read.fill(0xff);
                if dev.core() == 1 && write == [0x0b] {
                    read.copy_from_slice(&0x1114_u16.to_be_bytes());
                }
                Ok(())
            }
            fn sleep(&mut self, _: Duration) {}
        }

        let (found, skipped) = probe(Connector::Lsdr, &mut WithFe4ch, &FrontEndSelection::Auto);
        assert!(
            matches!(found, Err(Error::NotPorted(Board::Fe4ch4007200))),
            "libusdr would bring the board up: {found:?}"
        );
        assert_eq!(skipped, [], "fe1005000 is absent, not failed");
    }
}
