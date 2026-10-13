//! A plan's registers: what libusdr stages for the XO (`lmk05318_set_xo_fref`), APLL1
//! (`lmk05318_tune_apll1` without the DPLL), APLL2 and each output (the end of
//! `lmk05318_solver`: `lmk05318_tune_apll2`, `lmk05318_set_out_mux`,
//! `lmk05318_set_out_div`) and the common settings (`lmk05318_set_common_registers`
//! without the DPLL), into a [`RegisterMap`].
//!
//! Register values only, no bus, so the oracle's parity tests compile this module (with
//! the plan, map and value types) directly and compare the map with libusdr's.

use bnb::{u2, u3};

use super::apll::{
    Apll1DenominatorMode, Apll2DenominatorMode, BawLock, Dither, Pll1Calibration, Pll1Control,
    Pll1Mode, Pll1Modulator, Pll2Control, Pll2Modulator, Pll2PostDividers, PllClocks,
};
use super::control::{DeviceControl, DpllControl, DpllFlags, InterruptControl, LossFlags};
use super::map::RegisterMap;
use super::output::{
    DividedOutput, FormatCode, LockMute, MuteGroup, MuteLevel, MuteLevels, MuxCode, OutputControl,
    OutputControlReg, OutputDivider, OutputFormatOnly, OutputMute, OutputSync, PostDividerSync,
};
use super::plan::{
    Apll1Plan, Apll2, Apll2Setting, ClockPlanError, OutputPlan, Plan, Xo, XoPath, divider_fits,
    out7_stages,
};
use super::reg::Reg;
use super::xo::{XoControl, XoDivider, XoInputType, baw_detection_for};

/// Stages the XO path and its BAW detection constants (`lmk05318_set_xo_fref`). An XO
/// without BAW settings fails after its path is staged, as in libusdr.
pub(crate) fn stage_xo(map: &mut RegisterMap, xo: Xo) -> Result<XoPath, ClockPlanError> {
    let path = xo.path()?;
    map.stage(XoControl::for_path(xo, path));
    map.stage(XoInputType::for_input(xo.input));
    map.stage(XoDivider::for_path(path));
    let table = baw_detection_for(xo.hz).ok_or(ClockPlanError::UnsupportedXo)?;
    for &(reg, value) in table {
        map.stage_raw(reg, value);
    }
    Ok(path)
}

/// Stages APLL1 running free with a programmed 24-bit fraction (`lmk05318_tune_apll1`
/// without a DPLL).
pub(crate) fn stage_apll1(map: &mut RegisterMap, plan: Apll1Plan) {
    map.stage(
        Pll1Modulator::new()
            .with_dither(Dither::Weak)
            .with_order(plan.order.into()),
    );
    map.stage(Pll1Mode::new());
    let [_, _, n_high, n_low] = plan.n.to_be_bytes();
    map.stage_raw(Reg::Pll1IntegerHigh, n_high);
    map.stage_raw(Reg::Pll1IntegerLow, n_low);
    let [.., den_high, den_middle, den_low] = plan.den.to_be_bytes();
    map.stage_raw(Reg::Pll1DenominatorLow, den_low);
    map.stage_raw(Reg::Pll1DenominatorMiddle, den_middle);
    map.stage_raw(Reg::Pll1DenominatorHigh, den_high);
    let [.., num_high, num_middle, num_low] = plan.num.to_be_bytes();
    map.stage_raw(Reg::Pll1NumeratorLow, num_low);
    map.stage_raw(Reg::Pll1NumeratorMiddle, num_middle);
    map.stage_raw(Reg::Pll1NumeratorHigh, num_high);
    map.stage(Pll1Control::new());
}

/// Stages a plan (the end of `lmk05318_solver`): APLL2 on `apll2`'s reference dividers,
/// then each output's source, format and divider in output order.
///
/// # Errors
///
/// [`ClockPlanError::BadDivider`] for a divider past its output's range, after its source
/// is staged, as in libusdr; the solver never plans one.
pub(crate) fn stage_plan(
    map: &mut RegisterMap,
    apll2: &Apll2,
    plan: &Plan,
) -> Result<(), ClockPlanError> {
    stage_apll2(map, apll2, plan.apll2);
    plan.outputs
        .iter()
        .try_for_each(|output| stage_output(map, output))
}

