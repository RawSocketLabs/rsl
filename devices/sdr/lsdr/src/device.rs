//! The public device: open with its probes, ADC power, the sample rate, the receive PGA
//! and NCO, the temperature, the RX stream's create (`stream`), and close.

use std::fmt;

use hal::{Access, Sequence, SequenceAction, run_sequence};
use sdr_frontend::{Fe1005000Setting, FrontEnd, FrontEndKind};

use crate::board::{LSDR_SUPPLIES, LSDR_TEMPERATURE, LSDR_WIRING};
use crate::error::Error;
use crate::frontend;
use crate::lowlevel::Bus;
use crate::portable::adc_power::AdcPower;
use crate::portable::fpga::{self, DEAD, LIVENESS};
use crate::portable::lmk04832::ClockPlan;
use crate::portable::nco::{phase_increment, select_channels, set_nco};
use crate::portable::pga::{PgaGain, SetPga};
use crate::portable::probes::{Fitted, Probes};
use crate::portable::rate::{RateFinish, RatePlan, RatePrelude};
use crate::portable::si549::{self, FrequencyPlan, SetFrequency};
use crate::portable::single::Single;
use crate::portable::supplies::Supplies;

mod stream;

pub use stream::RxChannels;

/// The TMP108's name in errors.
const TMP108: &str = "TMP108";

/// The lowest frequency libusdr tunes fe1005000 to for `/dm/sdr/0/rx/frequency`, in Hz.
const FE1005000_MIN_HZ: u32 = 2_000_000;

/// How to open a board: libusdr's connection parameters that change what open does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenOptions {
    /// Test for an LMK04832 clock generator (libusdr's `lmk=1`); off by default, as in
    /// libusdr.
    probe_clock_generator: bool,
}

impl OpenOptions {
    /// libusdr's defaults: no clock-generator test.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            probe_clock_generator: false,
        }
    }

    /// Whether open tests for an LMK04832 on SPI bus 2 (libusdr's `lmk=1`).
    #[must_use]
    pub const fn probe_clock_generator(mut self, probe: bool) -> Self {
        self.probe_clock_generator = probe;
        self
    }
}

/// An open `LSDr`: a receive-only board with one or two `ADS42LBx9` ADCs.
///
/// Dropping a `Device` leaves the board as it is; [`Self::close`] powers it down as
/// libusdr's close does.
pub struct Device {
    /// The board's bus.
    bus: Box<dyn Bus>,

    /// What open found.
    fitted: Fitted,

    /// The front end the open's probe brought up, if any, with the state libusdr keeps
    /// for it.
    front_end: Option<FrontEnd>,

    /// The ADC's rate the last sample rate set, in S/s; 0 before any (libusdr's
    /// `adc_rate`, stored before the oscillator is tuned, so a refused rate keeps it).
    adc_rate: u32,
}

impl Device {
    /// Opens the board on `bus`, as libusdr's open does without an `fe=` hint: checks that
    /// the link is alive, reads the PMIC's identification, sets four rails, gates the
    /// Si549 sample clock, supplies the ADCs while it probes for them (and, when asked, for
    /// an LMK04832), cuts their supply again, then probes the front-end connector for
    /// fe1005000 and fe4ch4007200 and brings up the board that answers
    /// ([`Self::front_end`]). The ADCs stay unpowered ([`Self::set_power`]).
    ///
    /// # Errors
    ///
    /// [`Error::DeadLink`] when the status register reads all ones, before anything is
    /// driven; [`Error::Bus`] when an access fails, the liveness read included (libusdr
    /// ignores that read's failure and carries on); [`Error::FrontEndNotPorted`] when
    /// fe4ch4007200 answers, whose bring-up is not ported. A missing chip is not an error:
    /// see [`Self::fitted`]; a front end that fails to come up is logged and skipped, as
    /// libusdr does.
    pub fn open(bus: impl Bus + 'static, options: OpenOptions) -> Result<Self, Error> {
        let mut bus: Box<dyn Bus> = Box::new(bus);
        let mut liveness = Single::new(SequenceAction::Fpga(LIVENESS), fpga::NAME);
        run(&mut *bus, &mut liveness)?;
        if liveness.value() == DEAD {
            return Err(Error::DeadLink);
        }
        run(&mut *bus, &mut Supplies::new(LSDR_SUPPLIES))?;
        let mut probes = Probes::new(LSDR_WIRING, options.probe_clock_generator);
        run(&mut *bus, &mut probes)?;
        let front_end = frontend::probe(&mut *bus)?;
        Ok(Self {
            bus,
            fitted: probes.fitted(),
            front_end,
            adc_rate: 0,
        })
    }

