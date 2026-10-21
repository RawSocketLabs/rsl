//! The LMX1204's plan: libusdr's `lmx1204_solver` (with `lmx1204_solver_prevalidate` and
//! `lmx1204_get_delay_step_size`) and the register list it writes after its reset; and the
//! registers the SYSREF delay reload and windowing write from the plan.
//!
//! Each SYSREF output's delay is the request's: `lmx1204_create`'s defaults (phase QCLK0,
//! I = 7, Q = 0x78, [`Delay::CREATE`]) unless set as `lmx1204_init_sysrefout_ch_delay`
//! sets them before the solver, or changed after it ([`Plan::with_delays`]) for
//! `lmx1204_reload_sysrefout_ch_delay`.

use core::fmt;

use bnb::{u2, u3, u4, u5, u6, u7, u9, u10, u12};

use super::super::float::{abs, c_f64, c_u64, c_unsigned, ceil_log2_ratio};
use super::super::ti_sync::delay_step;
use super::reg::{
    Channels, ClockMux, DelayScale, Format, LogicClock, LogicDivider, LogicSysrefOutput, Mode,
    MuxOut, Outputs, Reg, StateMachineClock, Sysref, SysrefDelay, SysrefDivider, SysrefDrive,
    SysrefMode, SysrefModeCode, Temperature,
};
use crate::portable::ti_spi::Register;

/// CLKIN's range (`CLKIN_MIN`, `CLKIN_MAX`), in Hz.
const CLKIN_HZ: core::ops::RangeInclusive<u64> = 300_000_000..=12_800_000_000;
/// The logic clock's range (`LOGICLKOUT_MIN`, `LOGICLKOUT_MAX`), in Hz.
const LOGIC_HZ: core::ops::RangeInclusive<u64> = 1_000_000..=800_000_000;
/// The state machine clock pre-divider's output limit (`SMCLK_DIV_PRE_OUT_MAX`), in Hz.
const STATE_MACHINE_PRE_OUT_MAX_HZ: u64 = 1_600_000_000;
/// The state machine clock's limit (`SMCLK_DIV_OUT_MAX`), in Hz.
const STATE_MACHINE_OUT_MAX_HZ: u64 = 30_000_000;
/// The logic and SYSREF pre-dividers' output limit (`LOGICLK_DIV_PRE_OUT_MAX`,
/// `SYSREF_DIV_PRE_OUT_MAX`), in Hz.
const PRE_OUT_MAX_HZ: u64 = 3_200_000_000;
/// The main divider's range (`CLK_DIV_MIN`, `CLK_DIV_MAX`).
const DIVIDER: core::ops::RangeInclusive<u32> = 2..=8;
/// The multiplier's range (`CLK_MULT_MIN`, `CLK_MULT_MAX`).
const MULTIPLIER: core::ops::RangeInclusive<u32> = 1..=4;
/// The logic divider's largest value (`LOGICLK_DIV_MAX`); below 2 is refused outright.
const LOGIC_DIVIDER_MAX: u32 = 1023;
/// The SYSREF divider's largest value (`SYSREF_DIV_MAX`); below 2 is refused outright.
const SYSREF_DIVIDER_MAX: u32 = 4095;
/// How far a derived frequency may land from its request (`FREQ_EPS`), in Hz.
const TOLERANCE_HZ: f64 = 1.0;

