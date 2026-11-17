//! The server's ephemeral ECDH key-exchange message.

// --- Workspace dependencies ---
use bnb::{BitEnum, bin};

// --- Internal modules ---
use super::{NamedGroup, SignatureScheme};

/// How the server describes its curve (RFC 8422 §5.4). Only `named_curve` remains in use;
/// any other value is preserved as [`EcCurveType::Other`].
//~ models rfc8422#5.4
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum EcCurveType {
    /// The curve is identified by a [`NamedGroup`].
    NamedCurve = 3,
    /// Any other curve type, preserved verbatim.
    #[catch_all]
    Other(u8),
}

/// The server's ephemeral ECDH public key and its group (RFC 8422 §5.4, `ServerECDHParams`
/// with a named curve).
///
/// The fields after `curve_type` are always read with the named-curve layout, so they mean
/// what their names say only when `curve_type` is [`EcCurveType::NamedCurve`].
///
/// Its encoding is the last part of what the server signs, after the two hello randoms, so
/// [`to_bytes`](Self::to_bytes) of a decoded value must reproduce the received bytes; it does,
/// because every field is stored verbatim.
//~ models rfc8422#5.4
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerEcdhParams {
    /// How the curve is described.
    pub curve_type: EcCurveType,
    /// The group the key is on.
    pub named_group: NamedGroup,
    /// The public key: an uncompressed point for the NIST curves, the 32- or 56-byte
    /// u-coordinate for X25519 and X448.
    #[brw(count_prefix = u8)]
    pub public: Vec<u8>,
}

/// The body of a `ServerKeyExchange` for an ECDHE suite (RFC 8422 §5.4): the parameters and
/// the server's signature over them (RFC 5246 §4.7, `DigitallySigned`).
///
/// A body for another key exchange does not have this layout; decode this type only after the
/// negotiated suite says the exchange is ECDHE.
//~ models rfc8422#5.4
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerKeyExchange {
    /// The server's ephemeral key and group.
    pub params: ServerEcdhParams,
    /// The algorithm the signature was made with.
    pub algorithm: SignatureScheme,
    /// The signature over the client random, the server random and `params`.
    #[brw(count_prefix = u16)]
    pub signature: Vec<u8>,
}
