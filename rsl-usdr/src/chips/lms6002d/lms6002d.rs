//! The LMS6002D driver: power-up configuration and the controls the board uses.

use super::afe::{self, Interface};
use std::time::Duration;

use bnb::{u3, u6};

use super::pll::{
    self, ChargePump, Comparator, DownOffset, FractionalDivider, Pll, PllConfig, VcoCapacitor,
    VcoRegulator, VcoSelect,
};
use super::rx_fe::{self, Lna, LnaControl, LnaGain, LnaLoad};
use super::rx_lpf;
use super::rx_vga2;
use super::spi::SpiWriter;
use super::top::{ChipId, ClockEnables, EnableConfig, ReferencePower};
use super::tx_rf::{Bias, PaSelect, PowerAmp};

use crate::error::Error;
use crate::lowlevel::{Bus, SpiAddr};

/// An initialised LMS6002D, with the values the driver last wrote to registers it later
/// updates field by field (the chip's registers are not read back).
#[derive(Debug)]
pub(crate) struct Lms6002d {
    /// SPI target the chip is on.
    target: SpiAddr,

    /// Last [`EnableConfig`] written.
    enable_config: EnableConfig,

    /// Last [`ClockEnables`] written.
    clock_enables: ClockEnables,

    /// Last RX synthesizer [`VcoSelect`] written.
    rx_vco: VcoSelect,

    /// Last [`LnaControl`] written.
    lna_control: LnaControl,

    /// Last [`PaSelect`] written.
    pa_select: PaSelect,
}

impl Lms6002d {
    /// The chip's name in errors.
    const NAME: &'static str = "LMS6002D";

    /// The SPI target where the uSDR wires it: FPGA SPI bus 0 (`SPI_LMS6` in libusdr).
    pub(crate) const USDR_TARGET: SpiAddr = SpiAddr(0);

    /// Reads the raw chip ID (version and revision) from the chip on `target`, configuring
    /// nothing.
    pub(crate) fn read_id(bus: &mut dyn Bus, target: SpiAddr) -> Result<u8, Error> {
        let id: ChipId = SpiWriter::new(target, Self::NAME, bus).read()?;
        Ok(id.to_raw())
    }

    /// Reads the chip ID, then writes the power-up configuration (`lms6002d_create`).
    pub(crate) fn create(bus: &mut dyn Bus, target: SpiAddr) -> Result<Self, Error> {
        let lms = Self {
            target,
            enable_config: EnableConfig::new()
                .with_modulators_running(true)
                .with_top_enabled(true)
                .with_four_wire_spi(true),
            clock_enables: ClockEnables::new(),
            rx_vco: VcoSelect::new().with_lo_buffer(Lna::Lna1),
            lna_control: LnaControl::new().with_gain(LnaGain::Max),
            pa_select: PaSelect::new(),
        };

        let mut regs = lms.on(bus);
        let chip_id: ChipId = regs.read()?;

        // Modulators held in soft reset, top modules off, while the SPI mode is set.
        regs.write(EnableConfig::new().with_four_wire_spi(true))?;
        regs.write(lms.enable_config)?;
        regs.write(lms.clock_enables)?;
        regs.write(ReferencePower::SELF_BIASED)?;
        regs.write_to(Pll::Tx, VcoRegulator::BYPASSED)?;
        regs.write_to(Pll::Rx, VcoRegulator::BYPASSED_WITH_UP_OFFSET)?;
        regs.write(rx_fe::Control::new().with_enabled(true))?;

        // Lime LMS6002D FAQ v1.0r12, 5.27.
        regs.write(Bias::LIME_RECOMMENDED)?;
        regs.write(afe::AdcReference::LIME_RECOMMENDED)?;
        regs.write(rx_vga2::Control::LIME_RECOMMENDED)?;
        regs.write(LnaLoad::LIME_RECOMMENDED)?;
        regs.write(Interface::I_FIRST)?;

        // libusdr checks the ID only after configuring.
        match chip_id.to_raw() {
            0xff => Err(Error::ChipMissing(Self::NAME)),
            _ => Ok(lms),
        }
    }

    /// Powers the transmitter and clocks its synthesizer's modulator, or turns both off.
    pub(crate) fn set_tx_enabled(&mut self, bus: &mut dyn Bus, enable: bool) -> Result<(), Error> {
        self.clock_enables.set_tx_pll_modulator_clock(enable);
        self.enable_config.set_tx_enabled(enable);
        self.write_enables(bus)
    }

