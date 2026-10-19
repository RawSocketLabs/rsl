//! Front-end boards on the M.2 key A+E connector, at register level: the chips libusdr's
//! `device/ext_*` drivers reach on the connector's I2C bus (FPGA I2C bus 1,
//! `I2C_BUS_FRONTEND`, on the uSDR and XSDR alike).
//!
//! Addresses not listed for a board read the idle bus, as on the bare connector; the host
//! sim answers them.

use crate::chips::{Dac80501, Expander, I2cChip, Lmk05318, Reg8File, Reg16File, Tmp114};
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
/// `SimpleSync`'s and XMASS's LMK05318 (`I2C_ADDR_LMK`).
const LMK05318: u16 = 0x65;
/// XMASS's control expander, an XRA1201 driven as a TCA9555 (`I2C_ADDR_XRA1201`).
const XMASS_EXPANDER: u16 = 0x14;
/// XMASS revision 1a's first temperature sensor, a TMP114NA (`I2C_TMP114NA`).
const XMASS_SENSOR_A: u16 = 0x4f;
/// XMASS revision 1a's second temperature sensor, a TMP114NB (`I2C_TMP114NB`).
const XMASS_SENSOR_B: u16 = 0x4e;
/// XMASS revision 1a's AT24 EEPROM, its security-register address (`I2C_DEV_AT24_SEC`).
const XMASS_EEPROM: u16 = 0x58;
/// The EEPROM's factory serial number: 16 bytes from security-register address 0x80
/// (`AT24_SECURE_SERIAL_OFF`). The bytes are a sim choice; libusdr only logs them.
const XMASS_SERIAL: [(u8, u8); 16] = [
    (0x80, 0xa0),
    (0x81, 0xa1),
    (0x82, 0xa2),
    (0x83, 0xa3),
    (0x84, 0xa4),
    (0x85, 0xa5),
    (0x86, 0xa6),
    (0x87, 0xa7),
    (0x88, 0xa8),
    (0x89, 0xa9),
    (0x8a, 0xaa),
    (0x8b, 0xab),
    (0x8c, 0xac),
    (0x8d, 0xad),
    (0x8e, 0xae),
    (0x8f, 0xaf),
];
/// The temperatures XMASS revision 1a's two sensors report, in millidegrees Celsius; a
/// sim choice, libusdr only logs them.
const XMASS_TEMPERATURES_MC: [i32; 2] = [31_500, 29_250];
/// The ID the modelled DACs report: a production DAC80501M, one of the IDs libusdr's
/// `dac80501_init` accepts.
const DAC_ID: u16 = 0x0195;

/// A front-end board to attach to a host sim's connector.
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

    /// The `SimpleSync` clock board (`simplesync`): an LMK05318 at 0x65 whose APLLs report
    /// lock (assumptions with the chip model).
    SimpleSync,

    /// The XMASS board (`xmass`): a TCA9555-compatible expander at 0x14 and an LMK05318 at
    /// 0x65, as on [`FrontEnd::SimpleSync`].
    Xmass,

    /// The XMASS board as libusdr logs revision 1a: [`FrontEnd::Xmass`] plus the two TMP114s
    /// `board_xmass_init` reads, a TMP114NA at 0x4F and a TMP114NB at 0x4E (31.5 °C and
    /// 29.25 °C), and an AT24 EEPROM whose security registers answer at 0x58 with a 16-byte
    /// serial number from 0x80. libusdr reads the serial only if a sensor identifies itself.
    XmassRev1a,
}

/// The chips of an attached front end, as a host sim keeps them.
#[derive(Debug)]
pub struct Model(Chips);