    /// What open found on the board.
    #[must_use]
    pub fn fitted(&self) -> Fitted {
        self.fitted
    }

    /// The front end the open's probe brought up, if any: fe1005000, tuned to 900 MHz.
    #[must_use]
    pub fn front_end(&self) -> Option<FrontEndKind> {
        self.front_end.as_ref().map(FrontEnd::kind)
    }

    /// Applies one fe1005000 control (libusdr's `/debug/hw/fe_100_5000_cmd/0/reg` write,
    /// `ext_fe_100_5000_cmd_wr`): a frequency tunes the RF path, an attenuation sets the
    /// attenuator, a preselector the LNA switches. The setting is kept even when it fails,
    /// as libusdr keeps it.
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without fe1005000, before any bus access;
    /// [`Error::FrontEnd`] for a frequency below 20 MHz or above 6 GHz (libusdr's
    /// `-EINVAL`, before any bus access) or a failed access.
    pub fn configure_fe1005000(&mut self, setting: Fe1005000Setting) -> Result<(), Error> {
        let Some(FrontEnd::Fe1005000(board)) = &mut self.front_end else {
            return Err(Error::FrontEndMissing("fe1005000"));
        };
        Ok(board.apply(&mut *self.bus, setting)?)
    }

    /// Powers both ADCs up or down (libusdr's `/dm/power/en`): switches their supply, then
    /// pulses their resets and the receive FIFOs' and leaves the ADCs released (on) or held
    /// in reset (off). The sample clock stays gated.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] when an access fails; the rest of the sequence is not run.
    pub fn set_power(&mut self, on: bool) -> Result<(), Error> {
        run(&mut *self.bus, &mut AdcPower::new(LSDR_WIRING, on))
    }

