//! Integer minimum searches and the GCD (`cal/opt_func.c`).
//!
//! An objective maps an integer parameter to an integer cost; errors from it stop the
//! search and are returned unchanged, as the C returns the evaluator's status.

use core::cmp::{max, min};

/// libusdr's golden-ratio conjugate (`float gr = 0.61803398875f`).
#[expect(
    clippy::excessive_precision,
    reason = "libusdr's literal, rounded to f32 exactly as the C compiler does"
)]
const GOLDEN: f32 = 0.618_033_988_75;

/// A located minimum: the parameter and its cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Minimum {
    /// The parameter.
    pub x: i32,
    /// Its cost.
    pub cost: i32,
}

/// `(b - a) * gr`, truncated to an integer as C's `int d = (b - a) * gr` does.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "C converts the span to float and truncates the product back to int"
)]
fn golden_step(a: i32, b: i32) -> i32 {
    (b.saturating_sub(a) as f32 * GOLDEN) as i32
}

/// Golden-section search for a local minimum of `cost` over `start..=stop`
/// (`find_golden_min`).
///
/// Libusdr's quirks are kept: the interval shrinks by truncated golden steps, the search
/// stops as soon as a step truncates to zero, and the answer is the most recent left probe
/// rather than the better of the two.
///
/// # Errors
///
/// The first error `cost` returns.
pub fn golden_min<E>(
    start: i32,
    stop: i32,
    mut cost: impl FnMut(i32) -> Result<i32, E>,
) -> Result<Minimum, E> {
    let mut search = Line::golden(start, stop);
    while let Some(x) = search.probe() {
        search.report(cost(x)?);
    }
    Ok(search.minimum())
}

/// Exhaustive search over `start, start + step, ...` while below `stop`, where the step
/// is `1 + extra_step` (`find_iterate_min`). The first of equal costs wins.
///
/// As in libusdr, a range with `start >= stop` evaluates nothing and reports `x == -1`
/// with cost `i32::MAX`, even though the C prepares a negative step for `start > stop`.
///
/// # Errors
///
/// The first error `cost` returns.
pub fn iterate_min<E>(
    start: i32,
    stop: i32,
    extra_step: u16,
    mut cost: impl FnMut(i32) -> Result<i32, E>,
) -> Result<Minimum, E> {
    let mut search = Line::iterate(start, stop, extra_step);
    while let Some(x) = search.probe() {
        search.report(cost(x)?);
    }
    Ok(search.minimum())
}

/// Where a golden-section search is: which probe's cost it waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Golden {
    /// The first left probe, `x1`.
    FirstLeft,
    /// The first right probe, `x2`.
    FirstRight,
    /// A new left probe, after the interval moved right.
    Left,
    /// A new right probe, after the interval moved left.
    Right,
    /// Finished.
    Done,
}

/// A one-dimensional minimum search as a resumable machine: [`Line::probe`] names the
/// parameter whose cost it needs next, [`Line::report`] takes the cost. [`golden_min`] and
/// [`iterate_min`] drive it with their closures; the calibration drives it one bus
/// measurement at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// [`golden_min`]'s state: the interval `a..=b`, its step `d`, the probes and their
    /// costs.
    Golden {
        /// The interval's lower end.
        a: i32,
        /// The interval's upper end.
        b: i32,
        /// The truncated golden step.
        d: i32,
        /// The left probe.
        x1: i32,
        /// The right probe.
        x2: i32,
        /// The left probe's cost.
        f1: i32,
        /// The right probe's cost.
        f2: i32,
        /// The probe awaited.
        at: Golden,
    },
    /// [`iterate_min`]'s state: the next parameter, the step, the end and the best so far.
    Iterate {
        /// The next parameter.
        i: i32,
        /// The step.
        delta: i32,
        /// The end, excluded.
        stop: i32,
        /// The best found so far.
        best: Minimum,
    },
}

impl Line {
    /// [`golden_min`] over `start..=stop`, before its first probe.
    pub(crate) fn golden(start: i32, stop: i32) -> Self {
        let d = golden_step(start, stop);
        Self::Golden {
            a: start,
            b: stop,
            d,
            x1: start.saturating_add(d),
            x2: stop.saturating_sub(d),
            f1: 0,
            f2: 0,
            at: Golden::FirstLeft,
        }
    }