/// The chips of each board.
#[derive(Debug)]
enum Chips {
    /// See [`FrontEnd::Pciefe`].
    Pciefe {
        /// RF-path expander.
        fe: Expander,
        /// Power and LED expander.
        gpio: Expander,
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
    /// See [`FrontEnd::SimpleSync`].
    SimpleSync {
        /// The clock synchronizer, unless taken off the bus.
        lmk: Option<Lmk05318>,
    },
    /// See [`FrontEnd::Xmass`] and [`FrontEnd::XmassRev1a`].
    Xmass {
        /// The control expander.
        expander: Expander,
        /// The clock synchronizer, unless taken off the bus.
        lmk: Option<Lmk05318>,
        /// Revision 1a's sensors and EEPROM.
        rev1a: Option<Box<XmassRev1a>>,
    },
}

/// The chips XMASS revision 1a adds.
#[derive(Debug)]
struct XmassRev1a {
    /// The TMP114NA.
    sensor_a: Tmp114,
    /// The TMP114NB.
    sensor_b: Tmp114,
    /// The AT24's security registers: a byte file with an auto-incrementing pointer, so a
    /// read continues from the address written, as AT24 sequential reads do (assumption;
    /// no AT24 data sheet is vendored).
    eeprom: Reg8File,
}

impl Chips {
    /// The chips of `board` at reset.
    fn new(board: FrontEnd) -> Self {
        match board {
            FrontEnd::Pciefe => Self::Pciefe {
                fe: Expander::new(),
                gpio: Expander::new(),
                dac: Dac80501::new(DAC_ID),
            },
            FrontEnd::Exm2pe => Self::Exm2pe {
                dac: Dac80501::new(DAC_ID),
            },
            FrontEnd::SuperSync => Self::SuperSync {
                lmk: Reg16File::with_resets(&LMK5C33216_ID),
            },
            FrontEnd::SimpleSync => Self::SimpleSync {
                lmk: Some(Lmk05318::new()),
            },
            FrontEnd::Xmass | FrontEnd::XmassRev1a => Self::Xmass {
                expander: Expander::new(),
                lmk: Some(Lmk05318::new()),
                rev1a: (board == FrontEnd::XmassRev1a).then(|| {
                    Box::new(XmassRev1a {
                        sensor_a: Tmp114::new(XMASS_TEMPERATURES_MC[0]),
                        sensor_b: Tmp114::new(XMASS_TEMPERATURES_MC[1]),
                        eeprom: Reg8File::with_resets(&XMASS_SERIAL),
                    })
                }),
            },
        }
    }

    /// The chip answering at `addr` on the connector's I2C bus, if this board has one
    /// there.
    fn chip(&mut self, addr: u16) -> Option<&mut dyn I2cChip> {
        match (self, addr) {
            (Self::Pciefe { fe, .. }, PCIEFE_FE_EXPANDER) => Some(fe),
            (Self::Pciefe { gpio, .. }, PCIEFE_GPIO_EXPANDER) => Some(gpio),
            (Self::Pciefe { dac, .. } | Self::Exm2pe { dac }, DAC) => Some(dac),
            (Self::SuperSync { lmk }, SUPERSYNC_LMK) => Some(lmk),
            (Self::SimpleSync { lmk } | Self::Xmass { lmk, .. }, LMK05318) => {
                lmk.as_mut().map(|lmk| lmk as &mut dyn I2cChip)
            }
            (Self::Xmass { expander, .. }, XMASS_EXPANDER) => Some(expander),
            (
                Self::Xmass {
                    rev1a: Some(chips), ..
                },
                XMASS_SENSOR_A,
            ) => Some(&mut chips.sensor_a),
            (
                Self::Xmass {
                    rev1a: Some(chips), ..
                },
                XMASS_SENSOR_B,
            ) => Some(&mut chips.sensor_b),
            (
                Self::Xmass {
                    rev1a: Some(chips), ..
                },
                XMASS_EEPROM,
            ) => Some(&mut chips.eeprom),
            _ => None,
        }
    }

