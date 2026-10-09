//! The LMS6002D driver: power-up configuration and the controls the board uses.

use super::afe::{self, Interface};
use super::pll::{Pll, VcoRegulator, VcoSelect};
use super::rx_fe::{self, Lna, LnaControl, LnaGain, LnaLoad};
use super::rx_vga2;
use super::spi::SpiRegisters;
use super::top::{ChipId, ClockEnables, EnableConfig, ReferencePower};
use super::tx_rf::{Bias, PaSelect, PowerAmp};

use crate::error::Error;
use crate::lowlevel::{Bus, SpiAddr};

/// An initialised LMS6002D, with the values the driver last wrote to registers it later
/// updates field by field (the chip's registers are not read back).
#[derive(Debug)]
pub(crate) struct Lms6002d {
    /// The chip's registers.
    regs: SpiRegisters,

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
    /// The SPI target where the uSDR wires it: FPGA SPI bus 0 (`SPI_LMS6` in libusdr).
    pub(crate) const USDR_TARGET: SpiAddr = SpiAddr(0);

    /// Reads the raw chip ID (version and revision) from the chip on `target`, configuring
    /// nothing.
    pub(crate) fn read_id(bus: &mut dyn Bus, target: SpiAddr) -> Result<u8, Error> {
        let id: ChipId = SpiRegisters::new(target).on(bus).read()?;
        Ok(id.to_raw())
    }

    /// Reads the chip ID, then writes the power-up configuration (`lms6002d_create`).
    pub(crate) fn create(bus: &mut dyn Bus, target: SpiAddr) -> Result<Self, Error> {
        let lms = Self {
            regs: SpiRegisters::new(target),
            enable_config: EnableConfig::new()
                .with_modulators_running(true)
                .with_top_enabled(true)
                .with_four_wire_spi(true),
            clock_enables: ClockEnables::new(),
            rx_vco: VcoSelect::new().with_lo_buffer(Lna::Lna1),
            lna_control: LnaControl::new().with_gain(LnaGain::Max),
            pa_select: PaSelect::new(),
        };

        let mut regs = lms.regs.on(bus);
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
            0xff => Err(Error::ChipMissing("LMS6002D")),
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

    /// Makes `lna` the active RX input and powers the RX synthesizer's LO buffer for it.
    pub(crate) fn select_lna(&mut self, bus: &mut dyn Bus, lna: Lna) -> Result<(), Error> {
        self.rx_vco.set_lo_buffer(lna);
        self.lna_control.set_active(lna);
        let mut regs = self.regs.on(bus);
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
        self.regs.on(bus).write(self.pa_select)
    }

    /// Writes [`ClockEnables`] then [`EnableConfig`] from the cached values.
    fn write_enables(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let mut regs = self.regs.on(bus);
        regs.write(self.clock_enables)?;
        regs.write(self.enable_config)
    }
}
