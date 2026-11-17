//! The port range of a rule.

/// The ports a rule covers: an address's `port` member, a single number or `a-b`
/// (spec §6.2). A range may contain no port at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ports {
    /// The lowest port covered.
    min: u16,
    /// The highest port covered. Below `min` when the range is empty.
    max: u16,
}

impl Ports {
    /// A range that contains no port.
    pub const EMPTY: Self = Self { min: 1, max: 0 };

    /// The ports from `min` to `max` inclusive: [`Self::EMPTY`] when `min` is above `max`, so
    /// that every range with no port in it is the same value.
    #[must_use]
    pub const fn new(min: u16, max: u16) -> Self {
        if min > max {
            return Self::EMPTY;
        }
        Self { min, max }
    }

    /// Read a `port` value, or `None` when it is malformed and the address is to be skipped.
    ///
    /// The numbers are not range-checked by the protocol: a lower bound above the upper one,
    /// or above 65535, gives a range with no port in it, and an upper bound above 65535
    /// reaches the last port.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let (low, high) = match text.split_once('-') {
            Some((low, high)) => (decimal(low)?, decimal(high)?),
            None => (decimal(text)?, decimal(text)?),
        };
        let Ok(min) = u16::try_from(low) else {
            return Some(Self::EMPTY);
        };
        Some(Self::new(min, u16::try_from(high).unwrap_or(u16::MAX)))
    }

    /// Whether `port` is in the range.
    #[must_use]
    pub const fn contains(self, port: u16) -> bool {
        self.min <= port && port <= self.max
    }

    /// Whether the range contains no port.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.min > self.max
    }
}

/// A number written as decimal digits and nothing else: no sign, no white space. A number
/// too large to hold is the largest that can be held, which is past every bound it is
/// compared with.
pub(crate) fn decimal(text: &str) -> Option<u64> {
    let digits_only = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    digits_only.then(|| text.parse().unwrap_or(u64::MAX))
}