    /// The oscillator DAC's output code, on boards with one.
    fn dac_code(&self) -> Option<u16> {
        match self {
            Self::Pciefe { dac, .. } | Self::Exm2pe { dac } => Some(dac.code()),
            Self::SuperSync { .. } | Self::SimpleSync { .. } | Self::Xmass { .. } => None,
        }
    }

    /// Makes the board's DAC identify as `id`; `false` if it has none.
    fn set_dac_id(&mut self, id: u16) -> bool {
        match self {
            Self::Pciefe { dac, .. } | Self::Exm2pe { dac } => {
                dac.set_id(id);
                true
            }
            Self::SuperSync { .. } | Self::SimpleSync { .. } | Self::Xmass { .. } => false,
        }
    }

    /// Makes the `PCIe` front end's RF-path expander read all inputs low; `false` on other
    /// boards.
    fn stick_expander_low(&mut self) -> bool {
        match self {
            Self::Pciefe { fe, .. } => {
                fe.stick_inputs_low();
                true
            }
            _ => false,
        }
    }

    /// A clock-synchronizer register, on boards with one fitted.
    fn clock_reg(&self, addr: u16) -> Option<u8> {
        match self {
            Self::SuperSync { lmk } => Some(lmk.reg(addr)),
            Self::SimpleSync { lmk } | Self::Xmass { lmk, .. } => {
                lmk.as_ref().map(|lmk| lmk.reg(addr))
            }
            Self::Pciefe { .. } | Self::Exm2pe { .. } => None,
        }
    }

    /// The board's LMK05318 slot, on boards that carry one.
    fn lmk05318(&mut self) -> Option<&mut Option<Lmk05318>> {
        match self {
            Self::SimpleSync { lmk } | Self::Xmass { lmk, .. } => Some(lmk),
            Self::Pciefe { .. } | Self::Exm2pe { .. } | Self::SuperSync { .. } => None,
        }
    }

    /// Makes the board's LMK05318 APLLs never lock; `false` on boards without one fitted.
    fn unlock_lmk05318(&mut self) -> bool {
        self.lmk05318()
            .and_then(Option::as_mut)
            .map(Lmk05318::unlock)
            .is_some()
    }

    /// Takes the board's LMK05318 off the bus; `false` on boards without one fitted.
    fn remove_lmk05318(&mut self) -> bool {
        self.lmk05318().and_then(Option::take).is_some()
    }
}

impl Model {
    /// The chips of `board` at reset.
    #[must_use]
    pub fn new(board: FrontEnd) -> Self {
        Self(Chips::new(board))
    }

    /// The chip answering at `addr` on the connector's I2C bus, if this board has one
    /// there.
    pub fn chip(&mut self, addr: u16) -> Option<&mut dyn I2cChip> {
        self.0.chip(addr)
    }

    /// The oscillator DAC's output code, on boards with one.
    #[must_use]
    pub fn dac_code(&self) -> Option<u16> {
        self.0.dac_code()
    }

    /// Makes the board's DAC identify as `id`; `false` if it has none.
    pub fn set_dac_id(&mut self, id: u16) -> bool {
        self.0.set_dac_id(id)
    }

    /// Makes the `PCIe` front end's RF-path expander read all inputs low; `false` on other
    /// boards.
    pub fn stick_expander_low(&mut self) -> bool {
        self.0.stick_expander_low()
    }

    /// A clock-synchronizer register, on boards with one fitted.
    #[must_use]
    pub fn clock_reg(&self, addr: u16) -> Option<u8> {
        self.0.clock_reg(addr)
    }

    /// Makes the board's LMK05318 APLLs never lock; `false` on boards without one fitted.
    pub fn unlock_lmk05318(&mut self) -> bool {
        self.0.unlock_lmk05318()
    }

    /// Takes the board's LMK05318 off the bus; `false` on boards without one fitted.
    pub fn remove_lmk05318(&mut self) -> bool {
        self.0.remove_lmk05318()
    }
}
