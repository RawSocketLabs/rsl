//! The LMK05318 driver: bring-up, output plans, lock waits, synchronization and output
//! power-down, as libusdr's `hw/lmk05318/lmk05318.c` runs them for the `SimpleSync` and
//! XMASS front ends.

use std::time::Duration;

use bnb::{u2, u3};
use rsl_deps::tracing::{debug, info};

use super::apll::{
    Apll, Apll1DenominatorMode, Apll2DenominatorMode, BawLock, Dither, Pll1Calibration,
    Pll1Control, Pll1Mode, Pll1Modulator, Pll2Control, Pll2Modulator, Pll2PostDividers, PllClocks,
    VcoStatus,
};
use super::control::{DeviceControl, DpllControl, DpllFlags, InterruptControl, LossFlags};
use super::map::RegisterMap;
use super::output::{
    DividedOutput, FormatCode, LockMute, MuteGroup, MuteLevel, MuteLevels, MuxCode, OutputControl,
    OutputControlReg, OutputDivider, OutputFormatOnly, OutputMute, OutputSync, PostDividerSync,
    without_format,
};
use super::plan::{
    Apll1Plan, Apll2, Apll2Setting, OutputPlan, OutputRequest, PlanError, Xo, XoPath, divider_fits,
    out7_stages,
};
use super::reg::{IndexedLmk05318Register, Lmk05318Register, Reg};
use super::xo::{XoControl, XoDivider, XoInputType, baw_detection_for};
use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// An LMK05318 on the I2C bus, with the state libusdr keeps for it (`lmk05318_state_t`
/// without the DPLL, and its register staging map).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lmk05318 {
    /// Where the chip answers.
    dev: I2cAddr,
    /// APLL2's dividers and last plan.
    apll2: Apll2,
    /// Registers staged for the next write.
    map: RegisterMap,
}

/// The latched loss flags and BAW lock, as `lmk05318_check_lock` reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LockStatus {
    /// XO and APLL losses.
    loss: LossFlags,
    /// DPLL events.
    dpll: DpllFlags,
    /// BAW lock.
    baw: BawLock,
}

impl Lmk05318 {
    /// The chip's name in errors.
    pub(crate) const NAME: &'static str = "LMK05318";

    /// The identification bytes from address 0, in wire order.
    pub(crate) const ID: [u8; 4] = [0x10, 0x0b, 0x35, 0x42];

    /// How long a lock wait sleeps between polls (`usleep(100)`).
    const POLL_INTERVAL: Duration = Duration::from_micros(100);

    /// The pause after each write of a toggled bit (`lmk05318_trigger_reg_with_sleep`), and
    /// after the bring-up's register load.
    const SETTLE: Duration = Duration::from_millis(10);

    /// The chip at `dev`, as `lmk05318_create` sets its state up before talking to it:
    /// APLL2 cascaded from APLL1, nothing planned or staged.
    pub(crate) fn at(dev: I2cAddr) -> Self {
        Self {
            dev,
            apll2: Apll2::cascaded(),
            map: RegisterMap::new(),
        }
    }

    /// Brings the chip up (`lmk05318_create` without a DPLL): checks its identity, plans
    /// `xo`, APLL1 and `outputs`, writes the plan with the common settings, waits for it to
    /// settle and soft-resets.
    ///
    /// The state starts over first and keeps whatever the steps reached, so a caller that
    /// carries on after a failure (XMASS does) sees libusdr's state.
    ///
    /// # Errors
    ///
    /// [`Error::ChipId`] when another chip answers (libusdr's `-ENODEV`), before any write;
    /// [`Error::ClockPlan`] when no plan exists; any bus failure.
    pub(crate) fn create(
        &mut self,
        bus: &mut dyn Bus,
        xo: Xo,
        outputs: &[OutputRequest],
    ) -> Result<(), Error> {
        *self = Self::at(self.dev);
        let id = self.read_id(bus)?;
        debug!(?id, "LMK05318 identification");
        if id != Self::ID {
            return Err(Error::ChipId {
                chip: Self::NAME,
                expected: u32::from_be_bytes(Self::ID),
                found: u32::from_be_bytes(id),
            });
        }
        let path = self.stage_xo(xo)?;
        let apll1 = xo.apll1(path).map_err(plan_error)?;
        self.stage_apll1(apll1);
        self.plan(outputs)?;
        self.stage_common();
        self.write_staged(bus)?;
        bus.sleep(Self::SETTLE);
        self.soft_reset(bus)?;
        info!("LMK05318 initialized");
        Ok(())
    }