/// Why the LMX1204 has no configuration for a request (libusdr's `-EINVAL` for each,
/// before any bus access).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlanError {
    /// CLKIN is outside 300 MHz to 12.8 GHz.
    ClockInOutOfRange,
    /// CLKOUT is outside its mode's range.
    ClockOutOutOfRange,
    /// The logic clock is outside 1 to 800 MHz.
    LogicOutOfRange,
    /// SYSREFOUT is above its mode's limit.
    SysrefOutOfRange,
    /// With SYSREF on, an enabled channel's delay steps do not pair up (`I + Q != 0x7F`).
    UnpairedDelay,
    /// No divider or multiplier reaches CLKOUT within 1 Hz (or, in precision mode, CLKIN is
    /// not CLKOUT times it: libusdr's check, which only the 1:1 filter passes in multiplier
    /// mode), or the state machine clock cannot be divided down to 30 MHz.
    Unreachable,
    /// No logic pre-divider and divider reach the logic clock.
    LogicUnreachable,
    /// The SYSREF output does not divide the delay generator's clock, needs a divider below
    /// 2, is not a whole division of CLKIN, or (repeater mode) differs from SYSREFREQ.
    SysrefUnreachable,
}

impl PlanError {
    /// What the error says, for reports.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::ClockInOutOfRange => "CLKIN out of range",
            Self::ClockOutOutOfRange => "CLKOUT out of range",
            Self::LogicOutOfRange => "logic clock out of range",
            Self::SysrefOutOfRange => "SYSREF output out of range",
            Self::UnpairedDelay => "a SYSREF delay's I and Q steps do not sum to 0x7F",
            Self::Unreachable => "no divider or multiplier reaches CLKOUT",
            Self::LogicUnreachable => "no divider reaches the logic clock",
            Self::SysrefUnreachable => "no divider reaches the SYSREF output",
        }
    }
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

/// One channel's switches (`ch_en`, `clkout_en`, `sysrefout_en`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Channel {
    /// The channel on.
    pub(crate) enabled: bool,
    /// Its clock output on.
    pub(crate) clock: bool,
    /// Its SYSREF output on.
    pub(crate) sysref: bool,
}

/// One SYSREF output's delay (`lmx1204_sysrefout_channel_delay_t`): the interpolator
/// phase and its I and Q steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Delay {
    /// The interpolator phase (`SYSREFOUTn_DELAY_PHASE`).
    pub(crate) phase: u2,
    /// The I step (`SYSREFOUTn_DELAY_I`).
    pub(crate) i: u7,
    /// The Q step (`SYSREFOUTn_DELAY_Q`); TI pairs it with I to sum to 0x7F.
    pub(crate) q: u7,
}

impl Delay {
    /// The delay `lmx1204_create` gives every output "according to TICS Pro": phase QCLK0
    /// (1), I = 7, Q = 0x7F - I.
    pub(crate) const CREATE: Self = Self {
        phase: u2::new(1),
        i: u7::new(7),
        q: u7::new(0x7f - 7),
    };

    /// This output's delay with the previous output's Q (R18 to R21).
    fn after(self, previous: Self) -> SysrefDelay {
        SysrefDelay::new()
            .with_delay_i(self.i)
            .with_delay_phase(self.phase)
            .with_previous_q(previous.q)
    }
}

/// What `lmx1204_solver` is asked for: the fields of `lmx1204_state_t` a board sets.
/// libusdr's frequencies other than CLKIN and SYSREFREQ are `double`s; they are whole here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    /// CLKIN, in Hz (`clkin`).
    pub(crate) clkin_hz: u64,
    /// The SYSREF request input, in Hz (`sysrefreq`); repeater mode must repeat it.
    pub(crate) sysref_request_hz: u64,
    /// With CLKIN = CLKOUT, filter through the multiplier rather than buffer
    /// (`filter_mode`).
    pub(crate) filter: bool,
    /// The SYSREF mode (`sysref_mode`).
    pub(crate) sysref_mode: SysrefModeCode,
    /// SYSREF on (`sysref_en`).
    pub(crate) sysref: bool,
    /// Channels 0 to 3, then the logic channel (`LMX1204_CH_LOGIC`).
    pub(crate) channels: [Channel; 5],
    /// CLKOUT, in Hz (`clkout`).
    pub(crate) clkout_hz: u64,
    /// SYSREFOUT, in Hz (`sysrefout`).
    pub(crate) sysref_out_hz: u64,
    /// The logic clock, in Hz (`logiclkout`).
    pub(crate) logic_hz: u64,
    /// LOGICLKOUT's format (`logiclkout_fmt`).
    pub(crate) logic_clock_format: Format,
    /// LOGISYSREFOUT's format (`logisysrefout_fmt`).
    pub(crate) logic_sysref_format: Format,
    /// Each SYSREF output's delay, channels 0 to 3, then the logic channel
    /// (`sysref_indiv_ch_delay`).
    pub(crate) delays: [Delay; 5],
    /// Precision mode (`prec_mode`): CLKOUT and the logic clock must divide CLKIN exactly,
    /// not within 1 Hz.
    pub(crate) precision: bool,
}