    /// [`iterate_min`] over `start..stop` with `extra_step`, before its first probe.
    pub(crate) fn iterate(start: i32, stop: i32, extra_step: u16) -> Self {
        let extra = i32::from(extra_step);
        Self::Iterate {
            i: start,
            delta: if start > stop { -1 - extra } else { 1 + extra },
            stop,
            best: Minimum {
                x: -1,
                cost: i32::MAX,
            },
        }
    }

    /// The parameter whose cost the search needs next; `None` once it has finished.
    pub(crate) fn probe(&self) -> Option<i32> {
        match *self {
            Self::Golden { at, x1, x2, .. } => match at {
                Golden::FirstLeft | Golden::Left => Some(x1),
                Golden::FirstRight | Golden::Right => Some(x2),
                Golden::Done => None,
            },
            Self::Iterate { i, stop, .. } => (i < stop).then_some(i),
        }
    }

    /// Takes the cost at [`Self::probe`]'s parameter; ignored once finished.
    pub(crate) fn report(&mut self, cost: i32) {
        match self {
            Self::Golden {
                a,
                b,
                d,
                x1,
                x2,
                f1,
                f2,
                at,
            } => {
                match at {
                    Golden::FirstLeft => {
                        *f1 = cost;
                        *at = Golden::FirstRight;
                        return;
                    }
                    Golden::Left => *f1 = cost,
                    Golden::FirstRight | Golden::Right => *f2 = cost,
                    Golden::Done => return,
                }
                // The loop's test, then one turn of `find_golden_min`'s loop up to its next
                // probe; a step that truncates to zero ends it before that probe.
                if *d <= 0 {
                    *at = Golden::Done;
                    return;
                }
                if *f1 < *f2 {
                    *a = *x2;
                    *x2 = *x1;
                    *f2 = *f1;
                    *d = golden_step(*a, *b);
                    if *d != 0 {
                        *x1 = a.saturating_add(*d);
                    }
                    *at = Golden::Left;
                } else {
                    *b = *x1;
                    *x1 = *x2;
                    *f1 = *f2;
                    *d = golden_step(*a, *b);
                    if *d != 0 {
                        *x2 = b.saturating_sub(*d);
                    }
                    *at = Golden::Right;
                }
                if *d == 0 {
                    *at = Golden::Done;
                }
            }
            Self::Iterate {
                i,
                delta,
                stop,
                best,
            } => {
                if *i >= *stop {
                    return;
                }
                if cost < best.cost {
                    *best = Minimum { x: *i, cost };
                }
                *i = i.saturating_add(*delta);
            }
        }
    }

    /// The minimum found: once finished, the search's answer.
    pub(crate) const fn minimum(&self) -> Minimum {
        match *self {
            Self::Golden { x1, f1, .. } => Minimum { x: x1, cost: f1 },
            Self::Iterate { best, .. } => best,
        }
    }
}

/// Which one-dimensional search a [`Stage`] uses (`enum search_function`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Search {
    /// [`iterate_min`], with the stage's `extra_step`.
    Iterate,
    /// [`golden_min`].
    Golden,
}

/// A bound pair (`opt_par_limits_t`): both ends are searched by [`Search::Golden`], while
/// [`Search::Iterate`] stops before `max`, as in libusdr.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Lower bound.
    pub min: i32,
    /// Upper bound.
    pub max: i32,
}

/// One round of [`best_2d`] (`struct opt_iteration2d`). Its evaluator is the
/// [`Objective2d`] called with this stage's index, so stages can measure differently, as
/// libusdr's LO calibration does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stage {
    /// The search along each axis.
    pub search: Search,
    /// Bounds for X then Y. The first stage's are absolute; every stage's are also
    /// offsets from the current best, clamped to the first stage's.
    pub limits: [Limits; 2],
    /// [`iterate_min`]'s extra step.
    pub extra_step: u16,
}

/// One coordinate of a two-dimensional search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// The first coordinate.
    X,
    /// The second coordinate.
    Y,
}

/// A two-dimensional objective: setting a coordinate moves the system there; evaluating
/// sets it and measures the cost (`evaluate_fnN_t` with and without a result pointer).
/// `stage` is the index of the [`Stage`] whose evaluator libusdr would call.
pub trait Objective2d {
    /// What a failed set or evaluation reports.
    type Error;

    /// Moves `axis` to `value` without measuring.
    ///
    /// # Errors
    ///
    /// Stops the search. libusdr ignores these failures and keeps measuring at the wrong
    /// point; a successful search makes the same calls either way.
    fn set(&mut self, stage: usize, axis: Axis, value: i32) -> Result<(), Self::Error>;

