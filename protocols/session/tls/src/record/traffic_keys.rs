//! The pair of record keys a connection uses.

// --- Internal modules ---
use super::Aes128GcmProtection;

/// Both directions' record protection for one connection. Each end seals with its own write
/// state and opens with the other's.
pub struct TrafficKeys {
    /// Protects records the client sends.
    pub client_write: Aes128GcmProtection,
    /// Protects records the server sends.
    pub server_write: Aes128GcmProtection,
}