/// A plan: the mode and ratio, the state machine clock, the logic clock and SYSREF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The request.
    request: Request,
    /// Buffer, divider or multiplier.
    mode: Mode,
    /// The divider or multiplier; 1 in buffer mode.
    ratio: u32,
    /// The state machine clock's pre-divider (multiplier mode; else 0).
    state_machine_pre: u8,
    /// Its divider as a power of two (multiplier mode; else 0).
    state_machine_log2: u8,
    /// The logic pre-divider (logic clock on; else 0).
    logic_pre: u8,
    /// The logic divider; 0 when bypassed or off.
    logic_divider: u32,
    /// The delay generators' clock division code (SYSREF on; else 0).
    delay_divider: u8,
    /// The delay generators' scale (SYSREF on; else 0).
    delay_scale: u8,
    /// The SYSREF pre-divider as a power of two (SYSREF on; else 0).
    sysref_pre_log2: u32,
    /// The SYSREF divider (SYSREF on; else 0).
    sysref_divider: u32,
}

impl Plan {
    /// Plans `request` (`lmx1204_solver`).
    #[expect(
        clippy::float_cmp,
        reason = "libusdr's precision mode compares doubles exactly"
    )]
    pub(crate) fn solve(request: &Request) -> Result<Self, PlanError> {
        let clkin = request.clkin_hz;
        let clkout = request.clkout_hz;
        let mode = if clkin > clkout {
            Mode::Divider
        } else if clkin < clkout || request.filter {
            Mode::Multiplier
        } else {
            Mode::Buffer
        };
        validate(request, mode)?;

        let (ratio, reached) = match mode {
            Mode::Buffer => (1, c_f64(clkin)),
            Mode::Divider => {
                let ratio = c_unsigned(c_f64(clkin) / c_f64(clkout) + 0.5);
                if !DIVIDER.contains(&ratio) {
                    return Err(PlanError::Unreachable);
                }
                (ratio, c_f64(clkin) / f64::from(ratio))
            }
            Mode::Multiplier => {
                let ratio = c_unsigned(c_f64(clkout) / c_f64(clkin) + 0.5);
                if !MULTIPLIER.contains(&ratio) {
                    return Err(PlanError::Unreachable);
                }
                (ratio, c_f64(clkin) * f64::from(ratio))
            }
        };
        if abs(reached - c_f64(clkout)) > TOLERANCE_HZ {
            return Err(PlanError::Unreachable);
        }
        // `st->clkin != st->clkout * mult_div`, in doubles, whatever the mode.
        if request.precision && c_f64(clkin) != c_f64(clkout) * f64::from(ratio) {
            return Err(PlanError::Unreachable);
        }
        let (state_machine_pre, state_machine_log2) = if mode == Mode::Multiplier {
            state_machine_clock(clkin)?
        } else {
            (0, 0)
        };
        let logic = request.channels[4];
        let (logic_pre, logic_divider) = if logic.enabled && logic.clock {
            logic_clock(clkin, request.logic_hz, request.precision)?
        } else {
            (0, 0)
        };
        let (delay_divider, delay_scale, sysref_pre_log2, sysref_divider) = if request.sysref {
            sysref(request)?
        } else {
            (0, 0, 0, 0)
        };
        Ok(Self {
            request: *request,
            mode,
            ratio,
            state_machine_pre,
            state_machine_log2,
            logic_pre,
            logic_divider,
            delay_divider,
            delay_scale,
            sysref_pre_log2,
            sysref_divider,
        })
    }

    /// Whether the plan uses the multiplier, whose PLL the driver then waits to lock.
    pub(crate) fn multiplies(&self) -> bool {
        self.mode == Mode::Multiplier
    }

    /// The state machine clock as the lock wait leaves it once locked: off, "to reduce
    /// spurs" (`lmx1204_wait_pll_lock`).
    pub(crate) fn state_machine_off(&self) -> (u8, u16) {
        self.state_machine(false).entry()
    }

    /// [`StateMachineClock`] with the plan's pre-divider.
    fn state_machine(&self, enabled: bool) -> StateMachineClock {
        StateMachineClock::new()
            .with_pre_divider(u4::new(self.state_machine_pre))
            .with_enabled(enabled)
            .with_reserved(u5::new(3))
    }

    /// The registers `lmx1204_solver` writes after its reset, in its order, ending with R0,
    /// whose write starts the multiplier's calibration.
    pub(crate) fn registers(&self) -> [(u8, u16); 28] {
        let raw = |reg: Reg, value| (u8::from(reg), value);
        let request = &self.request;
        let bypass = self.logic_bypassed();
        let multiplies = self.multiplies();
        let channels = request.channels;
        let [ch0, ch1, ch2, ch3, logic] = channels;
        let bits = |pick: fn(&Channel) -> bool| {
            channels[..4]
                .iter()
                .rev()
                .fold(0u8, |bits, channel| bits << 1 | u8::from(pick(channel)))
        };
        let scale = u2::new(self.delay_scale);
        let [r22, r21, r20, r19, r18, r17] = self.delay_registers();
        let ratio = u8::try_from(if multiplies {
            self.ratio
        } else {
            self.ratio - 1
        })
        .expect("invariant: the ratio is at most 8");
        [
            raw(Reg::LogicBypass90, if bypass { 0x60 } else { 0 }),
            // `MUXOUT_EN_OVRD` off.
            raw(Reg::MuxOutOverride, 0),
            raw(Reg::LogicBypass79, if bypass { 0x104 } else { 5 }),
            raw(Reg::Undocumented76, 0),
            // SYSREF delay generator engaged in generator mode, bypassed in repeater mode.
            raw(Reg::SysrefRetime, 0),
            // The multiplier's calibration values (the YAML gives 0x5666 for R33, libusdr
            // writes 0x6666), or the reset values without it.
            raw(
                Reg::MultiplierTrim67,
                if multiplies { 0x51cb } else { 0x50c8 },
            ),
            raw(Reg::MultiplierTrim34, if multiplies { 0x04c5 } else { 0 }),
            raw(
                Reg::MultiplierTrim33,
                if multiplies { 0x6666 } else { 0x7777 },
            ),
            ClockMux::new()
                .with_reserved(u9::new(4))
                .with_ratio(u3::new(ratio))
                .with_mode(self.mode)
                .entry(),
            Temperature::new().with_counting(true).entry(),
            MuxOut::readback(true)
                .with_logic_scale(scale)
                .with_scale3(scale)
                .with_scale2(scale)
                .entry(),
            r22,
            r21,
            r20,
            r19,
            r18,
            r17,
            SysrefDivider::new()
                .with_pulses(u4::new(1))
                .with_divider(u12::from_raw(low_u16(self.sysref_divider)))
                .entry(),
            self.sysref(0),
            self.delay_step(),
            self.logic_divider(false),
            LogicClock::new()
                .with_pre_divider(u3::new(self.logic_pre))
                .with_reserved(true)
                .with_enabled(logic.enabled)
                .with_sysref_format(request.logic_sysref_format)
                .with_clock_format(request.logic_clock_format)
                .entry(),
            LogicSysrefOutput::new().with_enabled(logic.sysref).entry(),
            // Every SYSREF common mode and SYSREFOUT3's power at 4.
            SysrefDrive::new()
                .with_logic_clock(logic.clock)
                .with_common_modes(u12::new(0o4444))
                .with_sysref3_power(u3::new(4))
                .entry(),
            Outputs::new()
                .with_clock1_power(u3::new(6))
                .with_clock0_power(u3::new(6))
                .with_sysref(u4::new(bits(|channel| channel.sysref)))
                .with_clock(u4::new(bits(|channel| channel.clock)))
                .entry(),
            Channels::new()
                .with_ch3(ch3.enabled)
                .with_ch2(ch2.enabled)
                .with_ch1(ch1.enabled)
                .with_ch0(ch0.enabled)
                .with_mute_during_calibration(u5::new(0b1_1111))
                .with_state_machine_divider_log2(u3::new(self.state_machine_log2))
                .entry(),
            self.state_machine(multiplies).entry(),
            // Reset cleared last: the write that starts the multiplier's calibration.
            raw(Reg::Control, 0),
        ]
    }

    /// The same plan with the SYSREF outputs delayed by `delays` (channels 0 to 3, then
    /// the logic channel), as `lmx1204_init_sysrefout_ch_delay` changes the state before
    /// `lmx1204_reload_sysrefout_ch_delay`.
    #[allow(
        dead_code,
        reason = "the windowing's, which no libusdr PE_SYNC path calls (see `windowing`)"
    )]
    pub(crate) const fn with_delays(mut self, delays: [Delay; 5]) -> Self {
        self.request.delays = delays;
        self
    }

    /// R22 down to R17: the delay generators' clock division and scale, each output's
    /// delay and the SYSREF mode (`lmx1204_reload_sysrefout_ch_delay`'s list, which the
    /// solver's repeats).
    pub(crate) fn delay_registers(&self) -> [(u8, u16); 6] {
        let raw = |reg: Reg, value: SysrefDelay| (u8::from(reg), value.into());
        let scale = u2::new(self.delay_scale);
        let [ch0, ch1, ch2, ch3, logic] = self.request.delays;
        [
            DelayScale::new()
                .with_scale1(scale)
                .with_scale0(scale)
                .with_delay_divider(u3::new(self.delay_divider))
                .with_logic_q(logic.q)
                .entry(),
            raw(Reg::LogicSysrefDelay, logic.after(ch3)),
            raw(Reg::SysrefDelay3, ch3.after(ch2)),
            raw(Reg::SysrefDelay2, ch2.after(ch1)),
            raw(Reg::SysrefDelay1, ch1.after(ch0)),
            SysrefMode::new()
                .with_delay_i(ch0.i)
                .with_delay_phase(ch0.phase)
                .with_mode(self.request.sysref_mode)
                .entry(),
        ]
    }

    /// R9 as the plan sets it, with the dividers' synchronization on or off (`SYNC_EN`):
    /// off in the solver's registers and after the windowing, on before the SYSREF it
    /// captures.
    pub(crate) fn logic_divider(&self, sync: bool) -> (u8, u16) {
        LogicDivider::new()
            .with_sync(sync)
            .with_bypass(self.logic_bypassed())
            .with_divider(u10::from_raw(low_u16(self.logic_divider)))
            .entry()
    }

    /// R15 as the plan sets it, with SYSREFREQ delayed by `delay` steps (0 in the solver's
    /// registers; the capture's centring delay after a windowing).
    pub(crate) fn sysref(&self, delay: u8) -> (u8, u16) {
        Sysref::new()
            .with_pre_divider_log2(u2::from_raw(low_u8(self.sysref_pre_log2)))
            .with_reserved(u2::new(3))
            .with_enabled(self.request.sysref)
            .with_request_delay(u6::from_raw(delay))
            .entry()
    }

    /// R13: the SYSREFREQ delay step size for CLKIN (`lmx1204_get_delay_step_size`).
    pub(crate) fn delay_step(&self) -> (u8, u16) {
        (Reg::DelayStepSize.into(), delay_step(self.request.clkin_hz))
    }

    /// The logic divider is bypassed: CLKIN itself through a pre-divider of 1.
    fn logic_bypassed(&self) -> bool {
        self.logic_pre == 1 && self.logic_divider == 0
    }
}

