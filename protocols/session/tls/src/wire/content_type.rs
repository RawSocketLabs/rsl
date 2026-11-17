//! The record layer's content-type registry.

// --- Workspace dependencies ---
use bnb::BitEnum;

/// Which higher-level protocol a record's fragment belongs to (RFC 5246 §6.2.1). Dual-use:
/// any other value is preserved as [`ContentType::Other`].
//~ models rfc5246#6.2.1
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum ContentType {
    /// The single-byte message that switches to the pending cipher state.
    ChangeCipherSpec = 20,
    /// A warning or fatal alert.
    Alert = 21,
    /// Handshake messages.
    Handshake = 22,
    /// Application bytes.
    ApplicationData = 23,
    /// Any other content type, preserved verbatim.
    #[catch_all]
    Other(u8),
}