    /// Moves `axis` to `value` and returns the cost there.
    ///
    /// # Errors
    ///
    /// Stops the search.
    fn evaluate(&mut self, stage: usize, axis: Axis, value: i32) -> Result<i32, Self::Error>;
}

/// The best point of a two-dimensional search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Best2d {
    /// The first coordinate.
    pub x: i32,
    /// The second coordinate.
    pub y: i32,
    /// The cost there.
    pub cost: i32,
}

/// Coordinate-wise search: each stage searches X with Y fixed, then Y with X fixed, and
/// stops early once a cost falls below `stop_when` (`find_best_2d`).
///
/// It starts at the midpoint of the first stage's limits. Libusdr's quirks are kept: each
/// search window runs from the current point plus the stage's lower offset to plus its
/// upper offset, clamped to the first stage's limits, so with non-negative offsets the
/// first windows cover only the upper half; and the reported cost is the best seen,
/// while the reported point combines the best X and best Y found in any stage.
///
/// # Errors
///
/// The first error an evaluation returns.
pub fn best_2d<O: Objective2d>(
    stages: &[Stage],
    stop_when: i32,
    objective: &mut O,
) -> Result<Best2d, O::Error> {
    let mut search = Search2d::new(stages, stop_when);
    while let Some(probe) = search.probe() {
        match probe {
            Probe2d::Set { stage, axis, value } => objective.set(stage, axis, value)?,
            Probe2d::Evaluate { stage, axis, value } => {
                search.report(objective.evaluate(stage, axis, value)?);
                continue;
            }
        }
        search.report(0);
    }
    Ok(search.best())
}

/// What a [`Search2d`] needs done next: the [`Objective2d`] calls, as data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Probe2d {
    /// [`Objective2d::set`]; report any value.
    Set {
        /// The stage whose evaluator libusdr would call.
        stage: usize,
        /// The coordinate.
        axis: Axis,
        /// Its value.
        value: i32,
    },
    /// [`Objective2d::evaluate`]; report the cost.
    Evaluate {
        /// The stage whose evaluator libusdr would call.
        stage: usize,
        /// The coordinate.
        axis: Axis,
        /// Its value.
        value: i32,
    },
}

/// Where a [`Search2d`] is in a stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step2d {
    /// Setting the starting Y.
    Start,
    /// Searching `axis` along this line.
    Searching(Axis, Line),
    /// Setting `axis` to its best.
    Setting(Axis),
    /// Finished.
    Done,
}

/// [`best_2d`] as a resumable machine over stages `S`: [`Search2d::probe`] names the next
/// objective call, [`Search2d::report`] takes its result. The calibration drives it one bus
/// measurement at a time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Search2d<S> {
    /// The stages.
    stages: S,
    /// The cost below which the search stops after a stage.
    stop_when: i32,
    /// The stage running.
    stage: usize,
    /// The current X and Y.
    at: (i32, i32),
    /// The best X, Y and cost so far.
    best: Best2d,
    /// The last line search's cost, which decides the early stop.
    found_cost: i32,
    /// Where it is.
    step: Step2d,
}

impl<S: AsRef<[Stage]>> Search2d<S> {
    /// The search over `stages`, stopping early below `stop_when`, before its first call:
    /// at the midpoint of the first stage's limits.
    pub(crate) fn new(stages: S, stop_when: i32) -> Self {
        let first = stages.as_ref().first().map(|first| first.limits);
        let (at, step) = match first {
            Some([x, y]) => (((x.min + x.max) / 2, (y.min + y.max) / 2), Step2d::Start),
            None => ((0, 0), Step2d::Done),
        };
        Self {
            stages,
            stop_when,
            stage: 0,
            at,
            best: Best2d {
                x: at.0,
                y: at.1,
                cost: i32::MAX,
            },
            found_cost: i32::MAX,
            step,
        }
    }

    /// The next objective call; `None` once the search has finished.
    pub(crate) fn probe(&self) -> Option<Probe2d> {
        let stage = self.stage;
        Some(match self.step {
            // Set the starting Y, as X is searched first.
            Step2d::Start => Probe2d::Set {
                stage,
                axis: Axis::Y,
                value: self.at.1,
            },
            Step2d::Searching(axis, line) => Probe2d::Evaluate {
                stage,
                axis,
                value: line.probe()?,
            },
            Step2d::Setting(axis) => Probe2d::Set {
                stage,
                axis,
                value: match axis {
                    Axis::X => self.best.x,
                    Axis::Y => self.best.y,
                },
            },
            Step2d::Done => return None,
        })
    }