    /// Powers the receiver and clocks its synthesizer's modulator, or turns both off.
    pub(crate) fn set_rx_enabled(&mut self, bus: &mut dyn Bus, enable: bool) -> Result<(), Error> {
        self.clock_enables.set_rx_pll_modulator_clock(enable);
        self.enable_config.set_rx_enabled(enable);
        self.write_enables(bus)
    }

    /// Powers RXVGA2 the way libusdr's RX power-up does (`lms6002d_rxvga2_enable(true)`).
    pub(crate) fn enable_rx_vga2(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.on(bus).write(rx_vga2::Control::RX_POWER_UP)
    }

    /// Sets the RX channel filter for `bandwidth_hz` from libusdr's table, bypassing it
    /// above 47 MHz (`lms6002d_set_bandwidth`).
    pub(crate) fn set_rx_bandwidth(
        &self,
        bus: &mut dyn Bus,
        bandwidth_hz: u32,
    ) -> Result<(), Error> {
        let (bandwidth, bypass, control) = rx_lpf::for_bandwidth(bandwidth_hz);
        let mut regs = self.on(bus);
        regs.write(bandwidth)?;
        regs.write(bypass)?;
        regs.write(control)
    }

    /// Tunes the RX synthesizer's LO to `lo_hz` from a `reference_hz` reference, and returns
    /// the capacitor codes it locked over (`lms6002d_tune_pll_stat` for RX).
    ///
    /// Picks the VCO and divider, writes the fractional divider, checks it reads back, then
    /// searches the VCO capacitor bank against the tuning-voltage comparator and settles
    /// mid-window.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedFrequency`] below 170 MHz, before any write;
    /// [`Error::PllUnlocked`] if no code locks, after the writes, as libusdr;
    /// [`Error::PllFault`] if the divider reads back wrong or the comparator reports both
    /// bits; any bus failure.
    pub(crate) fn tune_rx(
        &mut self,
        bus: &mut dyn Bus,
        lo_hz: u32,
        reference_hz: u32,
    ) -> Result<CapacitorWindow, Error> {
        if lo_hz < pll::LOWEST_LO_HZ {
            return Err(Error::UnsupportedFrequency(lo_hz));
        }
        let (vco, exponent) = pll::vco_for(lo_hz);
        let divider = FractionalDivider::for_vco(reference_hz, u64::from(lo_hz) << (exponent + 1));
        let divider_bytes = divider.to_raw().to_be_bytes();
        let tuned = VcoSelect::new()
            .with_vco(vco)
            .with_divider_range(u3::new(exponent | 0b100))
            .with_lo_buffer(self.rx_vco.lo_buffer());
        self.clock_enables.set_rx_pll_modulator_clock(true);

        let mut regs = self.on(bus);
        regs.write(self.clock_enables)?;
        for (reg, byte) in pll::RX_DIVIDER.into_iter().zip(divider_bytes) {
            regs.write_raw(reg, byte)?;
        }
        regs.write_to(Pll::Rx, PllConfig::TUNING)?;
        // The VCO first with no divider range or LO buffer, then all three.
        regs.write_to(Pll::Rx, VcoSelect::new().with_vco(vco))?;
        regs.write_to(Pll::Rx, tuned)?;
        regs.write_to(Pll::Rx, ChargePump::TUNING)?;
        regs.write_to(Pll::Rx, VcoRegulator::TUNING)?;
        regs.write_to(Pll::Rx, DownOffset::TUNING)?;
        regs.write_to(Pll::Rx, capacitor(32))?;
        regs.write_raw(pll::Reg::RxComparatorPower, pll::COMPARATOR_ON)?;
        regs.sleep(Duration::from_micros(100));

        // libusdr ignores read failures here; its zeroed buffer then fails the comparison.
        let mut readback = [0; 4];
        for (reg, byte) in pll::RX_DIVIDER.into_iter().zip(&mut readback) {
            *byte = regs.read_raw(reg).unwrap_or(0);
        }
        if readback != divider_bytes {
            return Err(Error::PllFault);
        }

        let window = find_rx_capacitor(&mut regs)?;
        // Mid-window, rounding down as libusdr's integer division does.
        let mid = u8::midpoint(window.low, window.high);
        regs.write_to(Pll::Rx, capacitor(i32::from(mid)))?;
        regs.write_to(Pll::Rx, tuned)?;
        regs.write_raw(pll::Reg::RxComparatorPower, pll::COMPARATOR_OFF)?;
        self.rx_vco = tuned;
        // libusdr's lock test: the search ended with an empty window at the bottom.
        if window.low > window.high && window.high == 0 {
            return Err(Error::PllUnlocked(lo_hz));
        }
        Ok(window)
    }

