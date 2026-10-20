//! Values the admissible evidence does not give yet.
//!
//! A [`Pending`] stands where a typed value (a register address, request code or
//! identifier) will go once a capture or descriptor dump supplies it. It holds no value, so nothing can
//! use a guess: [`Pending::get`] always refuses with the [`Gap`] naming the ledger row
//! (`PROVENANCE.md`) that records what is missing. When the evidence arrives the constant
//! becomes a plain value of type `T`, and each `.get()?` at its callers becomes that value.

use core::fmt;
use core::marker::PhantomData;

/// A value of type `T` that is not yet evidenced; see the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pending<T> {
    /// The ledger row recording the gap.
    gap: Gap,

    /// The type the evidence will supply.
    value: PhantomData<fn() -> T>,
}

impl<T> Pending<T> {
    /// A placeholder for the value ledger row `id` (class GAP) is waiting on.
    #[must_use]
    pub const fn gap(id: &'static str) -> Self {
        Self {
            gap: Gap::new(id),
            value: PhantomData,
        }
    }

    /// The value, which is not evidenced yet.
    ///
    /// # Errors
    ///
    /// Always the [`Gap`]: the placeholder cannot be used at runtime.
    pub const fn get(&self) -> Result<T, Gap> {
        Err(self.gap)
    }

    /// The ledger row the value is waiting on.
    #[must_use]
    pub const fn ledger_id(&self) -> &'static str {
        self.gap.0
    }
}

/// An operation refused because a value it needs is a GAP row in `PROVENANCE.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gap(&'static str);

impl Gap {
    /// A refusal naming ledger row `id` (class GAP).
    pub(crate) const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// The ledger row, such as `RTL-GAP-07`.
    #[must_use]
    pub const fn ledger_id(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Gap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "not evidenced yet: PROVENANCE.md row {} awaits a capture or document",
            self.0
        )
    }
}

impl core::error::Error for Gap {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pending_value_refuses_with_its_ledger_row() {
        const PLACEHOLDER: Pending<u16> = Pending::gap("TEST-GAP-01");
        assert_eq!(PLACEHOLDER.get(), Err(Gap("TEST-GAP-01")));
        assert_eq!(PLACEHOLDER.ledger_id(), "TEST-GAP-01");
    }
}
