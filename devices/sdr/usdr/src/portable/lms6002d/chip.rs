//! The LMS6002D's cached state and its straight-line operations: each updates the cache as
//! libusdr's `lms6002d.c` does and returns the [`Script`] of register accesses that applies
//! it. The searches and calibrations with reads that steer them are machines of their
//! own (`tune`, `calibration`).

use bnb::{u4, u5, u7};

use super::super::address::SpiAddr;
use super::super::register::{IndexedRegister, Register};
use super::super::script::Script;
use super::super::sequence::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};
use super::afe::{self, Interface};
use super::lpf::{self, Lpf};
use super::pll::{Pll, PllConfig, VcoRegulator, VcoSelect};
use super::rx_fe::{
    self, ExternalLoad, InternalLoad, Ip2TrimQ, Lna, LnaControl, LnaGain, LnaLoad, MixerInput,
    PowerDown, TiaFeedback,
};
use super::rx_vga2;
use super::spi::{self, AnyReg, BlockReg};
use super::top::{ChipId, ClockEnables, EnableConfig, Loopback, ReferencePower};
use super::tx_rf::{Bias, IqChannel, PaSelect, PowerAmp, Vga1DcShift, Vga1Gain, Vga2Gain};

/// The chip's name in failures and logs.
pub(crate) const NAME: &str = "LMS6002D";

/// An initialised LMS6002D, with the values the driver last wrote to registers it later
/// updates field by field (the chip's registers are not read back). Small and `Copy`: a
/// machine that changes it owns a copy and hands it back.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lms6002d {
    /// SPI target the chip is on.
    target: SpiAddr,

    /// Last [`EnableConfig`] written.
    pub(super) enable_config: EnableConfig,

    /// Last [`ClockEnables`] written.
    pub(super) clock_enables: ClockEnables,

    /// Last RX synthesizer [`VcoSelect`] written.
    pub(super) rx_vco: VcoSelect,

    /// Last [`LnaControl`] written.
    lna_control: LnaControl,

    /// Last [`PaSelect`] written.
    pa_select: PaSelect,

    /// Last TXVGA1 gain written, which libusdr's TX IQ calibration restores
    /// (`trf_vga1_gain`). Code 21 from power-up, as libusdr assumes without writing it.
    tx_vga1: Vga1Gain,
}

impl Lms6002d {
    /// The SPI target where the uSDR wires it: FPGA SPI bus 0 (`SPI_LMS6` in libusdr).
    pub(crate) const USDR_TARGET: SpiAddr = SpiAddr(0);