    /// Takes the result of [`Self::probe`]'s call: the cost of an evaluation, any value
    /// for a set. Ignored once finished.
    pub(crate) fn report(&mut self, cost: i32) {
        match self.step {
            Step2d::Start => self.search(Axis::X),
            Step2d::Searching(axis, mut line) => {
                line.report(cost);
                self.step = Step2d::Searching(axis, line);
                self.settle_line();
            }
            Step2d::Setting(Axis::X) => self.search(Axis::Y),
            Step2d::Setting(Axis::Y) => {
                let stages = self.stages.as_ref().len();
                if self.found_cost < self.stop_when || self.stage + 1 >= stages {
                    self.step = Step2d::Done;
                    return;
                }
                // Start over from the best.
                self.at = (self.best.x, self.best.y);
                self.stage += 1;
                self.search(Axis::X);
            }
            Step2d::Done => {}
        }
    }

    /// The best point found: once finished, the search's answer.
    pub(crate) const fn best(&self) -> Best2d {
        self.best
    }

    /// Starts the current stage's line search along `axis`, from the current point.
    fn search(&mut self, axis: Axis) {
        let stages = self.stages.as_ref();
        let (Some(stage), Some(first)) = (stages.get(self.stage), stages.first()) else {
            self.step = Step2d::Done;
            return;
        };
        let (index, at) = match axis {
            Axis::X => (0, self.at.0),
            Axis::Y => (1, self.at.1),
        };
        let limits = first.limits[index];
        let start = max(at.saturating_add(stage.limits[index].min), limits.min);
        let stop = min(at.saturating_add(stage.limits[index].max), limits.max);
        let line = match stage.search {
            Search::Iterate => Line::iterate(start, stop, stage.extra_step),
            Search::Golden => Line::golden(start, stop),
        };
        self.step = Step2d::Searching(axis, line);
        self.settle_line();
    }

    /// Moves past a finished line search: records its minimum, then sets the axis.
    fn settle_line(&mut self) {
        let Step2d::Searching(axis, line) = self.step else {
            return;
        };
        if line.probe().is_some() {
            return;
        }
        let found = line.minimum();
        self.found_cost = found.cost;
        let improved = found.cost < self.best.cost;
        match axis {
            Axis::X => {
                self.at.0 = found.x;
                if improved {
                    self.best.x = found.x;
                }
            }
            Axis::Y => {
                self.at.1 = found.x;
                if improved {
                    self.best.y = found.x;
                }
            }
        }
        if improved {
            self.best.cost = found.cost;
        }
        self.step = Step2d::Setting(axis);
    }
}

/// The greatest common divisor (`find_gcd`); zero only for `gcd(0, 0)`.
#[must_use]
pub fn gcd(a: u64, b: u64) -> u64 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_search_finds_a_unimodal_minimum() {
        let found = golden_min::<()>(0, 1000, |x| Ok((x - 618).abs())).unwrap();
        assert!(
            (found.x - 618).abs() <= 2,
            "within the truncated step of the minimum"
        );
        assert_eq!(found.cost, (found.x - 618).abs());
    }

    #[test]
    fn exhaustive_search_takes_the_first_of_equal_minima_and_skips_empty_ranges() {
        let found = iterate_min::<()>(0, 10, 0, |x| Ok((x % 3 != 0).into())).unwrap();
        assert_eq!(found, Minimum { x: 0, cost: 0 });
        let empty = iterate_min::<()>(5, 5, 0, |_| Ok(0)).unwrap();
        assert_eq!(
            empty,
            Minimum {
                x: -1,
                cost: i32::MAX
            }
        );
    }

    #[test]
    fn an_objective_error_stops_the_search() {
        let mut calls = 0;
        let result = golden_min(0, 100, |_| {
            calls += 1;
            Err("measurement failed")
        });
        assert_eq!(result, Err("measurement failed"));
        assert_eq!(calls, 1);
    }

    #[test]
    fn gcd_matches_euclid_on_edges() {
        assert_eq!(gcd(0, 0), 0);
        assert_eq!(gcd(0, 7), 7);
        assert_eq!(gcd(12, 18), 6);
        assert_eq!(gcd(26_000_000, 20_000_000), 2_000_000);
    }
}
