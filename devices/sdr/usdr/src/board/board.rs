//! The `m2_lm6_1` board: its state, how its chips are wired, and its supply setpoints.
//! The sequences that drive it are `impl Board` blocks in sibling modules.
//!
//! The crate docs describe the board as a whole: its chips, signal paths, clock tree and
//! power-up order. This module holds the facts the sequences need about this one board:
//! which chip sits where (each chip's `usdr()` constructor) and the voltages the board
//! runs its rails at. A chip module knows what its registers mean; only `board/` decides
//! what to write to them and in what order.

use std::fmt;

use super::stream::StreamGroup;
use super::tune::RxBand;
use crate::chips::lms6002d::Lms6002d;
use crate::chips::lp8758::{BuckControl, BuckVoltage, Config, Lp8758};
use crate::chips::si5332::{LvpeclOutput, Reference, Si5332};
use crate::chips::tmp114::Tmp114;
use crate::chips::tps6381x::{OutputVoltage, Tps6381x};
use crate::fpga::{Decimation, IqImbalance};
use crate::frontend::FrontEnd;
use crate::lowlevel::Bus;
use crate::portable::power::RailPlan;
use crate::reference::ReferenceClock;

/// A powered uSDR board and the bus it is reached through.
///
/// A `Board` exists only between power-up ([`Identified::power_up`]) and power-down
/// ([`Board::power_down`]), so holding one means the rails, clocks and LMS6002D are up.
/// Before power-up, [`Identified`] owns the bus instead.
///
/// [`Identified`]: super::power::Identified
/// [`Identified::power_up`]: super::power::Identified::power_up
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags, each one of libusdr's, not states of one machine"
)]
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

    /// The reference every clock derives from; its frequency feeds each sample-clock plan
    /// and synthesizer tune (libusdr's `fref`).
    pub(super) reference: ReferenceClock,

    /// The LMS6002D I/O rail is at [`Self::LMS_VIO_BOOST`] rather than
    /// [`Self::LMS_VIO_NORMAL`].
    pub(super) vio_boosted: bool,

    /// The ADC's sample rate, the requested rate times the decimation (libusdr's
    /// `adc_clk`); 0 until a rate is set.
    pub(super) adc_rate_hz: u32,

    /// The mixer LO: the Si5332 VCO over [`Self::MIXER_LO_DIVIDER`] (libusdr's
    /// `mixer_lo`); 0 until a rate is set.
    pub(super) mixer_lo_hz: u32,

    /// What the receive chain was last set to.
    pub(super) rx: RxState,

    /// What the transmit chain was last set to.
    pub(super) tx: TxState,

    /// libusdr's `rx_run`: set when an RX stream is created, cleared when it is destroyed,
    /// and left set by a failed create.
    pub(super) rx_running: bool,

    /// libusdr's `tx_run`, the same for the TX stream.
    pub(super) tx_running: bool,

    /// The stream group, while one exists.
    pub(super) streams: Option<StreamGroup>,

    /// The front end on the M.2 connector, once the probe brought one up (libusdr's
    /// `fe`).
    pub(super) front_end: Option<FrontEnd>,
}

/// The receive chain's settings that later calls depend on, as libusdr tracks them.
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags, each one of libusdr's, not states of one machine"
)]
pub(super) struct RxState {
    /// The LMS6002D's receiver and RXVGA2 are powered (libusdr's `rx_pwren`).
    pub(super) powered: bool,

    /// The caller fixed the RX bandwidth (libusdr's `rx_bw.set`), so sample-rate and
    /// frequency changes leave the filter alone.
    pub(super) bandwidth_fixed: bool,

    /// The decimation of the last sample rate set (libusdr's `rxbb_decim`); `None` until the
    /// first. Its FIR is loaded unless the board has no RX chain.
    pub(super) decimation: Option<Decimation>,

    /// The frequency the caller tuned to, in Hz (libusdr's `rx_lo`).
    pub(super) lo_hz: u32,

    /// The band in use (libusdr's `rx_cfg_path`): the frequency's, unless the caller fixed
    /// one; `None` until either happens.
    pub(super) band: Option<RxBand>,

    /// The caller fixed the band with [`Board::set_rx_path`], so frequency changes leave
    /// it alone (libusdr's `rx_rfic_path` other than `USDR_RX_AUTO`).
    pub(super) band_fixed: bool,

    /// The TX mixer's output is looped back into the RX path (libusdr's
    /// `rf_loopback_active`).
    pub(super) rf_loopback: bool,

    /// The board mixer is in the RX path (libusdr's `mexir_en`).
    pub(super) mixer_on: bool,

    /// The lowest LO the RX PLL was found to lock at below 250 MHz, in Hz (libusdr's
    /// `rx_minimal_lo`); 0 until a lock failure made the driver search for it.
    pub(super) minimal_lo_hz: u32,