/// `lmx1204_solver_prevalidate`'s range checks.
fn validate(request: &Request, mode: Mode) -> Result<(), PlanError> {
    if !CLKIN_HZ.contains(&request.clkin_hz) {
        return Err(PlanError::ClockInOutOfRange);
    }
    let logic = request.channels[4];
    if logic.enabled && logic.clock && !LOGIC_HZ.contains(&request.logic_hz) {
        return Err(PlanError::LogicOutOfRange);
    }
    let clkout = match mode {
        Mode::Divider => 150_000_000..=6_400_000_000,
        Mode::Multiplier => 3_200_000_000..=6_400_000_000,
        Mode::Buffer => CLKIN_HZ,
    };
    if !clkout.contains(&request.clkout_hz) {
        return Err(PlanError::ClockOutOutOfRange);
    }
    if request.sysref {
        // A pulser has no limit; the generator 200 MHz, the repeater 100 MHz.
        let limit = match request.sysref_mode {
            SysrefModeCode::Continuous => Some(200_000_000),
            SysrefModeCode::Repeater => Some(100_000_000),
            SysrefModeCode::Pulser => None,
        };
        if limit.is_some_and(|limit| request.sysref_out_hz > limit) {
            return Err(PlanError::SysrefOutOfRange);
        }
        // Every phase is valid (`u2`); an enabled channel's I and Q must sum to 0x7F.
        let unpaired = request
            .channels
            .iter()
            .zip(request.delays)
            .any(|(channel, delay)| {
                channel.enabled && u16::from(delay.i.value()) + u16::from(delay.q.value()) != 0x7f
            });
        if unpaired {
            return Err(PlanError::UnpairedDelay);
        }
    }
    Ok(())
}

