//! The LMS6002D driver: power-up configuration and the controls the board uses.

use super::afe::{self, Interface};
use super::pll::{Pll, VcoRegulator, VcoSelect};
use super::rx_fe::{self, Lna, LnaControl, LnaGain, LnaLoad};
use super::rx_vga2;
use super::spi::{BlockReg, SpiWord};
use super::top::{ChipId, ClockEnables, EnableConfig, ReferencePower};
use super::tx_rf::{Bias, PaSelect, PowerAmp};
use bnb::{u4, u5, u6};

use crate::chips::register::Register;
use crate::error::{BusContext, Error};
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
    /// The SPI target where the uSDR wires it: FPGA SPI bus 0 (`SPI_LMS6` in libusdr).
    pub(crate) const USDR_TARGET: SpiAddr = SpiAddr(0);

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
        let chip_id: ChipId = lms.read(bus)?;
        // Regulators bypassed and off, band gap settling fast; RX charge-pump offset 30 µA.
        let vco_regulator = VcoRegulator::new()
            .with_regulator_bypassed(true)
            .with_regulator_off(true)
            .with_fast_bandgap_settling(true);
        lms.post(
            bus,
            &[
                // Modulators held in soft reset, top modules off, while the SPI mode is set.
                SpiWord::store(EnableConfig::new().with_four_wire_spi(true)),
                SpiWord::store(lms.enable_config),
                SpiWord::store(lms.clock_enables),
                SpiWord::store(
                    ReferencePower::new()
                        .with_xco_buffer_self_biased(true)
                        .with_lpf_calibration_reference_off(true),
                ),
                SpiWord::store_at(Pll::Tx, vco_regulator),
                SpiWord::store_at(
                    Pll::Rx,
                    vco_regulator.with_charge_pump_up_offset(u5::new(3)),
                ),
                SpiWord::store(rx_fe::Control::new().with_enabled(true)),
                // Lime LMS6002D FAQ v1.0r12, 5.27: TX LO buffer current, ADC reference,
                // RXVGA2 common mode, LNA load.
                SpiWord::store(Bias::new().with_lo_buffer_current(u4::new(4))),
                SpiWord::store_raw(afe::Reg::AdcReference, 0x29),
                SpiWord::store(
                    rx_vga2::Control::new()
                        .with_common_mode(u4::new(0b1101))
                        .with_enabled(true),
                ),
                SpiWord::store(LnaLoad::new().with_resistance(u6::new(0x37))),
                // I then Q, both frame-sync polarity bits set, DAC on the negative edge, ADC on the
                // positive edge (its reset is negative).
                SpiWord::store(
                    Interface::new()
                        .with_rx_frame_sync_polarity(true)
                        .with_dac_negative_edge(true)
                        .with_tx_frame_sync_polarity(true),
                ),
            ],
        )?;
        // libusdr checks the ID only after configuring.
        if chip_id.to_raw() == 0xff {
            return Err(Error::ChipMissing("LMS6002D"));
        }
        Ok(lms)
    }

    /// Powers the transmitter and clocks its synthesizer's modulator, or turns both off.
    pub(crate) fn set_tx_enabled(&mut self, bus: &mut dyn Bus, enable: bool) -> Result<(), Error> {
        self.clock_enables.set_tx_pll_modulator_clock(enable);
        self.enable_config.set_tx_enabled(enable);
        self.post_enables(bus)
    }

    /// Powers the receiver and clocks its synthesizer's modulator, or turns both off.
    pub(crate) fn set_rx_enabled(&mut self, bus: &mut dyn Bus, enable: bool) -> Result<(), Error> {
        self.clock_enables.set_rx_pll_modulator_clock(enable);
        self.enable_config.set_rx_enabled(enable);
        self.post_enables(bus)
    }

    /// Makes `lna` the active RX input and powers the RX synthesizer's LO buffer for it.
    pub(crate) fn select_lna(&mut self, bus: &mut dyn Bus, lna: Lna) -> Result<(), Error> {
        self.rx_vco.set_lo_buffer(lna);
        self.lna_control.set_active(lna);
        self.post(
            bus,
            &[
                SpiWord::store_at(Pll::Rx, self.rx_vco),
                SpiWord::store(self.lna_control),
            ],
        )
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
        self.post(bus, &[SpiWord::store(self.pa_select)])
    }

    /// Writes [`ClockEnables`] then [`EnableConfig`] from the cached values.
    fn post_enables(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.post(
            bus,
            &[
                SpiWord::store(self.clock_enables),
                SpiWord::store(self.enable_config),
            ],
        )
    }

    /// Reads a typed register.
    fn read<R: Register<Map: BlockReg>>(&self, bus: &mut dyn Bus) -> Result<R, Error> {
        let read = bus
            .spi32(self.target, SpiWord::load::<R>().to_raw().into())
            .during("LMS6002D read")?;
        Ok(R::from(read.to_le_bytes()[0]))
    }

    /// Sends register writes in order.
    fn post(&self, bus: &mut dyn Bus, words: &[SpiWord]) -> Result<(), Error> {
        for word in words {
            bus.spi32(self.target, word.to_raw().into())
                .during("LMS6002D write")?;
        }
        Ok(())
    }
}
