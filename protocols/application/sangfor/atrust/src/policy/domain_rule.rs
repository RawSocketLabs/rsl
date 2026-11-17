//! A rule that covers host names.

// --- Standard library ---
use std::sync::Arc;

// --- Internal modules ---
use super::{Ports, Protocol, Target, Transport};

/// A rule for a host name and the names beneath it, or, when it came from a `*.` wildcard,
/// for the names beneath a suffix only (spec §6.2, §6.5).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct DomainRule {
    /// The normalised name. A leading dot marks a wildcard-derived suffix.
    pub name: String,
    /// The ports covered.
    pub ports: Ports,
    /// The protocol covered.
    pub protocol: Protocol,
    /// The application the rule belongs to, shared by all of its rules.
    pub target: Arc<Target>,
}

impl DomainRule {
    /// The form in which rule names and queried names are compared: surrounding white space
    /// removed, letters lower-cased, one trailing dot removed (spec §6.5).
    ///
    /// Each letter is lower-cased on its own, so a name's form does not depend on what
    /// stands before or after it.
    #[must_use]
    pub fn normalise(name: &str) -> String {
        let mut name: String = name.trim().chars().flat_map(char::to_lowercase).collect();
        if name.ends_with('.') {
            name.pop();
        }
        name
    }

    /// Whether the rule's name selects `name`, which must already be normalised.
    ///
    /// A plain name selects itself and every name beneath it. A suffix selects the names
    /// beneath it and not the bare name.
    #[must_use]
    pub fn selects(&self, name: &str) -> bool {
        let Some(front) = name.strip_suffix(self.name.as_str()) else {
            return false;
        };
        front.is_empty() || self.name.starts_with('.') || front.ends_with('.')
    }

    /// Whether the rule applies to a destination reached over `transport` on `port`. ICMP
    /// has no ports, so the port range is not consulted for it.
    #[must_use]
    pub fn admits(&self, transport: Transport, port: u16) -> bool {
        self.protocol.admits(transport)
            && (transport == Transport::Icmp || self.ports.contains(port))
    }
}