/// The state machine clock's pre-divider and divider in multiplier mode: the smallest
/// pre-divider (2, 4 or 8) that brings CLKIN to 1.6 GHz, then the smallest power of two
/// that brings that to 30 MHz.
fn state_machine_clock(clkin: u64) -> Result<(u8, u8), PlanError> {
    // `MAX(ceil((double)clkin / SMCLK_DIV_PRE_OUT_MAX), SMCLK_DIV_PRE_PL2)` (`float`).
    let pre: u8 = match clkin.div_ceil(STATE_MACHINE_PRE_OUT_MAX_HZ).max(2) {
        3..=4 => 4,
        5..=8 => 8,
        _ => 2,
    };
    // `MAX(ceil(log2((double)clkin / div_pre / SMCLK_DIV_OUT_MAX)), SMCLK_DIV_PL1)`.
    let log2 = ceil_log2_ratio(clkin, u64::from(pre) * STATE_MACHINE_OUT_MAX_HZ);
    let log2 = u8::try_from(log2)
        .ok()
        .filter(|&log2| log2 <= 7)
        .ok_or(PlanError::Unreachable)?;
    Ok((pre, log2))
}

/// The logic clock's pre-divider and divider (0 when bypassed): CLKIN itself through a
/// pre-divider of 1, or the first of 2 and 4 (from the one that brings CLKIN to 3.2 GHz)
/// whose nearest divider lands within 1 Hz (in precision mode, exactly).
#[expect(
    clippy::float_cmp,
    reason = "libusdr's precision mode compares doubles exactly"
)]
fn logic_clock(clkin: u64, logic_hz: u64, precision: bool) -> Result<(u8, u32), PlanError> {
    if logic_hz == clkin {
        return Ok((1, 0));
    }
    // `MAX(ceil((double)clkin / LOGICLK_DIV_PRE_OUT_MAX), LOGICLK_DIV_PRE_PL2)` (`float`).
    let start = clkin.div_ceil(PRE_OUT_MAX_HZ).max(2);
    for pre in (start..=4).filter(|&pre| pre != 3) {
        let pre_f = c_f64(pre);
        let divider = c_unsigned(c_f64(clkin) / pre_f / c_f64(logic_hz) + 0.5);
        if divider < 2 {
            return Err(PlanError::LogicOutOfRange);
        }
        if divider > LOGIC_DIVIDER_MAX {
            continue;
        }
        let reached = c_f64(clkin) / pre_f / f64::from(divider);
        if abs(c_f64(logic_hz) - reached) > TOLERANCE_HZ {
            continue;
        }
        // `st->clkin != st->logiclkout * div_pre * div`, in doubles.
        if precision && c_f64(clkin) != c_f64(logic_hz) * pre_f * f64::from(divider) {
            continue;
        }
        let pre = u8::try_from(pre).map_err(|_| PlanError::LogicUnreachable)?;
        return Ok((pre, divider));
    }
    Err(PlanError::LogicUnreachable)
}

