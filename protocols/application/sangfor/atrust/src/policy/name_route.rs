//! The answer to "does this name go through a TCP tunnel".

// --- Internal modules ---
use super::DomainRule;

/// What the domain rules say about a TCP connection to a host name (spec §6.6 step 5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameRoute<'a> {
    /// This rule carries the connection through a TCP tunnel.
    Rule(&'a DomainRule),
    /// A rule covers the name but gives its TCP to the L3 tunnel. The connection is not for
    /// a TCP tunnel, and the address rules are not consulted.
    PrefersL3,
    /// No domain rule covers the name and port. The address rules decide, once the name is
    /// resolved.
    Unmatched,
}