    /// Sets the receive PGA's gain (libusdr's `/dm/sdr/0/rx/gain/pga`, and
    /// `/dm/sdr/0/rx/gain` with no front end).
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] when an access fails; the rest of the sequence is not run.
    pub fn set_rx_pga_gain(&mut self, gain: PgaGain) -> Result<(), Error> {
        run(&mut *self.bus, &mut SetPga::new(gain))
    }

    /// Sets the sample rate, in S/s (libusdr's `/dm/rate/master`; `/dm/rate/rxtxadcdac`
    /// passes its RX rate here and ignores the rest).
    ///
    /// As libusdr: the decimation is the largest of ×2–×6, ×8 and ×16–×128 that keeps the
    /// ADC under 260 MS/s (×2 when none does), the Si549 is tuned to the ADC rate (its
    /// output enabled) and the FIR gearbox loaded; when open found an LMK04832
    /// ([`Fitted::clock_generator`]), its PLL2 is laid out for the rate from `CLKin0` and
    /// both ADC ports clocked; after 100 ms the clock generator's status and lock are read
    /// (libusdr only logs them), the bitslip logic is reset, both ADCs put in normal mode,
    /// the DSP's mixer and the LVDS clock delay set and the DSP chain reset. The ADC rate
    /// is kept for the NCO ([`Self::set_rx_frequency`]) even when the rate fails.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedSampleRate`] when the ADC rate is outside the Si549's 200 kHz
    /// to 800 MHz, before any access; [`Error::ClockPlan`] when the LMK04832 has no plan,
    /// after the gearbox is loaded; [`Error::ChipId`] when the Si549 reads another device
    /// type, before it is written; [`Error::Bus`] when an access fails. A failure ends the
    /// rate, except that one in its last writes still takes the DSP reset's 10 µs wait, as
    /// libusdr's does.
    pub fn set_sample_rate(&mut self, rate: u32) -> Result<(), Error> {
        let unsupported = |chip| Error::UnsupportedSampleRate { rate, chip };
        let plan = RatePlan::new(rate);
        self.adc_rate = plan.adc_rate;
        let frequency = FrequencyPlan::new(plan.adc_rate).ok_or(unsupported(si549::NAME))?;
        let oscillator = SetFrequency::new(LSDR_WIRING.oscillator, frequency);
        run(&mut *self.bus, &mut RatePrelude::new(plan, oscillator))?;
        let clock = if self.fitted.clock_generator {
            Some(ClockPlan::new(rate, rate).map_err(|source| Error::ClockPlan { rate, source })?)
        } else {
            None
        };
        run(
            &mut *self.bus,
            &mut RateFinish::new(LSDR_WIRING, plan, clock),
        )
    }

    /// Sets the receive frequency, in Hz (libusdr's `/dm/sdr/0/rx/frequency`).
    ///
    /// With fe1005000 attached, its RF path is tuned to the frequency rounded down to a
    /// kHz, kept to 32 bits as libusdr's `unsigned` keeps it (so above 2³² Hz it wraps), at
    /// least 2 MHz, and the NCO takes the rest; without a front end, the frequency
    /// is the NCO's offset. The NCO folds the offset's low 32 bits into one ADC period as
    /// libusdr's integer arithmetic does (so an offset that wraps below 0 lands where
    /// libusdr's does), selects every channel, then writes the mixer and the phase
    /// increment.
    ///
    /// # Errors
    ///
    /// [`Error::FrontEnd`] when fe1005000 refuses the frequency (below 20 MHz or above
    /// 6 GHz) or a front-end access fails, before the NCO; [`Error::NoSampleRate`] before
    /// any sample rate, after the channel selection, as libusdr fails there;
    /// [`Error::Bus`] when an access fails, which ends the setting (libusdr carries on past
    /// a failed mixer write: a divergence, `PORTING.md`).
    pub fn set_rx_frequency(&mut self, frequency: u64) -> Result<(), Error> {
        let mut offset = frequency;
        if let Some(FrontEnd::Fe1005000(board)) = &mut self.front_end {
            let tuned = low32(frequency / 1000 * 1000).max(FE1005000_MIN_HZ);
            board.apply(&mut *self.bus, Fe1005000Setting::FrequencyKhz(tuned / 1000))?;
            offset = frequency.wrapping_sub(tuned.into());
        }
        run(
            &mut *self.bus,
            &mut Single::new(SequenceAction::Fpga(select_channels()), fpga::NAME),
        )?;
        let increment = phase_increment(self.adc_rate, low32(offset)).ok_or(Error::NoSampleRate)?;
        run(&mut *self.bus, &mut set_nco(increment))
    }

    /// The board's temperature in °C, from its TMP108 (libusdr's `/dm/sensor/temp`, which
    /// returns the sensor's word in 1/256 °C): one I2C transfer, the pointer to the
    /// temperature register written and two bytes read, most significant first.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] when the transfer fails.
    pub fn temperature(&mut self) -> Result<f32, Error> {
        let mut word = [0; 2];
        self.bus
            .i2c(LSDR_TEMPERATURE, &[0], &mut word)
            .map_err(|source| Error::Bus {
                chip: TMP108,
                access: Access::Read,
                reg: 0,
                source,
            })?;
        Ok(f32::from(i16::from_be_bytes(word)) / 256.0)
    }

    /// Closes the board as libusdr does: gates the sample clock, cuts the ADC supply, and
    /// tells both ADC sockets to power down. Every write is attempted whatever the others
    /// did, as in libusdr.
    ///
    /// # Errors
    ///
    /// The first failure among the four writes, [`Error::Bus`], after all four were
    /// attempted.
    pub fn close(mut self) -> Result<(), Error> {
        let mut first_failure = None;
        for mut write in LSDR_WIRING.close() {
            if let Err(error) = run(&mut *self.bus, &mut write) {
                first_failure.get_or_insert(error);
            }
        }
        first_failure.map_or(Ok(()), Err)
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("fitted", &self.fitted)
            .field("front_end", &self.front_end)
            .field("adc_rate", &self.adc_rate)
            .finish_non_exhaustive()
    }
}

/// The low 32 bits of `value`, as libusdr's assignments to `int` and `unsigned` keep them.
const fn low32(value: u64) -> u32 {
    let [a, b, c, d, ..] = value.to_le_bytes();
    u32::from_le_bytes([a, b, c, d])
}

/// Runs a portable sequence to its end over `bus`, with the failure in the driver's terms.
fn run(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    run_sequence(bus, sequence).map_err(Error::from)
}
