//! The LMS6002D driver: power-up configuration and the controls the board uses.

use super::afe::{self, ConverterEnables, Interface};
use super::dc_cal::{DcBlock, DcControl, DcStatus, DcValue};
use std::time::Duration;

use bnb::{u3, u4, u6};

use super::lpf::{self, Lpf};
use super::pll::{
    self, ChargePump, Comparator, DownOffset, FractionalDivider, Pll, PllConfig, VcoCapacitor,
    VcoRegulator, VcoSelect,
};
use super::rx_fe::{self, Lna, LnaControl, LnaGain, LnaLoad, MixerBias, MixerInput};
use super::rx_vga2;
use super::spi::SpiWriter;
use super::top::{ChipId, ClockEnables, EnableConfig, LpfTuning, LpfTuningControl, ReferencePower};
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
    /// The chip's name in errors and logs.
    pub(crate) const NAME: &'static str = "LMS6002D";

    /// The TX LO libusdr tunes to before the LPF tuning calibration, in Hz.
    const LPF_TUNING_TX_LO_HZ: u32 = 320_000_000;

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

    /// Powers RXVGA2 down the way libusdr's RX power-down does
    /// (`lms6002d_rxvga2_enable(false)`).
    pub(crate) fn disable_rx_vga2(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.on(bus).write(rx_vga2::Control::RX_POWER_DOWN)
    }

    /// libusdr's RX DC-offset calibration at stream creation (`_usdr_lms6002_dc_calib_rx`):
    /// the LPF's DC DAC, the LPF tuning for every bandwidth code (tuning the TX PLL to
    /// 320 MHz first, from a `reference_hz` reference), the TX and RX LPF offsets, RXVGA2's
    /// offsets, then RXVGA2's gains back to 0.
    ///
    /// Like libusdr, a calibration that does not converge is not an error, and the TX
    /// synthesizer's modulator clock stays on afterwards.
    ///
    /// # Errors
    ///
    /// [`Error::PllUnlocked`], [`Error::PllFault`] from the TX PLL tune; any bus failure
    /// outside the per-engine loops, whose own failures libusdr also discards.
    pub(crate) fn calibrate_rx_dc(
        &mut self,
        bus: &mut dyn Bus,
        reference_hz: u32,
    ) -> Result<(), Error> {
        self.calibrate_lpf_dac(bus)?;
        for code in 0..16 {
            self.tune_lpf_bandwidth(bus, u4::new(code), code == 0, reference_hz)?;
        }
        self.calibrate_lpf_dc(bus, Lpf::Tx)?;
        self.set_rx_external_termination(bus, true)?;
        self.calibrate_lpf_dc(bus, Lpf::Rx)?;
        self.calibrate_rx_vga2_dc(bus)?;
        self.set_rx_external_termination(bus, false)?;
        // libusdr's RXVGA2 gains, never changed from 0.
        self.on(bus).write(rx_vga2::Gain::new())
    }

    /// Calibrates the LPF's DC-offset DAC and writes the result to both filters
    /// (`lms6002d_cal_lpf`).
    fn calibrate_lpf_dac(&mut self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.clock_enables.set_lpf_calibration_clock(true);
        let mut regs = self.on(bus);
        regs.write(ConverterEnables::CALIBRATING)?;
        regs.write(self.clock_enables)?;
        // libusdr discards the loop's failures, bus errors included, and always cleans up.
        if let Ok(Some(value)) = calibration_loop(&mut regs, DcBlock::Lpf, 0) {
            let bypass = lpf::Bypass::new().with_dc_dac_calibration(u6::new(value));
            regs.write_to(Lpf::Rx, bypass)?;
            regs.write_to(Lpf::Tx, bypass)?;
        }
        self.clock_enables.set_lpf_calibration_clock(false);
        let mut regs = self.on(bus);
        regs.write(ConverterEnables::RUNNING)?;
        regs.write(self.clock_enables)
    }

    /// Runs the LPF tuning engine for bandwidth `code` with the transmitter on
    /// (`lms6002d_cal_lpf_bandwidth`). libusdr keeps the measured RC value only for its log;
    /// it is read and discarded here.
    fn tune_lpf_bandwidth(
        &mut self,
        bus: &mut dyn Bus,
        code: u4,
        tune_tx: bool,
        reference_hz: u32,
    ) -> Result<(), Error> {
        let tx_was_on = self.enable_config.tx_enabled();
        // `lms6002d_trf_enable(true)`.
        self.clock_enables.set_tx_pll_modulator_clock(true);
        self.enable_config.set_tx_enabled(true);
        let tuning = LpfTuning::new().with_bandwidth(code);
        // libusdr chains its steps on the first error but still waits, and still restores
        // the transmitter's cached state, after one.
        let mut result = self.write_tx_enables(bus);
        if tune_tx && result.is_ok() {
            result = self
                .tune(bus, Pll::Tx, Self::LPF_TUNING_TX_LO_HZ, reference_hz)
                .map(drop);
        }
        let mut regs = self.on(bus);
        if result.is_ok() {
            result = (|| {
                regs.write(LpfTuningControl::new())?;
                regs.write(tuning)?;
                regs.write(tuning.with_enabled(true))?;
                regs.write(LpfTuningControl::new().with_reset(true))?;
                regs.write(LpfTuningControl::new())
            })();
        }
        regs.sleep(Duration::from_micros(10));
        if result.is_ok() {
            result = regs.read_at::<DcStatus>(DcBlock::Lpf).map(drop);
        }
        if !tx_was_on {
            self.enable_config.set_tx_enabled(false);
        }
        result?;
        let mut regs = self.on(bus);
        regs.write(LpfTuningControl::new().with_clock_off(true))?;
        regs.write(tuning)?;
        regs.write(self.enable_config)
    }

    /// Writes the cached clock enables, then the cached enable configuration.
    fn write_tx_enables(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let mut regs = self.on(bus);
        regs.write(self.clock_enables)?;
        regs.write(self.enable_config)
    }

    /// Calibrates a filter's I and Q DC offsets (`lms6002d_cal_txrxlpfdc`). As in libusdr,
    /// a failed I channel skips Q, and neither failure, bus errors included, is reported.
    fn calibrate_lpf_dc(&mut self, bus: &mut dyn Bus, lpf: Lpf) -> Result<(), Error> {
        let block = match lpf {
            Lpf::Tx => DcBlock::TxLpf,
            Lpf::Rx => DcBlock::RxLpf,
        };
        self.clock_enables
            .set_rx_lpf_dc_calibration_clock(lpf == Lpf::Rx);
        self.clock_enables
            .set_tx_lpf_dc_calibration_clock(lpf == Lpf::Tx);
        let mut regs = self.on(bus);
        regs.write(self.clock_enables)?;
        if let Ok(Some(_)) = calibration_loop(&mut regs, block, 0) {
            let _ = calibration_loop(&mut regs, block, 1);
        }
        self.clock_enables.set_lpf_calibration_clock(false);
        self.clock_enables.set_rx_lpf_dc_calibration_clock(false);
        self.clock_enables.set_tx_lpf_dc_calibration_clock(false);
        self.on(bus).write(self.clock_enables)
    }

    /// Calibrates RXVGA2's reference and both stages' I and Q offsets
    /// (`lms6002d_cal_vga2`). The first failure skips the rest; libusdr reports none.
    fn calibrate_rx_vga2_dc(&mut self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.clock_enables.set_rx_vga2_dc_calibration_clock(true);
        let stage_a = rx_vga2::Gain::new().with_a(u4::new(10));
        let mut regs = self.on(bus);
        let _ = calibrate_vga2_channels(&mut regs, self.clock_enables, stage_a);
        self.clock_enables.set_rx_vga2_dc_calibration_clock(false);
        let mut regs = self.on(bus);
        regs.write(
            rx_vga2::CalibrationPower::new()
                .with_b_off(true)
                .with_a_off(true),
        )?;
        regs.write(self.clock_enables)?;
        // libusdr re-sends its whole setup array, whose third word is still stage A's gain.
        regs.write(stage_a)
    }

    /// Terminates the RX mixer's external input and routes the mixer to the pads, or
    /// undoes both (`lms6002d_set_rx_extterm`). libusdr also zeroes the I-channel DC trim and
    /// the mixer's LO bias (its reset value is 3).
    fn set_rx_external_termination(&self, bus: &mut dyn Bus, on: bool) -> Result<(), Error> {
        let mut regs = self.on(bus);
        regs.write(MixerInput::new().with_from_lna(!on))?;
        regs.write(MixerBias::new().with_input_terminated(on))
    }

    /// Sets the RX channel filter for `bandwidth_hz` from libusdr's table, bypassing it
    /// above 47 MHz (`lms6002d_set_bandwidth`).
    pub(crate) fn set_rx_bandwidth(
        &self,
        bus: &mut dyn Bus,
        bandwidth_hz: u32,
    ) -> Result<(), Error> {
        let (bandwidth, bypass, control) = lpf::for_bandwidth(bandwidth_hz);
        let mut regs = self.on(bus);
        regs.write(bandwidth)?;
        regs.write_to(Lpf::Rx, bypass)?;
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
        self.tune(bus, Pll::Rx, lo_hz, reference_hz)
    }

    /// Tunes either synthesizer, as [`Self::tune_rx`] describes. The TX synthesizer has no
    /// LO buffer select, and its result is not cached.
    fn tune(
        &mut self,
        bus: &mut dyn Bus,
        pll: Pll,
        lo_hz: u32,
        reference_hz: u32,
    ) -> Result<CapacitorWindow, Error> {
        if lo_hz < pll::LOWEST_LO_HZ {
            return Err(Error::UnsupportedFrequency(lo_hz));
        }
        let (vco, exponent) = pll::vco_for(lo_hz);
        let divider = FractionalDivider::for_vco(reference_hz, u64::from(lo_hz) << (exponent + 1));
        let divider_bytes = divider.to_raw().to_be_bytes();
        let lo_buffer = match pll {
            Pll::Tx => Lna::None,
            Pll::Rx => self.rx_vco.lo_buffer(),
        };
        let tuned = VcoSelect::new()
            .with_vco(vco)
            .with_divider_range(u3::new(exponent | 0b100))
            .with_lo_buffer(lo_buffer);
        match pll {
            Pll::Tx => self.clock_enables.set_tx_pll_modulator_clock(true),
            Pll::Rx => self.clock_enables.set_rx_pll_modulator_clock(true),
        }

        let mut regs = self.on(bus);
        regs.write(self.clock_enables)?;
        for (reg, byte) in pll.divider().into_iter().zip(divider_bytes) {
            regs.write_raw(reg, byte)?;
        }
        regs.write_to(pll, PllConfig::TUNING)?;
        // The VCO first with no divider range or LO buffer, then all three.
        regs.write_to(pll, VcoSelect::new().with_vco(vco))?;
        regs.write_to(pll, tuned)?;
        regs.write_to(pll, ChargePump::TUNING)?;
        regs.write_to(pll, VcoRegulator::TUNING)?;
        regs.write_to(pll, DownOffset::TUNING)?;
        regs.write_to(pll, capacitor(32))?;
        regs.write_raw(pll.comparator_power(), pll::COMPARATOR_ON)?;
        regs.sleep(Duration::from_micros(100));

        // libusdr ignores read failures here; its zeroed buffer then fails the comparison.
        let mut readback = [0; 4];
        for (reg, byte) in pll.divider().into_iter().zip(&mut readback) {
            *byte = regs.read_raw(reg).unwrap_or(0);
        }
        if readback != divider_bytes {
            return Err(Error::PllFault);
        }

        let window = find_capacitor(&mut regs, pll)?;
        // Mid-window, rounding down as libusdr's integer division does.
        let mid = u8::midpoint(window.low, window.high);
        regs.write_to(pll, capacitor(i32::from(mid)))?;
        regs.write_to(pll, tuned)?;
        regs.write_raw(pll.comparator_power(), pll::COMPARATOR_OFF)?;
        if pll == Pll::Rx {
            self.rx_vco = tuned;
        }
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

/// libusdr's RXVGA2 calibration steps between its setup and cleanup: comparators on, the
/// calibration clock, stage A at 10, the reference and stage A's I and Q, then stage B at
/// 10 and its I and Q. Stops at the first failure: `Ok(false)` if a channel did not
/// converge.
fn calibrate_vga2_channels(
    regs: &mut SpiWriter<'_>,
    clock_enables: ClockEnables,
    stage_a: rx_vga2::Gain,
) -> Result<bool, Error> {
    regs.write(rx_vga2::CalibrationPower::new())?;
    regs.write(clock_enables)?;
    regs.write(stage_a)?;
    for channel in 0..3 {
        if calibration_loop(regs, DcBlock::RxVga2, channel)?.is_none() {
            return Ok(false);
        }
    }
    regs.write(rx_vga2::Gain::new().with_b(u4::new(10)))?;
    for channel in 3..5 {
        if calibration_loop(regs, DcBlock::RxVga2, channel)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// libusdr's `lms6002d_calibration_loop`: starts `block`'s engine on `channel` from 31, then
/// polls every 7 µs, up to 100 times, until it finishes with an unsaturated value, which it
/// returns. A saturated result (0x1F) restarts the engine from 0. `None` if it never
/// converged.
fn calibration_loop(
    regs: &mut SpiWriter<'_>,
    block: DcBlock,
    channel: u8,
) -> Result<Option<u8>, Error> {
    /// The polls before libusdr gives up.
    const POLLS: usize = 100;
    /// A saturated result.
    const SATURATED: u8 = 0x1f;
    let start = |regs: &mut SpiWriter<'_>, from: u8| -> Result<(), Error> {
        let control = DcControl::new()
            .with_released(true)
            .with_channel(u3::new(channel));
        regs.write_to(block, DcValue::new().with_value(u6::new(from)))?;
        regs.write_to(block, control)?;
        regs.write_to(block, control.with_start(true))?;
        regs.write_to(block, control)
    };
    start(regs, SATURATED)?;
    for _ in 0..POLLS {
        regs.sleep(Duration::from_micros(7));
        let status: DcStatus = regs.read_at(block)?;
        if status.running() {
            continue;
        }
        let value: DcValue = regs.read_at(block)?;
        let value = value.to_raw() & SATURATED;
        if value == SATURATED {
            start(regs, 0)?;
            continue;
        }
        return Ok(Some(value));
    }
    Ok(None)
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

/// libusdr's `lms6002d_find_vcocap`: a five-step binary search from code 32, then a
/// linear scan up from its result until the comparator reports the capacitor too high.
///
/// In the binary phase an in-window reading counts as too high, as libusdr's switch falls
/// through; code 0 is never probed. Each probe writes the code, waits 150 µs and reads the
/// comparator.
fn find_capacitor(regs: &mut SpiWriter<'_>, pll: Pll) -> Result<CapacitorWindow, Error> {
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
        regs.write_to(pll, capacitor(code))?;
        regs.sleep(Duration::from_micros(150));
        let comparator: Comparator = regs.read_at(pll)?;
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