/// SYSREF's delay divider and scale, pre-divider and divider.
///
/// libusdr's search starts with `found = true`, so when no pre-divider leaves a divider of
/// at most 4095 it carries on with the pre-divider past the last (3, which the 2-bit field
/// keeps) and the last divider tried; so does this.
fn sysref(request: &Request) -> Result<(u8, u8, u32, u32), PlanError> {
    let clkin = request.clkin_hz;
    // `SYSREF_DELAY_DIV`: /2 up to 1.6 GHz, /4 to 3.2 GHz, /8 to 6.4 GHz, /16 above.
    let (delay_divider, interpolator_hz) = if clkin <= 1_600_000_000 {
        (0, clkin >> 1)
    } else if clkin <= 3_200_000_000 {
        (1, clkin >> 2)
    } else if clkin <= 6_400_000_000 {
        (2, clkin >> 3)
    } else {
        (4, clkin >> 4)
    };
    // `SYSREFOUTn_DELAY_SCALE` for the interpolator's frequency.
    let delay_scale = match interpolator_hz {
        hz if hz <= 200_000_000 => 2,
        hz if hz <= 400_000_000 => 1,
        _ => 0,
    };
    if request.sysref_mode == SysrefModeCode::Repeater {
        if request.sysref_out_hz != request.sysref_request_hz {
            return Err(PlanError::SysrefUnreachable);
        }
        // libusdr's defaults: pre-divider 4, divider 3.
        return Ok((delay_divider, delay_scale, 2, 3));
    }
    // `f_interpol % (uint64_t)sysrefout`; libusdr divides by zero for a 0 Hz output,
    // which this refuses instead.
    let out = c_u64(c_f64(request.sysref_out_hz));
    if out == 0 || interpolator_hz % out != 0 {
        return Err(PlanError::SysrefUnreachable);
    }
    // `MAX(log2f(ceil((double)clkin / SYSREF_DIV_PRE_OUT_MAX)), SYSREF_DIV_PRE_PL1)`: a
    // whole number from 1 to 4 here, so its log2 floors to `ilog2` (`float`).
    let mut pre = clkin.div_ceil(PRE_OUT_MAX_HZ).max(1).ilog2();
    let mut divider = 0;
    while pre <= 2 {
        divider = c_unsigned(c_f64(clkin >> pre) / c_f64(request.sysref_out_hz) + 0.5);
        if divider < 2 {
            return Err(PlanError::SysrefOutOfRange);
        }
        if divider <= SYSREF_DIVIDER_MAX {
            if clkin != (out * u64::from(divider)) << pre {
                return Err(PlanError::SysrefUnreachable);
            }
            break;
        }
        pre += 1;
    }
    Ok((delay_divider, delay_scale, pre, divider))
}

