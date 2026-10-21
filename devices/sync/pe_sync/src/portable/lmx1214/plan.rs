//! The LMX1214's plan: libusdr's `lmx1214_solver` (with `lmx1214_solver_prevalidate`) and
//! the register list it writes; and the registers its SYNC windowing writes from the plan.

use core::fmt;

use bnb::{u3, u9, u10, u12};

use super::super::float::{abs, c_f64, c_u64, c_unsigned};
use super::super::ti_sync::delay_step;
use super::reg::{
    AuxBypass, AuxDivider, AuxFormat, AuxOutput, Capture, ClockMux, MainMode, Outputs, Reg,
};
use crate::portable::ti_spi::Register;

/// The input range (`CLKIN_MIN`, `CLKIN_MAX_BUF`), in Hz.
const INPUT_HZ: core::ops::RangeInclusive<u64> = 300_000_000..=18_000_000_000;
/// The highest input the divider takes (`CLKIN_MAX_DIV`), in Hz.
const DIVIDER_INPUT_MAX_HZ: u64 = 12_800_000_000;
/// Buffer mode's output range (`CLKOUT_MIN_BUF`, `CLKOUT_MAX_BUF`), in Hz.
const BUFFER_OUTPUT_HZ: core::ops::RangeInclusive<u64> = 300_000_000..=18_000_000_000;
/// Divider mode's output range (`CLKOUT_MIN_DIV`, `CLKOUT_MAX_DIV_SYNC`), in Hz.
const DIVIDER_OUTPUT_HZ: core::ops::RangeInclusive<u64> = 150_000_000..=6_400_000_000;
/// The auxiliary output's range (`AUXCLKOUT_MIN`, `AUXCLKOUT_MAX`), in Hz.
const AUX_HZ: core::ops::RangeInclusive<f64> = 1e6..=800e6;
/// The most the auxiliary pre-divider may pass on (`AUXCLK_DIV_INP_MAX`), in Hz.
const AUX_DIVIDER_INPUT_MAX_HZ: u64 = 3_200_000_000;
/// The main divider's range (`CLK_DIV_MIN`, `CLK_DIV_MAX`).
const DIVIDER: core::ops::RangeInclusive<u32> = 2..=8;
/// The auxiliary divider's range (`AUXCLK_DIV_MIN`, `AUXCLK_DIV_MAX`).
const AUX_DIVIDER: core::ops::RangeInclusive<u32> = 2..=1023;
/// How far a divided frequency may land from its request (`FREQ_EPS`), in Hz.
const TOLERANCE_HZ: f64 = 1.0;

/// Why the LMX1214 has no configuration for a request (libusdr's `-EINVAL` for each,
/// before any bus access).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlanError {
    /// The input is outside 300 MHz to 18 GHz, or above 12.8 GHz for the divider.
    InputOutOfRange,
    /// The output is outside its mode's range.
    OutputOutOfRange,
    /// The auxiliary output is outside 1 to 800 MHz.
    AuxOutOfRange,
    /// Outputs 2 and 3 differ in being on (`OUT_EN2 != OUT_EN3`).
    UnevenOutputs,
    /// No divider from 2 to 8 reaches the output within 1 Hz, or (precision mode) none
    /// divides the input exactly.
    Unreachable,
    /// No auxiliary pre-divider and divider reach the auxiliary output within 1 Hz (in
    /// precision mode, exactly).
    AuxUnreachable,
}

impl PlanError {
    /// What the error says, for reports.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::InputOutOfRange => "input out of range",
            Self::OutputOutOfRange => "output frequency out of range",
            Self::AuxOutOfRange => "auxiliary output frequency out of range",
            Self::UnevenOutputs => "outputs 2 and 3 must be on or off together",
            Self::Unreachable => "no divider reaches the output",
            Self::AuxUnreachable => "no divider reaches the auxiliary output",
        }
    }
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

/// The auxiliary output as `lmx1214_auxclkout_cfg_t` describes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Aux {
    /// On (`enable`).
    pub(crate) enabled: bool,
    /// Its frequency, in Hz (`freq`; a `double` in libusdr, whole here).
    pub(crate) hz: u64,
    /// Its format (`fmt`).
    pub(crate) format: AuxFormat,
}

/// What `lmx1214_solver` is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    /// The input, in Hz (`in`).
    pub(crate) input_hz: u64,
    /// The four main outputs' frequency, in Hz (`out`); equal to the input for buffer mode.
    pub(crate) output_hz: u64,
    /// Which main outputs are on (`out_en`).
    pub(crate) outputs: [bool; 4],
    /// The auxiliary output.
    pub(crate) aux: Aux,
    /// Precision mode (`prec_mode`): every divider must divide exactly, not within 1 Hz.
    pub(crate) precision: bool,
}