    /// Plans `outputs` and stages their registers and APLL2's (`lmk05318_solver`); the
    /// next [`Lmk05318::write_staged`] writes them. Outputs not named keep their settings.
    ///
    /// # Errors
    ///
    /// [`Error::ClockPlan`] when no plan exists; nothing is staged then, but APLL2's state
    /// may have moved on, as libusdr's does.
    pub(crate) fn plan(&mut self, outputs: &[OutputRequest]) -> Result<(), Error> {
        let plan = self.apll2.plan(outputs).map_err(plan_error)?;
        self.stage_apll2(plan.apll2);
        for output in &plan.outputs {
            self.stage_output(output)?;
        }
        Ok(())
    }

    /// Writes the staged registers in address order, then empties the map
    /// (`lmk05318_reg_wr_from_map`). A failed write leaves the map as it was.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub(crate) fn write_staged(&mut self, bus: &mut dyn Bus) -> Result<(), Error> {
        for (reg, value) in self.map.staged() {
            self.write(bus, reg, value)?;
        }
        self.map.clear();
        Ok(())
    }

    /// Calibrates APLL1 and waits up to `timeout` for it to lock
    /// (`lmk05318_wait_apll1_lock` without a DPLL): its charge pump in range and the BAW
    /// locked. A zero timeout waits for ever, as libusdr's does.
    ///
    /// libusdr budgets the timeout in wall-clock time; this counts the 100 µs sleeps
    /// between polls, so on real buses it polls at least as long.
    ///
    /// # Errors
    ///
    /// [`Error::ClockUnlocked`] on timeout; any bus failure.
    pub(crate) fn wait_apll1_lock(
        &self,
        bus: &mut dyn Bus,
        timeout: Duration,
    ) -> Result<(), Error> {
        self.toggle(
            bus,
            Pll1Control::new().with_powered_down(true),
            Pll1Control::new(),
        )?;
        self.reset_loss_flags(bus)?;
        self.poll_lock(bus, Apll::Apll1, timeout, |status| status.baw.locked())?
            .then_some(())
            .ok_or(Error::ClockUnlocked("LMK05318 APLL1"))
    }

    /// Calibrates APLL2 and waits up to `timeout` for it to lock
    /// (`lmk05318_wait_apll2_lock`): its charge pump in range and no loss of lock latched.
    /// Nothing happens until a plan has used APLL2. Timing as for
    /// [`Lmk05318::wait_apll1_lock`].
    ///
    /// # Errors
    ///
    /// [`Error::ClockUnlocked`] on timeout; any bus failure.
    pub(crate) fn wait_apll2_lock(
        &self,
        bus: &mut dyn Bus,
        timeout: Duration,
    ) -> Result<(), Error> {
        if self.apll2.vco_hz == 0 {
            debug!("LMK05318 APLL2 unused; lock not checked");
            return Ok(());
        }
        let (primary, secondary) = (self.apll2.primary_divider, self.apll2.secondary_divider);
        self.toggle(
            bus,
            Pll2Control::with_dividers(primary, secondary, true),
            Pll2Control::with_dividers(primary, secondary, false),
        )?;
        self.reset_loss_flags(bus)?;
        self.poll_lock(bus, Apll::Apll2, timeout, |status| {
            !status.loss.apll2_unlocked()
        })?
        .then_some(())
        .ok_or(Error::ClockUnlocked("LMK05318 APLL2"))
    }

    /// Synchronizes the outputs: `SYNC_SW` set, then cleared (`lmk05318_sync`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub(crate) fn sync(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let control: DeviceControl = self.read_typed(bus)?;
        self.toggle(bus, control.with_sync(true), control.with_sync(false))
    }

    /// Turns output `port` off, keeping its source (`lmk05318_disable_port`).
    ///
    /// # Errors
    ///
    /// [`Error::ClockPlan`] for an output past 7, before any bus access; any bus failure.
    pub(crate) fn disable_output(&self, bus: &mut dyn Bus, port: u8) -> Result<(), Error> {
        let reg = OutputControlReg::of(port)
            .ok_or(plan_error(PlanError::BadRequest))?
            .addr();
        let control = self.read(bus, reg.into())?;
        self.write(bus, reg.into(), without_format(control))
    }

    /// Reads and logs the lock status, as libusdr does after each bring-up step
    /// (`lmk05318_check_lock(.., false)`, whose result its callers ignore). A failed read
    /// ends the reads and is only logged.
    pub(crate) fn log_lock(&self, bus: &mut dyn Bus) {
        match self.check_lock(bus) {
            Ok(status) => info!(?status, "LMK05318 lock status"),
            Err(error) => debug!(%error, "LMK05318 lock status unreadable"),
        }
    }

    /// Reads the four identification bytes from address 0, in wire order
    /// (`lmk05318_reg_get_u32(.., 0, ..)`).
    fn read_id(&self, bus: &mut dyn Bus) -> Result<[u8; 4], Error> {
        let reg = u16::from(Reg::Id);
        let mut raw = [0; 4];
        bus.i2c(self.dev, &reg.to_be_bytes(), &mut raw)
            .reading(Self::NAME, reg)?;
        Ok(raw)
    }

    /// Stages the XO path and its BAW detection constants (`lmk05318_set_xo_fref`).
    fn stage_xo(&mut self, xo: Xo) -> Result<XoPath, Error> {
        let path = xo.path().map_err(plan_error)?;
        self.map.stage(XoControl::for_path(xo, path));
        self.map.stage(XoInputType::for_input(xo.input));
        self.map.stage(XoDivider::for_path(path));
        // libusdr stages the XO path before finding the XO unsupported.
        let table = baw_detection_for(xo.hz).ok_or(plan_error(PlanError::UnsupportedXo))?;
        for &(reg, value) in table {
            self.map.stage_raw(reg, value);
        }
        Ok(path)
    }

    /// Stages APLL1 running free with a programmed 24-bit fraction
    /// (`lmk05318_tune_apll1` without a DPLL).
    fn stage_apll1(&mut self, plan: Apll1Plan) {
        self.map.stage(
            Pll1Modulator::new()
                .with_dither(Dither::Weak)
                .with_order(plan.order.into()),
        );
        self.map.stage(Pll1Mode::new());
        let [_, _, n_high, n_low] = plan.n.to_be_bytes();
        self.map.stage_raw(Reg::Pll1IntegerHigh, n_high);
        self.map.stage_raw(Reg::Pll1IntegerLow, n_low);
        let [.., den_high, den_middle, den_low] = plan.den.to_be_bytes();
        self.map.stage_raw(Reg::Pll1DenominatorLow, den_low);
        self.map.stage_raw(Reg::Pll1DenominatorMiddle, den_middle);
        self.map.stage_raw(Reg::Pll1DenominatorHigh, den_high);
        let [.., num_high, num_middle, num_low] = plan.num.to_be_bytes();
        self.map.stage_raw(Reg::Pll1NumeratorLow, num_low);
        self.map.stage_raw(Reg::Pll1NumeratorMiddle, num_middle);
        self.map.stage_raw(Reg::Pll1NumeratorHigh, num_high);
        self.map.stage(Pll1Control::new());
    }

    /// Stages APLL2's settings (`lmk05318_tune_apll2`).
    fn stage_apll2(&mut self, setting: Apll2Setting) {
        let (primary, secondary) = (self.apll2.primary_divider, self.apll2.secondary_divider);
        let Apll2Setting::On {
            n,
            num,
            den,
            pd1,
            pd2,
            order,
        } = setting
        else {
            self.map
                .stage(Pll2Control::with_dividers(primary, secondary, true));
            return;
        };
        self.map.stage(
            Pll2Modulator::new()
                .with_dither(Dither::Weak)
                .with_order(order.into()),
        );
        self.map.stage(Pll2PostDividers::with_dividers(pd1, pd2));
        let [_, _, n_high, n_low] = n.to_be_bytes();
        self.map.stage_raw(Reg::Pll2IntegerHigh, n_high);
        self.map.stage_raw(Reg::Pll2IntegerLow, n_low);
        let [_, num_high, num_middle, num_low] = num.to_be_bytes();
        self.map.stage_raw(Reg::Pll2NumeratorLow, num_low);
        self.map.stage_raw(Reg::Pll2NumeratorMiddle, num_middle);
        self.map.stage_raw(Reg::Pll2NumeratorHigh, num_high);
        let [_, den_high, den_middle, den_low] = den.to_be_bytes();
        self.map.stage_raw(Reg::Pll2DenominatorHigh, den_high);
        self.map.stage_raw(Reg::Pll2DenominatorMiddle, den_middle);
        self.map.stage_raw(Reg::Pll2DenominatorLow, den_low);
        self.map
            .stage(Pll2Control::with_dividers(primary, secondary, false));
    }

    /// Stages one output's source, format and divider (`lmk05318_set_out_mux`, then
    /// `lmk05318_set_out_div`); an output turned off gets source 0 and no format.
    fn stage_output(&mut self, output: &OutputPlan) -> Result<(), Error> {
        let port = output.request.port;
        let (mux, format) = output
            .route
            .map_or((MuxCode::Apll1, FormatCode::Disabled), |route| {
                (route.mux.into(), output.request.format.into())
            });
        match OutputControlReg::of(port).ok_or(plan_error(PlanError::BadRequest))? {
            OutputControlReg::Muxed(index) => self.map.stage_at(
                index,
                OutputControl::new().with_mux(mux).with_format(format),
            ),
            OutputControlReg::Paired(index) => self
                .map
                .stage_at(index, OutputFormatOnly::new().with_format(format)),
        }
        let Some(route) = output.route else {
            return Ok(());
        };
        if !divider_fits(port, route.divider) {
            return Err(plan_error(PlanError::BadDivider));
        }
        let divided = DividedOutput::of(port).ok_or(plan_error(PlanError::BadRequest))?;
        if divided != DividedOutput::Out7 {
            self.map
                .stage_at(divided, OutputDivider::ratio(route.divider));
            return Ok(());
        }
        let (first, second) = out7_stages(route.divider);
        self.map
            .stage_at(divided, OutputDivider::ratio(u64::from(first)));
        let [.., high, middle, low] = second.wrapping_sub(1).to_be_bytes();
        self.map.stage_raw(Reg::Output7Stage2High, high);
        self.map.stage_raw(Reg::Output7Stage2Middle, middle);
        self.map.stage_raw(Reg::Output7Stage2Low, low);
        Ok(())
    }

    /// Stages the settings every bring-up shares, without a DPLL
    /// (`lmk05318_set_common_registers`): APLL1 running free with programmed denominators,
    /// BAW lock detection, cascaded APLLs, every output synchronized and unmuted, muted
    /// outputs driven low, and the loss flags and interrupt cleared.
    fn stage_common(&mut self) {
        self.map.stage(
            DeviceControl::new()
                .with_sync_after_apll_lock(true)
                .with_mute_during_sync(true)
                .with_cascaded_startup(true)
                .with_autostart(true),
        );
        self.map
            .stage(Apll1DenominatorMode::new().with_programmed(true));
        self.map
            .stage(Apll2DenominatorMode::new().with_programmed(true));
        self.map.stage(DpllControl::new());
        self.map.stage(
            Pll1Calibration::new()
                .with_baw_lock_detect(true)
                .with_vco_wait(u2::new(1)),
        );
        self.map.stage(BawLock::new().with_locked(true));

        self.map
            .stage(PllClocks::new().with_vco1_outputs(u3::new(0b111)));
        self.map.stage(
            PostDividerSync::new()
                .with_apll2_post2(true)
                .with_apll2_post1(true)
                .with_apll1(true),
        );
        self.map.stage(
            OutputSync::new()
                .with_output7(true)
                .with_output6(true)
                .with_output5(true)
                .with_output4(true)
                .with_outputs23(true)
                .with_outputs01(true),
        );
        self.map.stage(
            LockMute::new()
                .with_apll2(true)
                .with_dpll_phase(true)
                .with_dpll_frequency(true)
                .with_apll1(true),
        );
        self.map.stage(OutputMute::new());
        let low = MuteLevels::all(MuteLevel::DifferentialLow);
        self.map.stage_at(MuteGroup::Low, low);
        self.map.stage_at(MuteGroup::High, low);
        self.map.stage(LossFlags::new());
        self.map.stage(DpllFlags::new());
        self.map.stage(InterruptControl::new());
    }

    /// Soft-resets the chip: `RESET_SW` set, then cleared (`lmk05318_softreset`).
    fn soft_reset(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        let control: DeviceControl = self.read_typed(bus)?;
        self.toggle(bus, control.with_reset(true), control.with_reset(false))
    }

    /// Clears the latched loss flags (`lmk05318_reset_los_flags`).
    fn reset_loss_flags(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.write_typed(bus, LossFlags::new())?;
        self.write_typed(bus, DpllFlags::new())
    }

    /// Reads the latched loss flags and BAW lock (`lmk05318_check_lock`), stopping at the
    /// first failed read.
    fn check_lock(&self, bus: &mut dyn Bus) -> Result<LockStatus, Error> {
        Ok(LockStatus {
            loss: self.read_typed(bus)?,
            dpll: self.read_typed(bus)?,
            baw: self.read_typed(bus)?,
        })
    }

    /// Polls `apll`'s VCO status and the lock status until the VCO is in range and
    /// `locked` holds, sleeping between polls, for at most `timeout` of sleeps; `false`
    /// when it times out.
    fn poll_lock(
        &self,
        bus: &mut dyn Bus,
        apll: Apll,
        timeout: Duration,
        locked: impl Fn(&LockStatus) -> bool,
    ) -> Result<bool, Error> {
        let mut waited = Duration::ZERO;
        while timeout.is_zero() || waited < timeout {
            let vco: VcoStatus = self.read_at(bus, apll)?;
            let status = self.check_lock(bus)?;
            if vco.in_range() && locked(&status) {
                debug!(?apll, ?waited, "LMK05318 locked");
                return Ok(true);
            }
            bus.sleep(Self::POLL_INTERVAL);
            waited += Self::POLL_INTERVAL;
        }
        Ok(false)
    }

    /// Writes `set`, then `clear`, pausing after each (`lmk05318_trigger_reg_with_sleep`).
    fn toggle<R: Lmk05318Register>(
        &self,
        bus: &mut dyn Bus,
        set: R,
        clear: R,
    ) -> Result<(), Error> {
        self.write_typed(bus, set)?;
        bus.sleep(Self::SETTLE);
        self.write_typed(bus, clear)?;
        bus.sleep(Self::SETTLE);
        Ok(())
    }

    /// Reads a typed register.
    fn read_typed<R: Lmk05318Register>(&self, bus: &mut dyn Bus) -> Result<R, Error> {
        self.read(bus, R::ADDR.into()).map(R::from)
    }

    /// Reads one copy of a repeated register.
    fn read_at<R: IndexedLmk05318Register>(
        &self,
        bus: &mut dyn Bus,
        index: R::Index,
    ) -> Result<R, Error> {
        self.read(bus, R::addr(index).into()).map(R::from)
    }

    /// Writes a typed register.
    fn write_typed<R: Lmk05318Register>(&self, bus: &mut dyn Bus, value: R) -> Result<(), Error> {
        self.write(bus, R::ADDR.into(), value.into())
    }

    /// Reads one register (`lmk05318_reg_rd`).
    fn read(&self, bus: &mut dyn Bus, reg: u16) -> Result<u8, Error> {
        let mut value = [0];
        bus.i2c(self.dev, &reg.to_be_bytes(), &mut value)
            .reading(Self::NAME, reg)?;
        Ok(value[0])
    }

    /// Writes one register (`lmk05318_reg_wr`).
    fn write(&self, bus: &mut dyn Bus, reg: u16, value: u8) -> Result<(), Error> {
        let [high, low] = reg.to_be_bytes();
        bus.i2c(self.dev, &[high, low, value], &mut [])
            .writing(Self::NAME, reg)
    }
}

/// A plan failure as the driver's error.
fn plan_error(error: PlanError) -> Error {
    Error::ClockPlan(error.reason())
}