/// Stages APLL2's settings (`lmk05318_tune_apll2`).
fn stage_apll2(map: &mut RegisterMap, apll2: &Apll2, setting: Apll2Setting) {
    let (primary, secondary) = (apll2.primary_divider, apll2.secondary_divider);
    let Apll2Setting::On {
        n,
        num,
        den,
        pd1,
        pd2,
        order,
    } = setting
    else {
        map.stage(Pll2Control::with_dividers(primary, secondary, true));
        return;
    };
    map.stage(
        Pll2Modulator::new()
            .with_dither(Dither::Weak)
            .with_order(order.into()),
    );
    map.stage(Pll2PostDividers::with_dividers(pd1, pd2));
    let [_, _, n_high, n_low] = n.to_be_bytes();
    map.stage_raw(Reg::Pll2IntegerHigh, n_high);
    map.stage_raw(Reg::Pll2IntegerLow, n_low);
    let [_, num_high, num_middle, num_low] = num.to_be_bytes();
    map.stage_raw(Reg::Pll2NumeratorLow, num_low);
    map.stage_raw(Reg::Pll2NumeratorMiddle, num_middle);
    map.stage_raw(Reg::Pll2NumeratorHigh, num_high);
    let [_, den_high, den_middle, den_low] = den.to_be_bytes();
    map.stage_raw(Reg::Pll2DenominatorHigh, den_high);
    map.stage_raw(Reg::Pll2DenominatorMiddle, den_middle);
    map.stage_raw(Reg::Pll2DenominatorLow, den_low);
    map.stage(Pll2Control::with_dividers(primary, secondary, false));
}

/// Stages one output's source, format and divider (`lmk05318_set_out_mux`, then
/// `lmk05318_set_out_div`); an output turned off gets source 0 and no format.
fn stage_output(map: &mut RegisterMap, output: &OutputPlan) -> Result<(), ClockPlanError> {
    let port = output.request.port;
    let (mux, format) = output
        .route
        .map_or((MuxCode::Apll1, FormatCode::Disabled), |route| {
            (route.mux.into(), output.request.format.into())
        });
    match OutputControlReg::of(port).ok_or(ClockPlanError::BadRequest)? {
        OutputControlReg::Muxed(index) => {
            map.stage_at(
                index,
                OutputControl::new().with_mux(mux).with_format(format),
            );
        }
        OutputControlReg::Paired(index) => {
            map.stage_at(index, OutputFormatOnly::new().with_format(format));
        }
    }
    let Some(route) = output.route else {
        return Ok(());
    };
    if !divider_fits(port, route.divider) {
        return Err(ClockPlanError::BadDivider);
    }
    let divided = DividedOutput::of(port).ok_or(ClockPlanError::BadRequest)?;
    if divided != DividedOutput::Out7 {
        map.stage_at(divided, OutputDivider::ratio(route.divider));
        return Ok(());
    }
    let (first, second) = out7_stages(route.divider);
    map.stage_at(divided, OutputDivider::ratio(u64::from(first)));
    let [.., high, middle, low] = second.wrapping_sub(1).to_be_bytes();
    map.stage_raw(Reg::Output7Stage2High, high);
    map.stage_raw(Reg::Output7Stage2Middle, middle);
    map.stage_raw(Reg::Output7Stage2Low, low);
    Ok(())
}

/// Stages the settings every bring-up shares, without a DPLL
/// (`lmk05318_set_common_registers`): APLL1 running free with programmed denominators,
/// BAW lock detection, cascaded APLLs, every output synchronized and unmuted, muted
/// outputs driven low, and the loss flags and interrupt cleared.
pub(crate) fn stage_common(map: &mut RegisterMap) {
    map.stage(
        DeviceControl::new()
            .with_sync_after_apll_lock(true)
            .with_mute_during_sync(true)
            .with_cascaded_startup(true)
            .with_autostart(true),
    );
    map.stage(Apll1DenominatorMode::new().with_programmed(true));
    map.stage(Apll2DenominatorMode::new().with_programmed(true));
    map.stage(DpllControl::new());
    map.stage(
        Pll1Calibration::new()
            .with_baw_lock_detect(true)
            .with_vco_wait(u2::new(1)),
    );
    map.stage(BawLock::new().with_locked(true));

    map.stage(PllClocks::new().with_vco1_outputs(u3::new(0b111)));
    map.stage(
        PostDividerSync::new()
            .with_apll2_post2(true)
            .with_apll2_post1(true)
            .with_apll1(true),
    );
    map.stage(
        OutputSync::new()
            .with_output7(true)
            .with_output6(true)
            .with_output5(true)
            .with_output4(true)
            .with_outputs23(true)
            .with_outputs01(true),
    );
    map.stage(
        LockMute::new()
            .with_apll2(true)
            .with_dpll_phase(true)
            .with_dpll_frequency(true)
            .with_apll1(true),
    );
    map.stage(OutputMute::new());
    let low = MuteLevels::all(MuteLevel::DifferentialLow);
    map.stage_at(MuteGroup::Low, low);
    map.stage_at(MuteGroup::High, low);
    map.stage(LossFlags::new());
    map.stage(DpllFlags::new());
    map.stage(InterruptControl::new());
}