/// A plan: the main outputs' mode and divider, and the auxiliary path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The input, in Hz.
    input_hz: u64,
    /// Buffer or divider.
    mode: MainMode,
    /// The main divider; 1 in buffer mode.
    divider: u8,
    /// Which main outputs are on.
    outputs: [bool; 4],
    /// The auxiliary pre-divider: 1, 2 or 4.
    aux_pre_divider: u8,
    /// The auxiliary divider is bypassed.
    aux_bypass: bool,
    /// The auxiliary divider.
    aux_divider: u16,
    /// The auxiliary output.
    aux: Aux,
}

impl Plan {
    /// Plans `request` (`lmx1214_solver`).
    pub(crate) fn solve(request: &Request) -> Result<Self, PlanError> {
        let Request {
            input_hz,
            output_hz,
            outputs,
            aux,
            precision,
        } = *request;
        if !INPUT_HZ.contains(&input_hz) {
            return Err(PlanError::InputOutOfRange);
        }
        let buffer = output_hz == input_hz;
        if !buffer && input_hz > DIVIDER_INPUT_MAX_HZ {
            return Err(PlanError::InputOutOfRange);
        }
        let outputs_range = if buffer {
            BUFFER_OUTPUT_HZ
        } else {
            DIVIDER_OUTPUT_HZ
        };
        if !outputs_range.contains(&output_hz) {
            return Err(PlanError::OutputOutOfRange);
        }
        let aux_hz = c_f64(aux.hz);
        if aux.enabled && !AUX_HZ.contains(&aux_hz) {
            return Err(PlanError::AuxOutOfRange);
        }
        if outputs[2] != outputs[3] {
            return Err(PlanError::UnevenOutputs);
        }

        let (mode, divider) = if buffer {
            (MainMode::Buffer, 1)
        } else {
            let divider = c_unsigned(c_f64(input_hz) / c_f64(output_hz) + 0.5);
            if !DIVIDER.contains(&divider) {
                return Err(PlanError::Unreachable);
            }
            let reached = c_f64(input_hz) / f64::from(divider);
            if abs(reached - c_f64(output_hz)) > TOLERANCE_HZ {
                return Err(PlanError::Unreachable);
            }
            // `in != out * clk_div`, in 64-bit integers.
            if precision && input_hz != output_hz.wrapping_mul(u64::from(divider)) {
                return Err(PlanError::Unreachable);
            }
            (MainMode::Divider, divider)
        };

        let (aux_pre_divider, aux_bypass, aux_divider) = if aux.enabled {
            aux_path(input_hz, aux_hz, precision).ok_or(PlanError::AuxUnreachable)?
        } else {
            // libusdr's defaults: pre-divider 4, divider 0x20.
            (4, false, 0x20)
        };
        Ok(Self {
            input_hz,
            mode,
            divider: u8::try_from(divider).map_err(|_| PlanError::Unreachable)?,
            outputs,
            aux_pre_divider,
            aux_bypass,
            aux_divider,
            aux,
        })
    }

    /// The registers `lmx1214_solver` writes between the main divider's reset and its
    /// release, in its order.
    pub(crate) fn registers(&self) -> [(u8, u16); 11] {
        let raw = |reg: Reg, value| (u8::from(reg), value);
        let [out0, out1, out2, out3] = self.outputs;
        [
            AuxBypass::new()
                .with_third(self.aux_bypass)
                .with_second(self.aux_bypass)
                .entry(),
            // R79 = 5, as the YAML says.
            raw(Reg::Undisclosed79, 0x0005),
            // R75's undisclosed bits 3:0 = 6, as the YAML says.
            raw(Reg::PinStatus, 0x0006),
            ClockMux::new()
                .with_undisclosed(u9::new(4))
                .with_divider_less_one(u3::new(self.divider - 1))
                .with_mode(self.mode)
                .entry(),
            // No SYNC latch, no clock-position capture.
            capture(false),
            self.aux_divider(false),
            AuxOutput::new()
                .with_pre_divider(u3::new(self.aux_pre_divider))
                .with_enabled(self.aux.enabled)
                .with_format(self.aux.format)
                .entry(),
            // R7 = 0x543F: output power 7 (`AUXCLKOUT_PWR`), pre-driver 0
            // (`AUXCLK_DIV_PWR_PRE`), common mode 1.0 V (`AUXCLKOUT_VCM` = 2) and the
            // undisclosed fields at the YAML's values.
            raw(Reg::AuxPower, 0x543f),
            Outputs::new()
                .with_out3(out3)
                .with_out2(out2)
                .with_out1(out1)
                .with_out0(out0)
                .with_undisclosed(u12::new(0xf86))
                .entry(),
            // R2 = 0x0223: the state machine clock on (`SMCLK_EN`), undisclosed fields 8
            // and 3, "en state machine".
            raw(Reg::StateMachine, 0x0223),
            // R0 = 0: not powered down.
            raw(Reg::Control, 0x0000),
        ]
    }