/// The C's narrowing of an `unsigned` into a `uint16_t` field: the low 16 bits.
const fn low_u16(value: u32) -> u16 {
    let [lo, hi, ..] = value.to_le_bytes();
    u16::from_le_bytes([lo, hi])
}

/// The C's narrowing of an `unsigned` into a `uint8_t` field: the low 8 bits.
const fn low_u8(value: u32) -> u8 {
    value.to_le_bytes()[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every output, clock and SYSREF, on.
    const ALL: Channel = Channel {
        enabled: true,
        clock: true,
        sysref: true,
    };

    /// `PE_SYNC`'s LMX1204: LMX2820 #0's 1 GHz multiplied by 4, continuous SYSREF at
    /// 3.125 MHz, a 125 MHz LVDS logic clock.
    const BOARD: Request = Request {
        clkin_hz: 1_000_000_000,
        sysref_request_hz: 25_000_000,
        filter: false,
        sysref_mode: SysrefModeCode::Continuous,
        sysref: true,
        channels: [ALL; 5],
        clkout_hz: 4_000_000_000,
        sysref_out_hz: 3_125_000,
        logic_hz: 125_000_000,
        logic_clock_format: Format::Lvds,
        logic_sysref_format: Format::Lvds,
        delays: [Delay::CREATE; 5],
        precision: false,
    };

    #[test]
    fn the_boards_multiplier_plan_matches_libusdrs_registers() {
        // The oracle's golden session.trace, lines 1452-1479.
        let plan = Plan::solve(&BOARD).expect("libusdr solves it");
        assert!(plan.multiplies());
        assert_eq!(
            plan.registers(),
            [
                (0x5a, 0x0000),
                (0x56, 0x0000),
                (0x4f, 0x0005),
                (0x4c, 0x0000),
                (0x48, 0x0000),
                (0x43, 0x51cb),
                (0x22, 0x04c5),
                (0x21, 0x6666),
                (0x19, 0x0223),
                (0x18, 0x0001),
                (0x17, 0xe040),
                (0x16, 0x0078),
                (0x15, 0x0ef8),
                (0x14, 0x0ef8),
                (0x13, 0x0ef8),
                (0x12, 0x0ef8),
                (0x11, 0x0074),
                (0x10, 0x1140),
                (0x0f, 0x0380),
                (0x0d, 0x0000),
                (0x09, 0x0004),
                (0x08, 0x00b0),
                (0x07, 0x0001),
                (0x06, 0xc924),
                (0x04, 0x36ff),
                (0x03, 0xff85),
                (0x02, 0x00a3),
                (0x00, 0x0000),
            ]
        );
        assert_eq!(plan.state_machine_off(), (0x02, 0x0083), "golden line 1484");
    }

    #[test]
    fn a_divider_plan_needs_no_state_machine_clock() {
        let request = Request {
            clkout_hz: 250_000_000,
            ..BOARD
        };
        let plan = Plan::solve(&request).expect("1 GHz / 4");
        assert!(!plan.multiplies());
        let words = plan.registers();
        assert_eq!(
            words[8],
            (0x19, 0x0200 | 3 << 3 | 2),
            "divider 4, as CLK_DIV + 1"
        );
        assert_eq!(
            words[26],
            (0x02, 0x0003),
            "state machine clock off, pre-divider 0"
        );
    }

    #[test]
    fn a_sysref_output_that_does_not_divide_the_interpolator_is_refused() {
        let request = Request {
            sysref_out_hz: 3_000_000,
            ..BOARD
        };
        assert_eq!(Plan::solve(&request), Err(PlanError::SysrefUnreachable));
    }
}
