//! Machine-readable algorithm lifecycle classifications.
//!
//! Cryptographic correctness and cryptographic suitability are different questions. A historical
//! algorithm can be implemented exactly and still be unsafe for new protection. [`SecurityStatus`]
//! lets documentation, facades, and protocol policy name that distinction without treating a
//! successful computation as permission to negotiate the algorithm.
//!
//! # Policy boundary
//!
//! These values describe the algorithm's broad lifecycle in this project. They are not runtime
//! negotiation policy, certification, an audit result, or a substitute for protocol-specific
//! parameter checks. Protocol crates must use explicit allowlists and must never automatically
//! fall back from a contemporary algorithm to a legacy or broken one.

/// Project-wide lifecycle status attached to cryptographic algorithms and constructions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum SecurityStatus {
    /// Suitable for new protocol design according to the project's current standards baseline.
    ///
    /// This label does not override `rsl-crypto`'s unaudited-library warning.
    Recommended,
    /// Retained only to interoperate with historical systems; prohibited from default negotiation.
    Legacy,
    /// Known practical attacks invalidate an intended security property.
    Broken,
    /// Exposed to teach mechanics or failures, not to protect real data.
    EducationalOnly,
}

/// A type whose algorithm lifecycle status can be inspected without constructing it.
pub trait SecurityClassification {
    /// Current project classification for this algorithm or construction.
    const SECURITY_STATUS: SecurityStatus;
}
