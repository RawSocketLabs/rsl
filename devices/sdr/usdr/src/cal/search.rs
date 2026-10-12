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
    let (mut a, mut b) = (start, stop);
    let mut d = golden_step(a, b);
    let mut x1 = a.saturating_add(d);
    let mut x2 = b.saturating_sub(d);
    let mut f1 = cost(x1)?;
    let mut f2 = cost(x2)?;
    while d > 0 {
        if f1 < f2 {
            a = x2;
            x2 = x1;
            f2 = f1;
            d = golden_step(a, b);
            if d == 0 {
                break;
            }
            x1 = a.saturating_add(d);
            f1 = cost(x1)?;
        } else {
            b = x1;
            x1 = x2;
            f1 = f2;
            d = golden_step(a, b);
            if d == 0 {
                break;
            }
            x2 = b.saturating_sub(d);
            f2 = cost(x2)?;
        }
    }
    Ok(Minimum { x: x1, cost: f1 })
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
    let extra = i32::from(extra_step);
    let delta = if start > stop { -1 - extra } else { 1 + extra };
    let mut best = Minimum {
        x: -1,
        cost: i32::MAX,
    };
    let mut i = start;
    while i < stop {
        let value = cost(i)?;
        if value < best.cost {
            best = Minimum { x: i, cost: value };
        }
        i = i.saturating_add(delta);
    }
    Ok(best)
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
    let Some(first) = stages.first() else {
        return Ok(Best2d {
            x: 0,
            y: 0,
            cost: i32::MAX,
        });
    };
    let [x_limits, y_limits] = first.limits;
    let mut x = (x_limits.min + x_limits.max) / 2;
    let mut y = (y_limits.min + y_limits.max) / 2;
    let (mut best_x, mut best_y, mut best_cost) = (x, y, i32::MAX);
    // Set the starting Y, as X is searched first.
    objective.set(0, Axis::Y, y)?;
    for (index, stage) in stages.iter().enumerate() {
        let window = |at: i32, axis: usize, limits: Limits| {
            (
                max(at.saturating_add(stage.limits[axis].min), limits.min),
                min(at.saturating_add(stage.limits[axis].max), limits.max),
            )
        };
        let (start, stop) = window(x, 0, x_limits);
        let found = search(stage, start, stop, |value| {
            objective.evaluate(index, Axis::X, value)
        })?;
        x = found.x;
        if found.cost < best_cost {
            best_x = x;
            best_cost = found.cost;
        }
        objective.set(index, Axis::X, best_x)?;

        let (start, stop) = window(y, 1, y_limits);
        let found = search(stage, start, stop, |value| {
            objective.evaluate(index, Axis::Y, value)
        })?;
        y = found.x;
        if found.cost < best_cost {
            best_y = y;
            best_cost = found.cost;
        }
        objective.set(index, Axis::Y, best_y)?;
        if found.cost < stop_when {
            break;
        }
        // Start over from the best.
        (x, y) = (best_x, best_y);
    }
    Ok(Best2d {
        x: best_x,
        y: best_y,
        cost: best_cost,
    })
}

/// Runs `stage`'s one-dimensional search.
fn search<E>(
    stage: &Stage,
    start: i32,
    stop: i32,
    cost: impl FnMut(i32) -> Result<i32, E>,
) -> Result<Minimum, E> {
    match stage.search {
        Search::Iterate => iterate_min(start, stop, stage.extra_step, cost),
        Search::Golden => golden_min(start, stop, cost),
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
