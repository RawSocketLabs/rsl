//! The named-group (elliptic-curve) registry.

// --- Workspace dependencies ---
use bnb::BitEnum;

/// A key-exchange group (RFC 8422 §5.1.1, `NamedCurve`). Dual-use: any other value is
/// preserved as [`NamedGroup::Other`].
//~ models rfc8422#5.1.1
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = big)]
#[repr(u16)]
pub enum NamedGroup {
    /// NIST P-256.
    Secp256r1 = 23,
    /// NIST P-384.
    Secp384r1 = 24,
    /// NIST P-521.
    Secp521r1 = 25,
    /// Curve25519.
    X25519 = 29,
    /// Curve448.
    X448 = 30,
    /// Any other group, preserved verbatim.
    #[catch_all]
    Other(u16),
}