    /// R9 as the plan sets it, with the SYNC path on or off (`SYNC_EN`): off in the
    /// solver's registers and after the windowing, on before the SYNC it captures.
    pub(crate) fn aux_divider(&self, sync: bool) -> (u8, u16) {
        AuxDivider::new()
            .with_sync(sync)
            .with_bypass(self.aux_bypass)
            .with_divider(u10::new(self.aux_divider))
            .entry()
    }

    /// The SYNC delay step for the plan's input (`lmx1214_sysref_windowing_beforesync`).
    pub(crate) fn sync_delay_step(&self) -> (u8, u16) {
        (Reg::SyncDelayStep.into(), delay_step(self.input_hz))
    }
}

/// R14 with the clock-position capture on or off.
pub(crate) fn capture(enabled: bool) -> (u8, u16) {
    Capture::new()
        .with_enabled(enabled)
        .with_undisclosed(true)
        .entry()
}

/// The auxiliary pre-divider, bypass and divider for `aux_hz` (the solver's search): the
/// smallest pre-divider whose output the divider takes, from 1 (bypassing the divider,
/// when it hits the frequency exactly), then 2 and 4. In precision mode a pre-divider must
/// divide the input exactly, as the divider its output.
#[expect(
    clippy::float_cmp,
    reason = "libusdr bypasses the divider only on an exact match, and compares doubles exactly in precision mode"
)]
fn aux_path(input_hz: u64, aux_hz: f64, precision: bool) -> Option<(u8, bool, u16)> {
    let start: u8 = if input_hz <= AUX_DIVIDER_INPUT_MAX_HZ {
        1
    } else if input_hz <= AUX_DIVIDER_INPUT_MAX_HZ << 1 {
        2
    } else {
        4
    };
    for pre in (start..=4).filter(|&pre| pre != 3) {
        let into_divider = c_f64(input_hz) / f64::from(pre);
        // `in != (uint64_t)(fmid + 0.5) * auxclk_div_pre`.
        if precision && input_hz != c_u64(into_divider + 0.5).wrapping_mul(u64::from(pre)) {
            continue;
        }
        if pre == 1 {
            if into_divider == aux_hz {
                return Some((1, true, 0x20));
            }
            // The divider cannot follow a pre-divider of 1 unbypassed.
            continue;
        }
        let divider = c_unsigned(into_divider / aux_hz + 0.5);
        if !AUX_DIVIDER.contains(&divider) {
            continue;
        }
        if abs(into_divider / f64::from(divider) - aux_hz) > TOLERANCE_HZ {
            continue;
        }
        if precision && into_divider != aux_hz * f64::from(divider) {
            continue;
        }
        return Some((pre, false, u16::try_from(divider).ok()?));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PE_SYNC`'s LMX1214: 550 MHz buffered to every output and to the auxiliary output in
    /// LVDS.
    const BOARD: Request = Request {
        input_hz: 550_000_000,
        output_hz: 550_000_000,
        outputs: [true; 4],
        aux: Aux {
            enabled: true,
            hz: 550_000_000,
            format: AuxFormat::Lvds,
        },
        precision: false,
    };

    #[test]
    fn the_boards_buffer_and_bypassed_aux_match_libusdrs_registers() {
        // The oracle's golden session.trace, lines 1426-1436.
        let words = Plan::solve(&BOARD).expect("libusdr solves it").registers();
        assert_eq!(
            words,
            [
                (0x5a, 0x0060),
                (0x4f, 0x0005),
                (0x4b, 0x0006),
                (0x19, 0x0201),
                (0x0e, 0x0002),
                (0x09, 0x0820),
                (0x08, 0x0050),
                (0x07, 0x543f),
                (0x03, 0xff86),
                (0x02, 0x0223),
                (0x00, 0x0000),
            ]
        );
    }

    #[test]
    fn a_divided_output_and_aux_take_the_nearest_dividers() {
        // 4.4 GHz / 8 = 550 MHz; the aux path skips the bypass (no exact hit) and the
        // pre-divider of 1, and divides 2.2 GHz by 22 for 100 MHz.
        let request = Request {
            input_hz: 4_400_000_000,
            aux: Aux {
                hz: 100_000_000,
                ..BOARD.aux
            },
            ..BOARD
        };
        let words = Plan::solve(&request).expect("solvable").registers();
        assert_eq!(words[3], (0x19, 0x0200 | 7 << 3 | 2), "divide by 8");
        assert_eq!(words[5], (0x09, 22), "aux divider 22, not bypassed");
        assert_eq!(
            words[6],
            (0x08, 2 << 6 | 1 << 4),
            "aux pre-divider 2, on, LVDS"
        );
    }

    #[test]
    fn outputs_2_and_3_must_switch_together() {
        let request = Request {
            outputs: [true, true, true, false],
            ..BOARD
        };
        assert_eq!(Plan::solve(&request), Err(PlanError::UnevenOutputs));
    }
}
