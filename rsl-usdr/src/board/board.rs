//! The `m2_lm6_1` board: its state, how its chips are wired, and its supply setpoints.
//! The sequences that drive it are `impl Board` blocks in sibling modules.
//!
//! The crate docs describe the board as a whole: its chips, signal paths, clock tree and
//! power-up order. This module holds the facts the sequences need about this one board:
//! which chip sits where (each chip's `usdr()` constructor) and the voltages the board
//! runs its rails at. A chip module knows what its registers mean; only `board/` decides
//! what to write to them and in what order.

use std::fmt;

use crate::chips::lms6002d::Lms6002d;
use crate::chips::lp8758::{BuckVoltage, Lp8758};
use crate::chips::si5332::{LvpeclOutput, Si5332};
use crate::chips::tmp114::Tmp114;
use crate::chips::tps6381x::{OutputVoltage, Tps6381x};
use crate::fpga::Decimation;
use crate::lowlevel::Bus;

/// A powered uSDR board and the bus it is reached through.
///
/// A `Board` exists only between power-up ([`Identified::power_up`]) and power-down
/// ([`Board::power_down`]), so holding one means the rails, clocks and LMS6002D are up.
/// Before power-up, [`Identified`] owns the bus instead.
///
/// [`Identified`]: super::power::Identified
/// [`Identified::power_up`]: super::power::Identified::power_up
pub(crate) struct Board {
    /// The hardware seam; chip drivers borrow it per call.
    pub(super) bus: Box<dyn Bus>,

    /// The RF transceiver. It is the one chip with driver-side state: the
    /// LMS6002D's registers are not read back, so it caches what it last wrote.
    pub(super) lms: Lms6002d,

    /// Revision 3, where the Si5332's RX sample clock is output 1 rather than 0.
    pub(super) rev3: bool,

    /// Which DSP chains the gateware has.
    pub(super) chains: Chains,

    /// The LMS6002D I/O rail is at [`Self::LMS_VIO_BOOST`] rather than
    /// [`Self::LMS_VIO_NORMAL`].
    pub(super) vio_boosted: bool,

    /// The LMS6002D's receiver and RXVGA2 are powered (libusdr's `rx_pwren`).
    pub(super) rx_powered: bool,

    /// The decimation of the last sample rate set (libusdr's `rxbb_decim`); `None` until the
    /// first. Its FIR is loaded unless the board has no RX chain.
    pub(super) rx_decimation: Option<Decimation>,
}

/// Which DSP chains the gateware has (HWID bits 25 and 24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Chains {
    /// An RX chain.
    pub(super) rx: bool,

    /// A TX chain.
    pub(super) tx: bool,
}

impl fmt::Debug for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Board")
            .field("lms", &self.lms)
            .field("rev3", &self.rev3)
            .field("chains", &self.chains)
            .field("vio_boosted", &self.vio_boosted)
            .field("rx_powered", &self.rx_powered)
            .field("rx_decimation", &self.rx_decimation)
            .finish_non_exhaustive()
    }
}

impl Board {
    /// The LP8758 PMIC: four step-down converters, among them the FPGA I/O bank rail
    /// ([`Self::VGPIO`]) and the LMS6002D's I/O rail ([`Self::LMS_VIO_NORMAL`]).
    pub(super) const PMIC: Lp8758 = Lp8758::usdr();

    /// The TMP114 temperature sensor, the only temperature the driver reads. The thermal
    /// policy acts on it before power-up, so it must answer from reset.
    pub(super) const TEMP: Tmp114 = Tmp114::usdr();

    /// The Si5332 clock generator: the LMS6002D's PLL reference, the RX and TX sample
    /// clocks, and the external mixer's LO. See the crate docs for its output wiring.
    pub(super) const CLOCK: Si5332 = Si5332::usdr();

    /// The TPS63811 buck-boost converter, programmed to [`Self::BOOST_VOLTAGE`] before
    /// the clocks start.
    pub(super) const BOOST: Tps6381x = Tps6381x::usdr();

    /// FPGA GPIO bank voltage (PMIC buck 1): 1.8 V, which libusdr chose "to be compatible
    /// with xSDR", another Wavelet Lab board.
    pub(super) const VGPIO: BuckVoltage = BuckVoltage::from_millivolts(1800);

    /// LMS6002D digital I/O rail (PMIC buck 3) at normal sample rates: 1.8 V
    /// (`LMS6_VIO_NORM`).
    pub(super) const LMS_VIO_NORMAL: BuckVoltage = BuckVoltage::from_millivolts(1800);

    /// LMS6002D digital I/O rail at ADC rates of 62 MS/s and above: 1.925 V
    /// (`LMS6_VIO_BOOST`), "to get stable samplerates over 60Msps" in libusdr's words.
    pub(super) const LMS_VIO_BOOST: BuckVoltage = BuckVoltage::from_millivolts(1925);

    /// The reference every clock derives from: 26 MHz on every revision
    /// (`USDR_INT_REFCLK`; external references are not ported).
    pub(super) const REFERENCE_HZ: u32 = 26_000_000;

    /// The mixer LO is the Si5332 VCO divided by this (`si_vco_div`, set in `usdr_init`).
    pub(super) const MIXER_LO_DIVIDER: u8 = 8;

    /// Boost converter output, 3.45 V, the same whichever level the TPS63811's VSEL pin
    /// sits at.
    pub(super) const BOOST_VOLTAGE: OutputVoltage = OutputVoltage::from_millivolts(3450);
}

impl Board {
    /// Which of the Si5332's outputs 0 and 1 carries the LMS6002D's PLL reference (LVPECL);
    /// the other is the RX sample clock. Revision 3 swapped them.
    pub(super) const fn lvpecl(rev3: bool) -> LvpeclOutput {
        if rev3 {
            LvpeclOutput::Out0
        } else {
            LvpeclOutput::Out1
        }
    }
}
