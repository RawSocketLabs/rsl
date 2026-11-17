//! The two-byte protocol version carried by records and hello messages.

// --- Workspace dependencies ---
use bnb::bin;

/// A protocol version as `{ major, minor }` (RFC 5246 §6.2.1). Dual-use: any pair
/// round-trips, so a caller can offer a version this crate does not implement.
//~ models rfc5246#6.2.1
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProtocolVersion {
    /// Major version; 3 for every SSL 3.0 and TLS version.
    pub major: u8,
    /// Minor version.
    pub minor: u8,
}

impl ProtocolVersion {
    /// TLS 1.0, `{ 3, 1 }`.
    pub const TLS_1_0: Self = Self::new(3, 1);
    /// TLS 1.1, `{ 3, 2 }`.
    pub const TLS_1_1: Self = Self::new(3, 2);
    /// TLS 1.2, `{ 3, 3 }`.
    pub const TLS_1_2: Self = Self::new(3, 3);

    /// Any version pair (no validation).
    #[must_use]
    pub const fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }
}