    /// How far the PLL sits from the wanted LO, in Hz, when it cannot reach it; the NCO
    /// makes up the difference (libusdr's `rx_exten_lo`).
    pub(super) lo_offset_hz: i32,

    /// RXVGA2's first-stage gain in 3 dB steps (libusdr's `rx_vga2a`), which the DC
    /// calibration at stream creation restores.
    pub(super) vga2a: u8,

    /// RXVGA2's second-stage gain in 3 dB steps (libusdr's `rx_vga2b`).
    pub(super) vga2b: u8,

    /// The IQ imbalance correction (libusdr's `rx_corr`), which stream creation writes
    /// again.
    pub(super) iq_imbalance: IqImbalance,
}

/// The transmit chain's settings that later calls depend on, as libusdr tracks them
/// (its `tx_*` fields).
///
/// The [`Board`] keeps it with the rest of the radio's state. Not `Copy`: lending a copy
/// would lose the call's updates.
#[derive(Debug, Default)]
pub(crate) struct TxState {
    /// The LMS6002D's transmitter is powered (libusdr's `tx_pwren`).
    pub(super) powered: bool,

    /// The caller fixed the TX bandwidth (libusdr's `tx_bw.set`), so frequency changes leave
    /// the filter alone.
    pub(super) bandwidth_fixed: bool,

    /// The frequency the caller tuned to, in Hz (libusdr's `tx_lo`); 0 until tuned.
    pub(super) lo_hz: u32,

    /// The caller fixed the band with [`Board::set_tx_path`], so frequency changes leave it
    /// alone (libusdr's `tx_rfic_path` other than `USDR_TX_AUTO`).
    pub(super) band_fixed: bool,

    /// The lowest LO the TX PLL was found to lock at below 250 MHz, in Hz (libusdr's
    /// `tx_minimal_lo`); 0 until a lock failure made the driver search for it.
    pub(super) minimal_lo_hz: u32,

    /// How far the PLL sits from the wanted LO, in Hz, when it cannot reach it; the TX NCOs
    /// make up the difference (libusdr's `tx_exten_lo`).
    pub(super) lo_offset_hz: i32,

    /// The IQ imbalance correction (libusdr's `tx_corr`), which TX stream creation writes
    /// again.
    pub(super) iq_imbalance: IqImbalance,
}

/// Which DSP chains the gateware has (HWID bits 25 and 24), and its TX stream core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Chains {
    /// An RX chain.
    pub(super) rx: bool,

    /// A TX chain.
    pub(super) tx: bool,

    /// The TX stream core is the extended one (`CORE_EXFETX_DMA32_R0_2`, HWID bit 26)
    /// rather than the original (`CORE_SFETX_DMA32_R0`).
    pub(super) extended_tx_core: bool,
}

impl fmt::Debug for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Board")
            .field("lms", &self.lms)
            .field("rev3", &self.rev3)
            .field("chains", &self.chains)
            .field("vio_boosted", &self.vio_boosted)
            .field("adc_rate_hz", &self.adc_rate_hz)
            .field("mixer_lo_hz", &self.mixer_lo_hz)
            .field("rx", &self.rx)
            .field("tx", &self.tx)
            .field("rx_running", &self.rx_running)
            .field("tx_running", &self.tx_running)
            .field("streams", &self.streams)
            .field("front_end", &self.front_end)
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

    /// The mixer LO is the Si5332 VCO divided by this (`si_vco_div`, set in `usdr_init`).
    pub(super) const MIXER_LO_DIVIDER: u8 = 8;

    /// Boost converter output, 3.45 V, the same whichever level the TPS63811's VSEL pin
    /// sits at.
    pub(super) const BOOST_VOLTAGE: OutputVoltage = OutputVoltage::from_millivolts(3450);

    /// The rails power-up brings up, in `usdr_init`'s order. The PMIC configuration is
    /// libusdr's `lp8758_ss(0)`: spread spectrum off; the same byte also sets the 105 °C
    /// die-temperature warning and drops the EN-pin pull-downs. Then channels 1 and 3 are
    /// set, all four run in forced PWM, and the boost converter starts in forced PWM.
    pub(super) const RAILS: RailPlan = RailPlan {
        pmic: Self::PMIC.dev(),
        config: Config::new().with_warn_at_105c(true),
        gpio: Self::VGPIO,
        lms_io: Self::LMS_VIO_NORMAL,
        control: BuckControl::new()
            .with_enabled(true)
            .with_discharge_when_off(true)
            .with_forced_pwm(true),
        boost: Self::BOOST.dev(),
        boost_forced_pwm: true,
        boost_voltage: Self::BOOST_VOLTAGE,
    };
}

impl Board {
    /// The Si5332 input `reference` selects on a board of this revision (`si5332_init`'s
    /// `ext_in2`).
    pub(crate) const fn si5332_reference(reference: ReferenceClock, rev3: bool) -> Reference {
        if reference.uses_input2(rev3) {
            Reference::Input2
        } else {
            Reference::Oscillator
        }
    }

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
