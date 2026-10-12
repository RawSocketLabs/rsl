//! Front-end boards on the uSDR's M.2 connector, at register level: the chips libusdr's
//! `device/ext_*` drivers reach on FPGA I2C bus 1 (`I2C_BUS_FRONTEND`).
//!
//! The connector's GPIO core (FPGA register 3) is only traced: no modelled chip depends on
//! the pins it sets. Addresses not listed for a board read the idle bus, as on the bare
//! board.

use crate::board::I2cAddress;
use crate::chips::{Dac80501, I2cChip, Reg16File, Tca6416};

/// The FPGA I2C bus on the connector.
const BUS: u8 = 1;
/// The `PCIe` front end's RF-path expander (`ext_pciefe.c`, `I2C_ADDR_FE`).
const PCIEFE_FE_EXPANDER: u16 = 0x20;
/// The `PCIe` front end's power and LED expander (`I2C_ADDR_GPIO`).
const PCIEFE_GPIO_EXPANDER: u16 = 0x21;
/// The oscillator DAC on both the `PCIe` front end and the breakout (`I2C_ADDR_DAC`).
const DAC: u16 = 0x48;
/// The `SuperSync` board's LMK5C33216 (`ext_supersync.c`, `I2C_ADDR_LMK`).
const SUPERSYNC_LMK: u16 = 0x64;
/// The LMK5C33216's identification bytes at addresses 0 to 3 (`VNDRID_BY1`, `VNDRID_BY0`,
/// `PRODID`, `REVID`), the values libusdr's `lmk5c33216_create` requires.
const LMK5C33216_ID: [(u16, u8); 4] = [(0, 0x10), (1, 0x0b), (2, 0x40), (3, 0x05)];
/// The ID the modelled DACs report: a production DAC80501M, one of the IDs libusdr's
/// `dac80501_init` accepts.
const DAC_ID: u16 = 0x0195;

/// A front-end board to attach to a [`SimBoard`](crate::SimBoard).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrontEnd {
    /// The `PCIe` front end (`pciefe`): two TCA6416 expanders at 0x20 and 0x21 and a
    /// DAC80501 at 0x48.
    Pciefe,

    /// The M.2-to-`PCIe` breakout (`exm2pe`) with its DAC80501 at 0x48. The board has no
    /// other chip on the bus; without its DAC it is indistinguishable from no board.
    Exm2pe,

    /// The `SuperSync` clock board (`supersync`): an LMK5C33216 at 0x64, modelled as a plain
    /// register file that identifies itself.
    SuperSync,
}

/// The chips of an attached front end.
#[derive(Debug)]
pub(crate) enum Model {
    /// See [`FrontEnd::Pciefe`].
    Pciefe {
        /// RF-path expander.
        fe: Tca6416,
        /// Power and LED expander.
        gpio: Tca6416,
        /// Oscillator DAC.
        dac: Dac80501,
    },
    /// See [`FrontEnd::Exm2pe`].
    Exm2pe {
        /// Oscillator DAC.
        dac: Dac80501,
    },
    /// See [`FrontEnd::SuperSync`].
    SuperSync {
        /// The clock synchronizer.
        lmk: Reg16File,
    },
}

impl Model {
    /// The chips of `board` at reset.
    pub(crate) fn new(board: FrontEnd) -> Self {
        match board {
            FrontEnd::Pciefe => Self::Pciefe {
                fe: Tca6416::new(),
                gpio: Tca6416::new(),
                dac: Dac80501::new(DAC_ID),
            },
            FrontEnd::Exm2pe => Self::Exm2pe {
                dac: Dac80501::new(DAC_ID),
            },
            FrontEnd::SuperSync => Self::SuperSync {
                lmk: Reg16File::with_resets(&LMK5C33216_ID),
            },
        }
    }

    /// The chip answering at `addr`, if this board has one there.
    pub(crate) fn chip(&mut self, addr: I2cAddress) -> Option<&mut dyn I2cChip> {
        if addr.bus != BUS {
            return None;
        }
        match (self, addr.addr) {
            (Self::Pciefe { fe, .. }, PCIEFE_FE_EXPANDER) => Some(fe),
            (Self::Pciefe { gpio, .. }, PCIEFE_GPIO_EXPANDER) => Some(gpio),
            (Self::Pciefe { dac, .. } | Self::Exm2pe { dac }, DAC) => Some(dac),
            (Self::SuperSync { lmk }, SUPERSYNC_LMK) => Some(lmk),
            _ => None,
        }
    }

    /// The oscillator DAC's output code, on boards with one.
    pub(crate) fn dac_code(&self) -> Option<u16> {
        match self {
            Self::Pciefe { dac, .. } | Self::Exm2pe { dac } => Some(dac.code()),
            Self::SuperSync { .. } => None,
        }
    }

    /// A clock-synchronizer register, on boards with one.
    pub(crate) fn clock_reg(&self, addr: u16) -> Option<u8> {
        match self {
            Self::SuperSync { lmk } => Some(lmk.reg(addr)),
            Self::Pciefe { .. } | Self::Exm2pe { .. } => None,
        }
    }
}
