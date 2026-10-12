//! Front-end boards on the uSDR's M.2 connector, at register level: the chips libusdr's
//! `device/ext_*` drivers reach on FPGA I2C bus 1 (`I2C_BUS_FRONTEND`).
//!
//! The connector's GPIO core (FPGA register 3) is only traced: no modelled chip depends on
//! the pins it sets. Addresses not listed for a board read the idle bus, as on the bare
//! board.

use crate::board::I2cAddress;
use crate::chips::{Dac80501, I2cChip, Tca6416};

/// The FPGA I2C bus on the connector.
const BUS: u8 = 1;
/// The `PCIe` front end's RF-path expander (`ext_pciefe.c`, `I2C_ADDR_FE`).
const PCIEFE_FE_EXPANDER: u16 = 0x20;
/// The `PCIe` front end's power and LED expander (`I2C_ADDR_GPIO`).
const PCIEFE_GPIO_EXPANDER: u16 = 0x21;
/// The oscillator DAC on both the `PCIe` front end and the breakout (`I2C_ADDR_DAC`).
const DAC: u16 = 0x48;
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
            _ => None,
        }
    }

    /// The oscillator DAC's output code.
    pub(crate) fn dac_code(&self) -> u16 {
        match self {
            Self::Pciefe { dac, .. } | Self::Exm2pe { dac } => dac.code(),
        }
    }
}
