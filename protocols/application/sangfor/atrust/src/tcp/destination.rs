//! Where a tunnel goes, and by which rule.

// --- Standard library ---
use std::net::{IpAddr, Ipv4Addr};

// --- Workspace dependencies ---
use sangfor_atrust_wire::Endpoint;

// --- Internal modules ---
use crate::policy::{DomainRule, NameRoute, Policy, Target};

/// A destination the policy sends through a TCP tunnel, with the values the request for it
/// carries (spec §6.6, §9.2).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Destination {
    /// What the node is asked to connect to: an IPv4 address, or a name it resolves itself.
    pub wire: Endpoint,
    /// The destination as the request names it: `host:port`.
    pub dest_addr: String,
    /// The address the client resolved the name to, when the rule leaves resolving to the
    /// client. It is sent beside the name.
    pub dest_ip: Option<Ipv4Addr>,
    /// The application the chosen rule belongs to.
    pub app_id: String,
    /// The node group that serves it. Empty when the rule names none.
    pub node_group_id: String,
}

impl Destination {
    /// Decide whether a TCP connection to `host:port` goes through a tunnel, and how
    /// (spec §6.6 steps 1 to 5). `None` means the policy does not cover it: the connection is
    /// not the tunnel's to carry.
    ///
    /// `host` is an address literal or a name, as the caller got it. `take_all` is for a
    /// client with no L3 tunnel: it takes TCP that rules would give to L3.
    ///
    /// A name's IPv4 address comes from the policy's pins, the first of them, and otherwise
    /// from `resolve`, which is given the normalised name: the caller's own table, then a
    /// resolver of its choosing. `resolve` is not asked when the policy pins the name, nor
    /// when the chosen rule has the gateway resolve it. Two things here differ from the
    /// reference client, as client policy may: there the caller's own table comes before the
    /// pins, and successive lookups rotate among several pins.
    ///
    /// IPv6 destinations are never covered: the rules are IPv4 only.
    pub fn route(
        policy: &Policy,
        host: &str,
        port: u16,
        take_all: bool,
        resolve: impl FnOnce(&str) -> Option<Ipv4Addr>,
    ) -> Option<Self> {
        let literal = host.trim();
        let literal = literal
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(literal);
        match literal.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => return None,
            Ok(IpAddr::V4(address)) => return Self::by_address(policy, address, port, take_all),
            Err(_) => {}
        }

        let name = DomainRule::normalise(host);
        let resolved = || policy.pinned(&name).next().or_else(|| resolve(&name));
        let rule = match policy.tcp_rule_for_name(&name, port, take_all) {
            NameRoute::PrefersL3 => return None,
            NameRoute::Unmatched => return Self::by_address(policy, resolved()?, port, take_all),
            NameRoute::Rule(rule) => rule,
        };
        // The gateway resolves the name, or the client does and sends both.
        let (wire, dest_ip) = if rule.target.addr_pretend {
            (Endpoint::domain(name.as_bytes(), port).ok()?, None)
        } else {
            let address = resolved()?;
            (Endpoint::addr(address, port), Some(address))
        };
        Some(Self::new(
            wire,
            format!("{name}:{port}"),
            dest_ip,
            &rule.target,
        ))
    }

    /// The destination for an address, by the address rules (spec §6.6 step 5.2). A name that
    /// led here plays no further part.
    fn by_address(policy: &Policy, address: Ipv4Addr, port: u16, take_all: bool) -> Option<Self> {
        let rule = policy.tcp_rule_for_address(address, port, take_all)?;
        let wire = Endpoint::addr(address, port);
        Some(Self::new(
            wire,
            format!("{address}:{port}"),
            None,
            &rule.target,
        ))
    }

    /// A destination under the rule that `target` belongs to.
    fn new(wire: Endpoint, dest_addr: String, dest_ip: Option<Ipv4Addr>, target: &Target) -> Self {
        Self {
            wire,
            dest_addr,
            dest_ip,
            app_id: target.app_id.clone(),
            node_group_id: target.node_group_id.clone(),
        }
    }
}