    /// Makes `lna` the active RX input and powers the RX synthesizer's LO buffer for it.
    pub(crate) fn select_lna(&mut self, bus: &mut dyn Bus, lna: Lna) -> Result<(), Error> {
        self.rx_vco.set_lo_buffer(lna);
        self.lna_control.set_active(lna);
        let mut regs = self.on(bus);
        regs.write_to(Pll::Rx, self.rx_vco)?;
        regs.write(self.lna_control)
    }

    /// Makes `amplifier` the TX output and clears `ENAUX`, as libusdr does for PA1 and PA2;
    /// see [`PaSelect`] on that bit's disputed polarity.
    pub(crate) fn select_pa(
        &mut self,
        bus: &mut dyn Bus,
        amplifier: PowerAmp,
    ) -> Result<(), Error> {
        self.pa_select.set_amplifier(amplifier);
        self.pa_select.set_aux_pa_off(false);
        self.on(bus).write(self.pa_select)
    }

    /// This chip's registers on `bus`, for the accesses of one operation.
    fn on<'a>(&self, bus: &'a mut dyn Bus) -> SpiWriter<'a> {
        SpiWriter::new(self.target, Self::NAME, bus)
    }

    /// Writes [`ClockEnables`] then [`EnableConfig`] from the cached values.
    fn write_enables(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let mut regs = self.on(bus);
        regs.write(self.clock_enables)?;
        regs.write(self.enable_config)
    }
}

/// The capacitor codes a synthesizer's VCO locked over, inclusive. `low` can exceed `high`
/// when the search found no lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CapacitorWindow {
    /// The lowest code that locked.
    pub(crate) low: u8,

    /// The highest code that locked.
    pub(crate) high: u8,
}

/// A [`VcoCapacitor`] value for `code`, keeping its low six bits as libusdr's macro does.
fn capacitor(code: i32) -> VcoCapacitor {
    let code = u8::try_from(code & 0x3f).expect("invariant: masked to six bits");
    VcoCapacitor::new().with_capacitor(u6::new(code))
}

/// libusdr's `lms6002d_find_vcocap` for RX: a five-step binary search from code 32, then a
/// linear scan up from its result until the comparator reports the capacitor too high.
///
/// In the binary phase an in-window reading counts as too high, as libusdr's switch falls
/// through; code 0 is never probed. Each probe writes the code, waits 150 µs and reads the
/// comparator.
fn find_rx_capacitor(regs: &mut SpiWriter<'_>) -> Result<CapacitorWindow, Error> {
    let (mut step, mut code, mut low, mut high) = (4_i32, 32_i32, 0_i32, -1_i32);
    let mut binary = true;
    loop {
        if binary && step < 0 {
            binary = false;
            low = code;
        }
        if !binary && code >= 64 {
            if high == -1 {
                high = 0;
            }
            break;
        }
        regs.write_to(Pll::Rx, capacitor(code))?;
        regs.sleep(Duration::from_micros(150));
        let comparator: Comparator = regs.read_at(Pll::Rx)?;
        match (comparator.vtune_high(), comparator.vtune_low()) {
            (true, true) => return Err(Error::PllFault),
            // Capacitor too low: raise it, or move the window's bottom up.
            (true, false) if binary => code += 1 << step,
            (true, false) => low = code + 1,
            // In the window.
            (false, false) if !binary => high = code,
            // Too high, or in the window during the binary phase.
            _ if binary => code -= 1 << step,
            _ => {
                high = (code - 1).max(0);
                break;
            }
        }
        if binary {
            step -= 1;
        } else {
            code += 1;
        }
    }
    let byte = |value: i32| u8::try_from(value).expect("invariant: the search stays in 0..=64");
    Ok(CapacitorWindow {
        low: byte(low),
        high: byte(high),
    })
}