    /// The chip on `target` as `lms6002d_create` leaves it, and the [`Create`] machine
    /// that reads its ID and writes that power-up configuration.
    pub(crate) fn create(target: SpiAddr) -> (Self, Create) {
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
            tx_vga1: Vga1Gain::new().with_code(u5::new(21)),
        };
        let script = Script::new()
            .fpga(NAME, spi::read(target, ChipId::ADDR))
            // Modulators held in soft reset, top modules off, while the SPI mode is set.
            .fpga(
                NAME,
                lms.write(EnableConfig::new().with_four_wire_spi(true)),
            )
            .fpga(NAME, lms.write(lms.enable_config))
            .fpga(NAME, lms.write(lms.clock_enables))
            .fpga(NAME, lms.write(ReferencePower::SELF_BIASED))
            .fpga(NAME, lms.write_at(Pll::Tx, VcoRegulator::BYPASSED))
            .fpga(
                NAME,
                lms.write_at(Pll::Rx, VcoRegulator::BYPASSED_WITH_UP_OFFSET),
            )
            .fpga(NAME, lms.write(rx_fe::Control::new().with_enabled(true)))
            // Lime LMS6002D FAQ v1.0r12, 5.27.
            .fpga(NAME, lms.write(Bias::LIME_RECOMMENDED))
            .fpga(NAME, lms.write(afe::AdcReference::LIME_RECOMMENDED))
            .fpga(NAME, lms.write(rx_vga2::Control::LIME_RECOMMENDED))
            .fpga(NAME, lms.write(LnaLoad::LIME_RECOMMENDED))
            .fpga(NAME, lms.write(Interface::I_FIRST));
        (lms, Create(script))
    }

    /// Reads the raw chip ID (version and revision) from the chip on `target`, configuring
    /// nothing; the script's [`Script::read`] holds the reply.
    pub(crate) fn read_id(target: SpiAddr) -> Script {
        Script::new().fpga(NAME, spi::read(target, ChipId::ADDR))
    }

    /// Reads register `reg` raw, for debugging; the cache is not consulted.
    pub(crate) fn read_register_raw(&self, reg: u7) -> Script {
        Script::new().fpga(NAME, spi::read(self.target, AnyReg(reg)))
    }

    /// Writes register `reg` raw, for debugging. The cache is not updated: a later field
    /// update writes it back over a register written here.
    pub(crate) fn write_register_raw(&self, reg: u7, value: u8) -> Script {
        Script::new().fpga(NAME, spi::write(self.target, AnyReg(reg), value))
    }

    /// Powers the transmitter and clocks its synthesizer's modulator, or turns both off.
    pub(crate) fn set_tx_enabled(&mut self, enable: bool) -> Script {
        self.clock_enables.set_tx_pll_modulator_clock(enable);
        self.enable_config.set_tx_enabled(enable);
        self.enables()
    }

    /// Powers the receiver and clocks its synthesizer's modulator, or turns both off.
    pub(crate) fn set_rx_enabled(&mut self, enable: bool) -> Script {
        self.clock_enables.set_rx_pll_modulator_clock(enable);
        self.enable_config.set_rx_enabled(enable);
        self.enables()
    }

    /// Powers RXVGA2 the way libusdr's RX power-up does (`lms6002d_rxvga2_enable(true)`).
    pub(crate) fn enable_rx_vga2(&self) -> Script {
        Self::one(self.write(rx_vga2::Control::RX_POWER_UP))
    }

    /// Powers RXVGA2 down the way libusdr's RX power-down does
    /// (`lms6002d_rxvga2_enable(false)`).
    pub(crate) fn disable_rx_vga2(&self) -> Script {
        Self::one(self.write(rx_vga2::Control::RX_POWER_DOWN))
    }

    /// Sets the active LNA's gain mode (`lms6002d_set_rxlna_gain`).
    pub(crate) fn set_lna_gain(&mut self, gain: LnaGain) -> Script {
        self.lna_control.set_gain(gain);
        Self::one(self.write(self.lna_control))
    }

    /// Sets RXVGA1's gain to `db`, as [`TiaFeedback::for_gain_db`] converts it
    /// (`_lms6002d_rxvga1_db_to_int`, then `lms6002d_set_rxvga1_gain`).
    pub(crate) fn set_rx_vga1_gain(&self, db: i32) -> Script {
        Self::one(self.write(TiaFeedback::for_gain_db(db)))
    }

    /// Sets RXVGA2's two stages in 3 dB steps, each held to 10 (30 dB)
    /// (`lms6002d_set_rxvga2ab_gain`).
    pub(crate) fn set_rx_vga2_gain(&self, a: u8, b: u8) -> Script {
        /// The highest step libusdr writes.
        const MAX_STEP: u8 = 10;
        let gain = rx_vga2::Gain::new()
            .with_a(u4::new(a.min(MAX_STEP)))
            .with_b(u4::new(b.min(MAX_STEP)));
        Self::one(self.write(gain))
    }

    /// Sets the RX channel filter for `bandwidth_hz` from libusdr's table, bypassing it
    /// above 47 MHz (`lms6002d_set_bandwidth` for RX).
    pub(crate) fn set_rx_bandwidth(&self, bandwidth_hz: u32) -> Script {
        self.bandwidth(Lpf::Rx, bandwidth_hz)
    }

    /// Sets the TX filter as [`Self::set_rx_bandwidth`] sets the RX one
    /// (`lms6002d_set_bandwidth` for TX).
    pub(crate) fn set_tx_bandwidth(&self, bandwidth_hz: u32) -> Script {
        self.bandwidth(Lpf::Tx, bandwidth_hz)
    }

    /// Writes `lpf`'s bandwidth, bypass and control for `bandwidth_hz`, in that order.
    fn bandwidth(&self, lpf: Lpf, bandwidth_hz: u32) -> Script {
        let (bandwidth, bypass, control) = lpf::for_bandwidth(bandwidth_hz);
        Script::new()
            .fpga(NAME, self.write_at(lpf, bandwidth))
            .fpga(NAME, self.write_at(lpf, bypass))
            .fpga(NAME, self.write_at(lpf, control))
    }

    /// Sets TXVGA1's gain to `db`, held to -35..=-4 dB (`lms6002d_set_txvga1_gain`, after
    /// libusdr's clamp).
    pub(crate) fn set_tx_vga1_gain(&mut self, db: i32) -> Script {
        self.set_tx_vga1_code(Vga1Gain::for_db(db).code())
    }

    /// Sets TXVGA1's gain code, -35 dB at 0 to -4 dB at 31, and keeps it, even if the write
    /// fails, as libusdr does (`lms6002d_set_txvga1_gain`).
    pub(crate) fn set_tx_vga1_code(&mut self, code: u5) -> Script {
        self.tx_vga1 = Vga1Gain::new().with_code(code);
        Self::one(self.write(self.tx_vga1))
    }

    /// The TXVGA1 gain code last written.
    pub(crate) fn tx_vga1_code(&self) -> u5 {
        self.tx_vga1.code()
    }

    /// Shifts TXVGA1's I and Q DC by offset-binary codes, 0x80 none
    /// (`lms6002d_set_txvga1_dc`).
    pub(crate) fn set_tx_vga1_dc(&self, i: u8, q: u8) -> Script {
        Script::new()
            .fpga(
                NAME,
                self.write_at(IqChannel::I, Vga1DcShift::new().with_code(i)),
            )
            .fpga(
                NAME,
                self.write_at(IqChannel::Q, Vga1DcShift::new().with_code(q)),
            )
    }

    /// The RX input selected (`LNASEL`), as last written.
    pub(crate) fn active_lna(&self) -> Lna {
        self.lna_control.active()
    }

    /// Stops the RX synthesizer's modulator clock and turns its loop off
    /// (`lms6002d_disable_pll` for RX).
    pub(crate) fn disable_rx_pll(&mut self) -> Script {
        self.disable_pll(Pll::Rx)
    }

    /// Stops the TX synthesizer's modulator clock and turns its loop off
    /// (`lms6002d_disable_pll` for TX).
    pub(crate) fn disable_tx_pll(&mut self) -> Script {
        self.disable_pll(Pll::Tx)
    }

    /// Writes the clock enables without `pll`'s modulator clock, then a zero loop setup.
    fn disable_pll(&mut self, pll: Pll) -> Script {
        self.set_pll_modulator_clock(pll, false);
        Script::new()
            .fpga(NAME, self.write(self.clock_enables))
            .fpga(NAME, self.write_at(pll, PllConfig::new()))
    }

    /// Records `pll`'s modulator clock on or off, unwritten.
    pub(super) fn set_pll_modulator_clock(&mut self, pll: Pll, on: bool) {
        match pll {
            Pll::Tx => self.clock_enables.set_tx_pll_modulator_clock(on),
            Pll::Rx => self.clock_enables.set_rx_pll_modulator_clock(on),
        }
    }

    /// Sets TXVGA2's gain to `db`, held to 0..=25 dB, with the detector multiplexer at 0
    /// (`lms6002d_set_txvga2_gain`, after libusdr's clamp).
    pub(crate) fn set_tx_vga2_gain(&self, db: i32) -> Script {
        Self::one(self.write(Vga2Gain::for_db(db)))
    }

    /// Makes `lna` the active RX input and powers the RX synthesizer's LO buffer for it.
    pub(crate) fn select_lna(&mut self, lna: Lna) -> Script {
        self.rx_vco.set_lo_buffer(lna);
        self.lna_control.set_active(lna);
        Script::new()
            .fpga(NAME, self.write_at(Pll::Rx, self.rx_vco))
            .fpga(NAME, self.write(self.lna_control))
    }

    /// Writes the mixer's I and Q DC-offset trims, with the mixer fed from the LNA and the
    /// LNA's internal load on (`lms6002d_set_rxfedc`). The I trim shares its register
    /// with the mixer input, which the stream's DC calibration rewrites with no trim, as
    /// libusdr's does.
    pub(crate) fn set_rx_dc_trim(&self, i: u7, q: u7) -> Script {
        let input = MixerInput::new().with_from_lna(true).with_dc_offset_i(i);
        let load = InternalLoad::new()
            .with_internal_load(true)
            .with_dc_offset_q(q);
        Script::new()
            .fpga(NAME, self.write(input))
            .fpga(NAME, self.write(load))
    }

    /// Writes the mixer's I and Q IP2 trims, with the LNA's external load off
    /// (`lms6002d_set_rxfe_ip2corr`).
    pub(crate) fn set_rx_ip2_trim(&self, i: u7, q: u7) -> Script {
        Script::new()
            .fpga(NAME, self.write(ExternalLoad::new().with_ip2_trim_i(i)))
            .fpga(NAME, self.write(Ip2TrimQ::new().with_ip2_trim_q(q)))
    }

    /// Loops the TX mixer's output into the active LNA's path, with the LNA powered down
    /// (`lms6002d_rf_loopback_en`). As in libusdr, the RX front end is left taking its
    /// controls from the test-mode registers, both PAs go off, and the loopback stays on
    /// the LNA path active now even if the LNA changes later.
    pub(crate) fn enable_rf_loopback(&self) -> Script {
        let test_mode = rx_fe::Control::new()
            .with_test_mode_controls(true)
            .with_enabled(true);
        Script::new()
            .fpga(NAME, self.write(PowerDown::new().with_lna_off(true)))
            .fpga(NAME, self.write(test_mode))
            .fpga(NAME, self.write(PaSelect::new()))
            .fpga(
                NAME,
                self.write(ReferencePower::SELF_BIASED.with_rf_loopback_on(true)),
            )
            .fpga(
                NAME,
                self.write(Loopback::new().with_rf(self.lna_control.active().into())),
            )
    }

    /// Undoes [`Self::enable_rf_loopback`] (`lms6002d_rf_loopback_dis`): the loopback
    /// switch off twice, as libusdr writes it, the cached PA selection back and the LNA
    /// powered. The RX front end keeps taking its controls from the test-mode registers.
    pub(crate) fn disable_rf_loopback(&self) -> Script {
        Script::new()
            .fpga(NAME, self.write(ReferencePower::SELF_BIASED))
            .fpga(NAME, self.write(Loopback::new()))
            .fpga(NAME, self.write(self.pa_select))
            .fpga(NAME, self.write(ReferencePower::SELF_BIASED))
            .fpga(NAME, self.write(PowerDown::new()))
    }

    /// Makes `amplifier` the TX output and clears `ENAUX`, as libusdr does for PA1 and PA2;
    /// see [`PaSelect`] on that bit's disputed polarity.
    pub(crate) fn select_pa(&mut self, amplifier: PowerAmp) -> Script {
        self.pa_select.set_amplifier(amplifier);
        self.pa_select.set_aux_pa_off(false);
        Self::one(self.write(self.pa_select))
    }

    /// Writes [`ClockEnables`] then [`EnableConfig`] from the cache.
    pub(super) fn enables(&self) -> Script {
        Script::new()
            .fpga(NAME, self.write(self.clock_enables))
            .fpga(NAME, self.write(self.enable_config))
    }

    /// A script of one access.
    fn one(access: FpgaAccess) -> Script {
        Script::new().fpga(NAME, access)
    }

    /// The exchange writing a typed register.
    pub(super) fn write<R: Register<Map: BlockReg>>(&self, value: R) -> FpgaAccess {
        spi::write(self.target, R::ADDR, value.into())
    }

    /// The exchange writing one copy of a repeated register.
    pub(super) fn write_at<R: IndexedRegister<Map: BlockReg>>(
        &self,
        index: R::Index,
        value: R,
    ) -> FpgaAccess {
        spi::write(self.target, R::addr(index), value.into())
    }

    /// The exchange writing a register as a byte.
    pub(super) fn write_raw(&self, reg: impl BlockReg, value: u8) -> FpgaAccess {
        spi::write(self.target, reg, value)
    }

    /// The exchange reading one copy of a repeated register.
    pub(super) fn read_at<R: IndexedRegister<Map: BlockReg>>(&self, index: R::Index) -> FpgaAccess {
        spi::read(self.target, R::addr(index))
    }

    /// The exchange reading a register as a byte.
    pub(super) fn read_raw(&self, reg: impl BlockReg) -> FpgaAccess {
        spi::read(self.target, reg)
    }
}

/// `lms6002d_create`'s accesses: the chip ID read, the power-up configuration, then the
/// ID check, which libusdr makes only after configuring: an ID of 0xFF (no chip
/// answering) fails with [`SequenceFailure::ChipMissing`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Create(Script);

impl Sequence for Create {
    fn next(&self) -> Option<SequenceAction> {
        self.0.next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        // An outcome after the last access is a duplicate; the check has been made.
        if self.0.next().is_none() {
            return Ok(());
        }
        self.0.complete(result)?;
        let absent = self.0.read().map(spi::value) == Some(0xff);
        if self.0.succeeded() && absent {
            return Err(SequenceFailure::ChipMissing { chip: NAME });
        }
        Ok(())
    }
}
